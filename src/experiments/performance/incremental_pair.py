"""Interleaved native A/B for one Python body edit; no agents/network/global state."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import statistics
import sqlite3
import subprocess
import sys
import tempfile

from autosync_eval import HarnessError, bounded_read, isolated_environment, native_resources, run_json, sha256
from compare_generations import canonical_bytes, fingerprint

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "src/experiments/agent_study"))
from treehash import tree_sha256

WARMUPS = 2
PAIRS = 9
MODULES = 16
FIXTURE_FILES = ("app.py", "conftest.py", "test_accounts.py", "pyproject.toml", "helper.rs", "router.ts")
COUNTERS = ("discovered_files", "parser_attempted_files", "indexed_units", "semantic_facts",
            "modified_files", "unchanged_files", "copied_forward_files", "reparsed_files",
            "families_recomputed", "dirty_records_cleared")


def variant(template, changed):
    if template.count(b"return []") != 3:
        raise HarnessError("frozen_body_patch_anchor_changed")
    return template.replace(b"return []", b"return [1]", 1) if changed else template


def prepare_fixture(fixture, repo):
    if {path.name for path in fixture.iterdir()} != set(FIXTURE_FILES):
        raise HarnessError("unqualified_synthetic_fixture_shape")
    repo.mkdir()
    for name in FIXTURE_FILES:
        source = fixture / name
        if source.is_symlink() or not source.is_file() or source.stat().st_size > 65_536:
            raise HarnessError("unqualified_synthetic_fixture_file")
        shutil.copyfile(source, repo / name)
    template = (repo / "app.py").read_bytes()
    variant(template, False)
    for index in range(1, MODULES):
        (repo / f"module_{index:04}.py").write_bytes(template)
    return template


def ordered_arms(pair):
    return ("baseline", "candidate") if pair % 2 == 0 else ("candidate", "baseline")


def qualified_outcome(value):
    if (value.get("status") != "complete" or value.get("sync_mode") != "incremental" or value.get("fallback_reason") is not None
            or value.get("modified_files") != 1 or value.get("reparsed_files") != 1):
        raise HarnessError("body_edit_did_not_remain_one_file_incremental")
    return {**{key: value.get(key) for key in COUNTERS},
            "sync_mode": value["sync_mode"], "fallback_reason": value.get("fallback_reason")}


def parity(left, right):
    if left != right:
        raise HarnessError("complete_active_analysis_inequivalent")
    return hashlib.sha256(canonical_bytes(left)).hexdigest()


def summary(pairs):
    result = {}
    for metric in ("wall_seconds", "cpu_seconds", "peak_rss_bytes"):
        left = [pair["arms"]["baseline"]["resources"][metric] for pair in pairs]
        right = [pair["arms"]["candidate"]["resources"][metric] for pair in pairs]
        result[metric] = {"baseline_median": statistics.median(left),
                          "candidate_median": statistics.median(right),
                          "paired_delta_median": statistics.median(b - a for a, b in zip(left, right)),
                          "paired_ratio_median": statistics.median(b / a for a, b in zip(left, right)) if all(a > 0 for a in left) else None}
    return result


def evaluate(args):
    pins = {"harness_sha256": sha256(__file__), "fixture_sha256": tree_sha256(str(args.fixture)),
            "python_sha256": sha256(args.python)}
    for arm in ("baseline", "candidate"):
        pins[arm] = {"binary_sha256": sha256(getattr(args, arm + "_bin")),
                     "worker_sha256": sha256(getattr(args, arm + "_worker"))}
    args.out.mkdir(parents=True, exist_ok=True)
    if any(args.out.iterdir()):
        raise HarnessError("refusing_nonempty_evidence_directory")
    report = {"schema_version": "incremental-pair-results.v1", "state": "FAIL",
              "pins": pins, "warmup_pairs": WARMUPS, "measured_pairs": PAIRS,
              "synthetic_modules": MODULES, "samples": [],
              "resource_scope": "direct product binary under native time; initialization, edits and comparisons outside timing",
              "worker_spawns": {"state": "NOT_MEASURED", "reason": "native runner has no worker observer"},
              "filesystem_bytes_read": {"state": "NOT_MEASURED", "reason": "native runner has no filesystem observer"}}
    try:
        with tempfile.TemporaryDirectory(prefix="repogrammar-incremental-pair-") as temporary:
            root = Path(temporary).resolve()
            arms = {}
            for name in ("baseline", "candidate"):
                directory = root / name
                directory.mkdir()
                repo = directory / "repo"
                template = prepare_fixture(args.fixture, repo)
                environment = isolated_environment(directory, args.python, getattr(args, name + "_worker"))
                environment["REPOGRAMMAR_PYTHON_PROJECT_SESSION"] = "1" if name == "candidate" else "0"
                git = subprocess.run([str(directory / "tools/git"), "-c", "core.hooksPath=" + str(Path("/dev/null")), "init", "-q"],
                                     cwd=repo, env=environment, capture_output=True, timeout=10)
                if git.returncode:
                    raise HarnessError("isolated_git_init_failed")
                source_hash = tree_sha256(str(repo))
                files = sorted(path for path in repo.rglob("*") if path.is_file() and ".git" not in path.relative_to(repo).parts)
                binary = getattr(args, name + "_bin")
                run_json([str(binary), "init", "--project", str(repo), "--yes", "--no-autosync", "--json", "--progress", "never"],
                         environment, repo, directory, "init")
                arms[name] = {"directory": directory, "repo": repo, "template": template,
                              "environment": environment, "binary": binary, "source_sha256": source_hash}
                inventory = {"files": len(files), "python_files": sum(path.suffix == ".py" for path in files),
                             "source_bytes": sum(path.stat().st_size for path in files),
                             "python_source_bytes": sum(path.stat().st_size for path in files if path.suffix == ".py")}
                if name == "baseline":
                    report["expanded_source_sha256"] = source_hash
                    report["source_inventory"] = inventory
                elif source_hash != report["expanded_source_sha256"] or inventory != report["source_inventory"]:
                    raise HarnessError("initial_source_mismatch")
            parity(*(fingerprint(arms[name]["repo"] / ".repogrammar/repogrammar.sqlite") for name in ("baseline", "candidate")))
            for index in range(WARMUPS + PAIRS):
                changed = index % 2 == 0
                content = variant(arms["baseline"]["template"], changed)
                for arm in arms.values():
                    (arm["repo"] / "app.py").write_bytes(content)
                pair = {"pair": index, "warmup": index < WARMUPS, "order": ordered_arms(index),
                        "changed_file_sha256": hashlib.sha256(content).hexdigest(), "arms": {}}
                for name in pair["order"]:
                    arm = arms[name]
                    native_path = args.out / f"{index:02}-{name}.native.txt"
                    command = ["/usr/bin/time", "-l" if sys.platform == "darwin" else "-v", "-o", str(native_path),
                               str(arm["binary"]), "sync", "--project", str(arm["repo"]), "--json", "--progress", "never"]
                    value = run_json(command, arm["environment"], arm["repo"], args.out, f"{index:02}-{name}")
                    counters = qualified_outcome(value)
                    resources = native_resources(bounded_read(native_path).decode("utf-8"), sys.platform)
                    if resources["state"] != "MEASURED":
                        raise HarnessError("native_resources_unavailable")
                    resources["cpu_seconds"] = resources["user_cpu_seconds"] + resources["system_cpu_seconds"]
                    pair["arms"][name] = {"counters": counters, "resources": resources}
                pair["active_analysis_sha256"] = parity(*(fingerprint(arms[name]["repo"] / ".repogrammar/repogrammar.sqlite") for name in ("baseline", "candidate")))
                report["samples"].append(pair)
        for name in ("baseline", "candidate"):
            if pins[name] != {"binary_sha256": sha256(getattr(args, name + "_bin")), "worker_sha256": sha256(getattr(args, name + "_worker"))}:
                raise HarnessError("producer_changed_during_measurement")
        if pins["fixture_sha256"] != tree_sha256(str(args.fixture)) or pins["python_sha256"] != sha256(args.python):
            raise HarnessError("input_changed_during_measurement")
        report.update(state="MEASURED", summary=summary(report["samples"][WARMUPS:]))
    except (HarnessError, ValueError, OSError, sqlite3.Error, subprocess.SubprocessError) as error:
        report["failure_reason"] = str(error) if isinstance(error, HarnessError) else "native_pair_environment_or_comparison_failure"
    (args.out / "incremental-pair.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for field in ("baseline-bin", "baseline-worker", "candidate-bin", "candidate-worker", "python"):
        parser.add_argument("--" + field, type=Path, required=True)
    parser.add_argument("--fixture", type=Path, default=ROOT / "src/fixtures/evaluation/performance")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if sys.platform not in ("darwin", "linux"):
        parser.error("native macOS/Linux required")
    for field in ("baseline_bin", "baseline_worker", "candidate_bin", "candidate_worker", "python", "fixture"):
        setattr(args, field, getattr(args, field).resolve(strict=True))
    args.out = args.out.resolve()
    if args.out == ROOT or ROOT in args.out.parents:
        parser.error("evidence output must be outside the repository")
    report = evaluate(args)
    print("incremental pair: " + report["state"])
    raise SystemExit(0 if report["state"] == "MEASURED" else 1)
