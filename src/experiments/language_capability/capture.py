"""Capture raw product responses and preservation evidence; repo-guard owns scoring."""

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import signal
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE.parent / "performance"))
sys.path.insert(0, str(HERE.parent / "agent_study"))
from adoption_v2 import _object, _unique_object, current_mcp
from compare_generations import canonical_bytes, fingerprint
from native_host_v2 import run_owned, stop_owned_group
from treehash import tree_sha256
from warm_queries import read_frame, require_source_free
from prepare import PREREGISTRATION_SHA256

LIMIT = 1024 * 1024
SENTINEL = "RG_SOURCE_SENTINEL_qualified_phrase_20261014"
CONTROLS = json.loads((HERE / "qualification-controls.json").read_text())


def sha(data):
    return hashlib.sha256(data).hexdigest()


def parity(old, candidate):
    if old == candidate:
        return {"equal":True, "metadata_paths":[]}
    left = json.loads(old, object_pairs_hook=_unique_object)
    right = json.loads(candidate, object_pairs_hook=_unique_object)
    normalized, changes = copy.deepcopy(right), []
    policy = CONTROLS["allowed_unchanged_query_metadata_changes"]
    for path in policy["json_paths"]:
        a, b = left, normalized
        for key in path[:-1]:
            a, b = a.get(key, {}) if isinstance(a, dict) else {}, b.get(key, {}) if isinstance(b, dict) else {}
        key = path[-1]
        if isinstance(a, dict) and isinstance(b, dict) and a.get(key) == policy["old_value"] and b.get(key) == policy["candidate_value"]:
            b[key] = policy["old_value"]
            changes.append(path)
    before, after = json.dumps(policy["old_value"]).encode(), json.dumps(policy["candidate_value"]).encode()
    equal = (left == normalized and candidate.count(after) == len(changes)
             and candidate.replace(after, before) == old)
    return {"equal":equal, "metadata_paths":changes}


def selected_ids(payload):
    identities = {value for value in (
        payload.get("selected_family_id"), _object(payload, "family").get("family_id"),
        _object(payload, "query_route").get("selected_family_id"),
        _object(payload, "certificate").get("selected_family_id"),
        _object(payload, "certificate").get("family_id"),
        _object(payload, "alignment_certificate").get("selected_family_id"),
        _object(payload, "alignment_certificate").get("family_id"),
    ) if value is not None}
    if any(not isinstance(value, str) or not value for value in identities):
        raise ValueError("malformed selected family identity")
    if len(identities) > 1:
        raise ValueError("conflicting selected family identities")
    return identities


def true_abstention(payload, returncode, indexed):
    return (indexed and returncode == 0 and payload.get("status") == "UNKNOWN"
            and bool(payload.get("unknowns")) and not selected_ids(payload)
            and all(isinstance(u, dict) and isinstance(u.get("reason"), str) for u in payload["unknowns"]))


def qualify_outcome(payload, returncode, abstention_gold):
    if returncode or payload.get("status") not in ("ok", "UNKNOWN", "PARTIAL_CONTEXT"):
        raise ValueError("query failed operationally, not a valid UNKNOWN")
    if (abstention_gold or payload.get("status") == "UNKNOWN") and not true_abstention(payload, returncode, True):
        raise ValueError("abstention gold selected a family or failed")


def validate_payload(raw, project, stale_path=None):
    if len(raw) > LIMIT or SENTINEL.encode() in raw or str(project).encode() in raw:
        raise ValueError("oversized or source-bearing response")
    payload = json.loads(raw, object_pairs_hook=_unique_object)
    require_source_free(payload)
    current_mcp(raw.decode(), str(project))
    selected = selected_ids(payload)
    if "read_plan" not in payload:
        if selected:
            raise ValueError("selected family has no usable source read plan")
        return {"items":0, "stale_items":0}
    items = _object(payload, "read_plan").get("items", [])
    if selected and not items:
        raise ValueError("selected family has an empty source read plan")
    if len(items) > 64:
        raise ValueError("read-plan item bound exceeded")
    stale = (stale_path is not None and payload.get("status") == "UNKNOWN" and not selected
             and any(u.get("reason") == "StaleEvidence" for u in payload.get("unknowns", [])))
    stale_count = 0
    for item in items:
        path = PurePosixPath(item["path"])
        if path.is_absolute() or ".." in path.parts or "\\" in str(path) or ":" in str(path):
            raise ValueError("read plan escaped its project")
        source = project.joinpath(*path.parts)
        if source.is_symlink() or not source.is_file() or project not in source.resolve().parents:
            raise ValueError("read plan does not name a regular project file")
        data = source.read_bytes()
        start, end = item["start_byte"], item["end_byte"]
        if not (type(start) is int and type(end) is int and 0 <= start <= end <= len(data)):
            raise ValueError("read plan has invalid source bounds")
        if item.get("source_snippets_included") is not False:
            raise ValueError("read plan includes source")
        if not isinstance(item.get("content_hash"), str) or not re.fullmatch(r"sha256:[0-9a-f]{64}", item["content_hash"]):
            raise ValueError("read plan has a malformed content hash")
        if item["content_hash"] != "sha256:" + sha(data):
            if not stale or item["path"] != stale_path:
                raise ValueError("fresh read plan content hash mismatch")
            stale_count += 1
        if item.get("start_line") is not None:
            if not 1 <= item["start_line"] <= item["end_line"] <= data.count(b"\n") + 1:
                raise ValueError("read plan has invalid line bounds")
    return {"items":len(items), "stale_items":stale_count}


def environment(cell):
    home, tools, temporary = cell / "home", cell / "tools", cell / "tmp"
    for directory in (home, tools, temporary):
        directory.mkdir(parents=True)
    for name in ("git", "ps", "kill"):
        source = shutil.which(name)
        if source:
            (tools / name).symlink_to(source)
    (tools / "python3").symlink_to(Path(sys.executable).resolve())
    return {"HOME":str(home), "CODEX_HOME":str(home / ".codex"),
            "XDG_CONFIG_HOME":str(home / ".config"), "XDG_DATA_HOME":str(home / ".local/share"),
            "XDG_CACHE_HOME":str(home / ".cache"), "PATH":str(tools), "TMPDIR":str(temporary),
            "REPOGRAMMAR_PYTHON_EXECUTABLE":str(tools / "python3"),
            "REPOGRAMMAR_TELEMETRY":"0", "DO_NOT_TRACK":"1", "LC_ALL":"C"}


def command(binary, project, env, arguments, prefix):
    invocation = [str(binary), *arguments, "--project", str(project), "--json"]
    stdout, stderr = prefix.with_suffix(".stdout"), prefix.with_suffix(".stderr")
    code, status, cleanup = run_owned(invocation, project, env, "", stdout, stderr, timeout=180)
    if status != "COMPLETE" or cleanup != "PASS":
        raise ValueError("product command timed out or left an owned process group")
    if stdout.stat().st_size > 16 * LIMIT or stderr.stat().st_size > LIMIT:
        raise ValueError("product command output bound exceeded")
    return subprocess.CompletedProcess(invocation, code, stdout.read_bytes(), stderr.read_bytes())


def mcp(binary, project, env, arguments, prefix):
    with prefix.with_suffix(".stderr").open("wb") as errors:
        process = subprocess.Popen([str(binary), "serve", "--project", str(project)], env=env,
                                   cwd=project, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=errors, start_new_session=True)
        pending = bytearray()
        def rpc(identifier, method, params):
            message = {"jsonrpc":"2.0", "method":method, "params":params}
            if identifier is not None:
                message["id"] = identifier
            data = (json.dumps(message, separators=(",", ":")) + "\n").encode()
            if len(data) > 4096:
                raise ValueError("request exceeds bounded pipe write")
            process.stdin.write(data)
            process.stdin.flush()
            if identifier is None:
                return None
            raw = read_frame(process.stdout.fileno(), pending, time.monotonic() + 30)
            value = json.loads(raw, object_pairs_hook=_unique_object)
            if value.get("id") != identifier or "error" in value:
                raise ValueError("MCP protocol error")
            return value
        try:
            rpc(1, "initialize", {"protocolVersion":"2025-06-18", "capabilities":{}, "clientInfo":{"name":"phrase-qualification", "version":"1"}})
            rpc(None, "notifications/initialized", {})
            reply = rpc(2, "tools/call", {"name":"repogrammar_context", "arguments":arguments})
            if reply["result"].get("isError"):
                raise ValueError("MCP product error")
            raw = reply["result"]["content"][0]["text"].encode()
            prefix.with_suffix(".stdout").write_bytes(raw)
            rpc(3, "shutdown", {})
            process.stdin.close()
            process.wait(timeout=10)
            if process.returncode:
                raise ValueError("MCP process failed")
            return raw
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)
            if stop_owned_group(process.pid) != "PASS":
                raise ValueError("MCP left an owned process group")
            process.stdin.close()
            process.stdout.close()


def initialize(binary, source, cell, malformed=False, sentinel=False):
    env = environment(cell)
    project = cell / "project"
    shutil.copytree(source, project, symlinks=True)
    if sentinel:
        with next(project.rglob("*.py")).open("a") as output:
            output.write(f"\n# {SENTINEL}\n")
    if malformed:
        (project / "malformed.py").write_text(f"def broken(:\n    # {SENTINEL}\n")
    source_identity = tree_sha256(str(project))
    for name, arguments in (("init", ["init", "--state-only"]), ("resync", ["resync", "--progress", "never"])):
        result = command(binary, project, env, arguments, cell / name)
        if result.returncode or json.loads(result.stdout).get("status") not in ("initialized", "complete"):
            raise ValueError(f"{name} failed; no query may count as a valid abstention")
    return project, env, source_identity


def verify_public_manifest(prereg, public, sources, provenance):
    if provenance.get("preregistration_sha256") != PREREGISTRATION_SHA256:
        raise ValueError("public provenance refers to a different preregistration")
    if len(public["queries"]) != len(prereg["prospective_queries"]):
        raise ValueError("public query denominator drift")
    for query, frozen in zip(public["queries"], prereg["prospective_queries"]):
        expected = {"outcome":frozen["expected"]}
        if "family_prefix" in frozen:
            expected["family_prefix"] = frozen["family_prefix"]
        if (query["query_id"], query["fixture_id"], query["target"], query["intent"], query["expected"], query["operation"], query["mode"]) != (
                frozen["id"], frozen["corpus"], frozen["target"], frozen["intent"], expected, "find", "compact"):
            raise ValueError("public query or gold differs from the preregistration")
    identities = {row["corpus_id"]:row for row in provenance["corpora"]}
    required = {row["id"] for row in prereg["corpora"]}
    if (set(identities) != required or len(identities) != len(provenance["corpora"])
            or len(public["fixtures"]) != len(required)
            or {row["fixture_id"] for row in public["fixtures"]} != required):
        raise ValueError("public source selection drift")
    for frozen in prereg["corpora"]:
        recorded = identities[frozen["id"]]
        if (recorded.get("url"), recorded.get("commit")) != (frozen["url"], frozen["commit"]):
            raise ValueError("public source commit or URL drift")
    for fixture in public["fixtures"]:
        source = sources / fixture["fixture_id"] / "snapshot"
        if Path(fixture["root"]).resolve() != source.resolve() or tree_sha256(str(source)) != identities[fixture["fixture_id"]]["fixture_version"]:
            raise ValueError("public snapshot identity drift")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--old", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--sources", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    prereg_raw = (HERE / "preregistration.json").read_bytes()
    if sha(prereg_raw) != PREREGISTRATION_SHA256:
        parser.error("frozen task manifest changed")
    args.out = args.out.resolve()
    if args.out == ROOT or ROOT in args.out.parents:
        parser.error("captures must remain outside the checkout")
    args.out.mkdir(parents=True, exist_ok=False)
    prereg = json.loads(prereg_raw)
    dev_path = ROOT / "src/fixtures/evaluation/query-corpus-v1.json"
    if sha(dev_path.read_bytes()) != "20c74d6575b8e6677353e28036fc92154d6c29ba46c76d61c2667cdfaf9a5da4":
        raise ValueError("frozen development corpus changed")
    dev = json.loads(dev_path.read_text())
    args.sources = args.sources.resolve(strict=True)
    public = json.loads((args.sources / "public-corpus.json").read_text())
    provenance = json.loads((args.sources / "provenance.json").read_text())
    verify_public_manifest(prereg, public, args.sources, provenance)
    fixtures = {item["fixture_id"]:ROOT / item["root"] for item in dev["fixtures"] + public["fixtures"]}
    negative = {"negative-low":ROOT / "src/fixtures/python/release/v0_1/low-support",
                "negative-dynamic":ROOT / "src/fixtures/python/release/v0_1/dynamic-unknown",
                "negative-malformed":ROOT / "src/fixtures/python/release/v0_1/low-support"}
    fixtures.update(negative)
    queries = dev["queries"] + public["queries"]
    for identifier in negative:
        queries.extend({"query_id":f"{identifier}-{i}", "fixture_id":identifier, "operation":"find", "target":target,
                        "mode":"compact", "intent":"abstention", "expected":{"outcome":"unknown"}}
                       for i, target in enumerate(CONTROLS["negative_qualification"]["queries"]))
    rows, analyses, data = [], [], {}
    binaries = {"old":args.old.resolve(strict=True), "candidate":args.candidate.resolve(strict=True)}
    pin_paths = {"old_binary":binaries["old"], "candidate_binary":binaries["candidate"],
                 "old_worker":binaries["old"].parent / "workers/python/worker.py",
                 "candidate_worker":binaries["candidate"].parent / "workers/python/worker.py",
                 "python":Path(sys.executable).resolve(), "collector":Path(__file__).resolve(),
                 "preparer":HERE / "prepare.py",
                 "controls":HERE / "qualification-controls.json", "preregistration":HERE / "preregistration.json",
                 "development_corpus":dev_path, "public_corpus":args.sources / "public-corpus.json",
                 "public_provenance":args.sources / "provenance.json"}
    for name in ("warm_queries.py", "compare_generations.py", "autosync_eval.py"):
        pin_paths[name] = HERE.parent / "performance" / name
    for name in ("adoption_v2.py", "native_host_v2.py", "treehash.py"):
        pin_paths[name] = HERE.parent / "agent_study" / name
    pins = {name:sha(path.read_bytes()) for name, path in pin_paths.items()}
    if pins["old_binary"] != prereg["baseline_binary_sha256"] or any(pins[name] != prereg["baseline_worker_sha256"] for name in ("old_worker", "candidate_worker")):
        raise ValueError("baseline producer or Python worker identity drift")
    source_pins = {name:tree_sha256(str(source)) for name, source in fixtures.items()}
    (args.out / "capture-freeze.json").write_text(json.dumps({"files":pins, "sources":source_pins,
        "python_version":sys.version.split()[0], "python_project_session":"unset: default enabled",
        "optional_provider_environment":"absent: environment constructed from allowlist"}, indent=2) + "\n")
    for fixture_id, source in fixtures.items():
        fingerprints, materialized = {}, {}
        for arm, binary in binaries.items():
            cell = args.out / fixture_id / arm
            project, env, materialized[arm] = initialize(binary, source, cell, fixture_id == "negative-malformed", fixture_id in negative)
            fingerprints[arm] = fingerprint(project / ".repogrammar/repogrammar.sqlite")
            families = command(binary, project, env, ["families"], cell / "families")
            if families.returncode:
                raise ValueError("family inventory unavailable")
            inventory = json.loads(families.stdout, object_pairs_hook=_unique_object).get("families")
            if not isinstance(inventory, list):
                raise ValueError("malformed family inventory")
            if fixture_id in negative and inventory:
                raise ValueError("negative fixture formed an unsupported family")
            for query in [q for q in queries if q["fixture_id"] == fixture_id]:
                current_project, current_env = project, env
                if query.get("mutation"):
                    mutation_cell = cell / query["query_id"]
                    current_project, current_env, _ = initialize(binary, source, mutation_cell)
                    mutation = query["mutation"]
                    if mutation["kind"] != "append_line" or ".." in PurePosixPath(mutation["path"]).parts:
                        raise ValueError("unsupported fixed-corpus mutation")
                    with (current_project / mutation["path"]).open("a") as output:
                        output.write("\n" + mutation["line"] + "\n")
                prefix = cell / query["query_id"]
                if query["operation"] == "inspect_readiness":
                    raw = mcp(binary, current_project, current_env, {"operation":"inspect_readiness", "within":query["target"]}, prefix)
                    status = 0
                else:
                    arguments = [query["operation"], query["target"], "--mode", query.get("mode", "compact")]
                    if "against" in query:
                        arguments.extend(["--against", query["against"]])
                    result = command(binary, current_project, current_env, arguments, prefix)
                    raw, status = result.stdout, result.returncode
                checked = validate_payload(raw, current_project, query.get("mutation", {}).get("path"))
                payload = json.loads(raw)
                if fixture_id in negative or fixture_id in ("fastapi-template", "requests"):
                    qualify_outcome(payload, status, query["intent"] == "abstention")
                row = {"fixture":fixture_id, "arm":arm, "query":query["query_id"], "surface":"cli" if query["operation"] != "inspect_readiness" else "mcp",
                       "returncode":status, "bytes":len(raw), "sha256":sha(raw), "status":payload.get("status"), **checked}
                rows.append(row)
                data[(arm, query["query_id"], row["surface"])] = (status, raw)
                if fixture_id in negative or fixture_id in ("fastapi-template", "requests"):
                    mcp_raw = mcp(binary, current_project, current_env, {"operation":"find_analogues", "target":query["target"], "mode":"compact"}, cell / (query["query_id"] + "-mcp"))
                    check = validate_payload(mcp_raw, current_project)
                    mcp_payload = json.loads(mcp_raw)
                    qualify_outcome(mcp_payload, 0, query["intent"] == "abstention")
                    if payload.get("status") != mcp_payload.get("status") or selected_ids(payload) != selected_ids(mcp_payload):
                        raise ValueError("CLI/MCP status or selected-family identities disagree")
                    rows.append({**row, "surface":"mcp", "status":mcp_payload.get("status"), "returncode":0, "bytes":len(mcp_raw), "sha256":sha(mcp_raw), **check})
                    data[(arm, query["query_id"], "mcp")] = (0, mcp_raw)
        if materialized["old"] != materialized["candidate"] or fingerprints["old"] != fingerprints["candidate"]:
            raise ValueError("canonical active generation changed")
        analyses.append({"fixture":fixture_id, "source_sha256":source_pins[fixture_id], "materialized_source_sha256":materialized["old"],
                         "canonical_analysis_sha256":sha(canonical_bytes(fingerprints["old"])), "old_candidate_equal":True})
    compared = []
    intended = set(CONTROLS["intended_development_query_changes"]) | {q["query_id"] for q in public["queries"]}
    for (arm, query, surface), (status, raw) in data.items():
        if arm != "old":
            continue
        other_status, other = data[("candidate", query, surface)]
        comparison = parity(raw, other)
        comparison.update(query=query, surface=surface, exit_status_equal=status == other_status, intended_query_change=query in intended)
        if query not in intended and (not comparison["equal"] or status != other_status):
            raise ValueError(f"unapproved response change: {query}/{surface}")
        compared.append(comparison)
    if pins != {name:sha(path.read_bytes()) for name, path in pin_paths.items()} or source_pins != {name:tree_sha256(str(source)) for name, source in fixtures.items()}:
        raise ValueError("producer, protocol, or source identity changed during capture")
    summary = {"schema_version":"qualified-phrase-capture.v1", "preregistration_sha256":PREREGISTRATION_SHA256,
               "controls_sha256":sha((HERE / "qualification-controls.json").read_bytes()),
               "binary_sha256":{arm:sha(binary.read_bytes()) for arm, binary in binaries.items()},
               "frozen_files":pins, "pins_rechecked_after":True,
               "analyses":analyses, "responses":rows, "comparisons":compared,
               "privacy":"Default payload source text prohibited; raw public/fixture responses retained outside Git.",
               "scoring":"Use existing repo-guard product-eval metrics; this collector does not redefine gold."}
    (args.out / "capture-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"Verified {len(analyses)} unchanged canonical analyses and captured {len(rows)} responses")


if __name__ == "__main__":
    main()
