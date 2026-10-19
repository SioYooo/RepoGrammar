"""Capture product read surfaces for every committed fixture root.

Run once with the baseline binary and once with the candidate, passing the
baseline `capture.json` to --compare. Each root is indexed in an isolated
workspace outside the checkout; only the per-generation identifiers listed in
DROPPED_KEYS are removed before hashing, so any other byte difference counts.
The full owned-table generation fingerprint reuses compare_generations.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "performance"))
from compare_generations import fingerprint

DROPPED_KEYS = frozenset({"active_generation", "generation_id"})
SURFACES = ("files", "units", "families", "unknowns")


def fixture_roots(fixtures):
    roots = sorted(path for path in fixtures.glob("*/release/*/*") if path.is_dir())
    roots += sorted(path for path in (fixtures / "unknown_reduction").iterdir() if path.is_dir())
    roots += sorted(path for path in (fixtures / "evaluation").iterdir() if path.is_dir())
    roots.append(fixtures / "incremental_equivalence/v1")
    return roots


def canonical(value):
    if isinstance(value, dict):
        return {key: canonical(item) for key, item in value.items() if key not in DROPPED_KEYS}
    if isinstance(value, list):
        return [canonical(item) for item in value]
    return value


def environment(home, python, worker):
    tools = home.parent / "tools"
    tools.mkdir()
    for name in ("git", "ps", "kill"):
        (tools / name).symlink_to(shutil.which(name))
    (tools / "python3").symlink_to(python)
    return {"HOME": str(home), "XDG_CONFIG_HOME": str(home / ".config"), "XDG_DATA_HOME": str(home / ".local/share"),
            "XDG_CACHE_HOME": str(home / ".cache"), "CODEX_HOME": str(home / ".codex"), "PATH": str(tools),
            "LC_ALL": "C", "REPOGRAMMAR_PYTHON_EXECUTABLE": str(python), "REPOGRAMMAR_PYTHON_WORKER": str(worker),
            "REPOGRAMMAR_TELEMETRY": "0", "DO_NOT_TRACK": "1",
            "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_SYSTEM": os.devnull}


def mcp_request(arguments):
    rows = [{"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {}},
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": 1, "method": "tools/call",
             "params": {"name": "repogrammar_context", "arguments": arguments}}]
    return ("\n".join(json.dumps(row) for row in rows) + "\n").encode()


def mcp_payload(stdout):
    rows = [json.loads(line) for line in stdout.splitlines() if line.strip()]
    response, = [row for row in rows if row.get("id") == 1]
    return json.loads(response["result"]["content"][0]["text"])


def run(binary, env, project, arguments, request=None):
    completed = subprocess.run([str(binary), *arguments, "--project", str(project)], env=env, cwd=project,
                               input=request, capture_output=True, timeout=120)
    return completed.returncode, completed.stdout


def record(rows, key, code, value):
    text = json.dumps(canonical(value), sort_keys=True, separators=(",", ":"))
    rows[key] = {"exit_code": code, "sha256": hashlib.sha256(text.encode()).hexdigest(), "canonical": text}
    return value


def capture_root(binary, python, worker, repo, root, scratch):
    rows = {}
    workspace = Path(tempfile.mkdtemp(prefix="language-pack-capture-", dir=scratch))
    try:
        home = workspace / "home"
        home.mkdir()
        project = workspace / "project"
        shutil.copytree(root, project)
        env = environment(home, python, worker)
        for name, arguments in (("init", ["init", "--yes", "--state-only", "--json", "--progress", "never"]),
                                ("resync", ["resync", "--json", "--progress", "never"])):
            code, stdout = run(binary, env, project, arguments)
            record(rows, name, code, json.loads(stdout) if stdout.strip() else None)
        record(rows, "generation", 0, fingerprint(project / ".repogrammar/repogrammar.sqlite"))
        listed = {}
        for surface in SURFACES:
            code, stdout = run(binary, env, project, [surface, "--json"])
            listed[surface] = record(rows, surface, code, json.loads(stdout))
        for family in listed["families"].get("families", []):
            family_id = family["family_id"]
            code, stdout = run(binary, env, project, ["family", family_id, "--json", "--mode", "deep"])
            record(rows, f"family:{family_id}", code, json.loads(stdout))
            code, stdout = run(binary, env, project, ["serve"],
                               mcp_request({"operation": "show_family", "target": family_id, "mode": "deep"}))
            record(rows, f"mcp_family:{family_id}", code, mcp_payload(stdout))
        for unit in listed["units"].get("units", []):
            code, stdout = run(binary, env, project, ["member", unit["id"], "--json", "--mode", "deep"])
            record(rows, f"member:{unit['id']}", code, json.loads(stdout))
        for path in sorted({item["path"] for item in listed["files"].get("files", [])}):
            for command in ("find", "check", "explain"):
                code, stdout = run(binary, env, project, [command, path, "--json"])
                record(rows, f"{command}:{path}", code, json.loads(stdout))
            code, stdout = run(binary, env, project, ["serve"],
                               mcp_request({"operation": "find_analogues", "target": path, "mode": "compact"}))
            record(rows, f"mcp_find:{path}", code, mcp_payload(stdout))
    finally:
        shutil.rmtree(workspace)
    return rows


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--bin", type=Path, required=True)
    parser.add_argument("--python", type=Path, required=True)
    parser.add_argument("--scratch", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--compare", type=Path)
    args = parser.parse_args(argv)
    repo, worker = args.repo.resolve(), args.repo.resolve() / "src/workers/python/worker.py"
    if args.scratch.resolve().is_relative_to(repo):
        raise SystemExit("scratch directory must be outside the checkout")
    roots = {}
    for root in fixture_roots(repo / "src/fixtures"):
        roots[str(root.relative_to(repo))] = capture_root(args.bin.resolve(), args.python, worker, repo, root,
                                                          args.scratch.resolve())
    report = {"schema_version": "language-pack-capture.v1",
              "binary_sha256": hashlib.sha256(args.bin.read_bytes()).hexdigest(),
              "worker_sha256": hashlib.sha256(worker.read_bytes()).hexdigest(),
              "harness_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "dropped_keys": sorted(DROPPED_KEYS), "roots": roots,
              "row_count": sum(len(rows) for rows in roots.values())}
    mismatches = []
    if args.compare:
        baseline = json.loads(args.compare.read_text())["roots"]
        for root in sorted(roots.keys() | baseline.keys()):
            left, right = baseline.get(root, {}), roots.get(root, {})
            mismatches += [f"{root} {key}" for key in sorted(left.keys() | right.keys())
                           if {k: v for k, v in left.get(key, {}).items() if k != "canonical"}
                           != {k: v for k, v in right.get(key, {}).items() if k != "canonical"}]
        report["mismatches"] = mismatches
    args.out.write_text(json.dumps(report, indent=1, sort_keys=True) + "\n")
    print(json.dumps({"roots": len(roots), "rows": report["row_count"], "mismatches": len(mismatches)}))
    return 1 if mismatches else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
