"""Bounded, source-free warm MCP profile over an already initialized corpus.

No index/configuration writes, repository downloads or agent execution. Run
baseline/candidate on the same frozen index; do not compare machine timings
across hosts. Two warmups are excluded per case/tier.
"""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import select
import signal
import sqlite3
import subprocess
import sys
import time
from contextlib import closing

from autosync_eval import native_resources, sha256
from compare_generations import canonical_bytes, fingerprint


def read_frame(fd, pending, deadline, limit=1_048_576):
    while b"\n" not in pending:
        remaining = deadline - time.monotonic()
        if remaining <= 0 or not select.select([fd], [], [], max(0, remaining))[0]:
            raise TimeoutError("MCP frame deadline exceeded")
        chunk = os.read(fd, min(65536, limit + 1 - len(pending)))
        if not chunk:
            raise ValueError("MCP stream closed before a complete frame")
        pending.extend(chunk)
        if len(pending) > limit:
            raise ValueError("MCP frame exceeds byte limit")
    end = pending.index(b"\n") + 1
    frame = bytes(pending[:end])
    del pending[:end]
    return frame


def sample_identity(payload, size):
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()
    return hashlib.sha256(canonical).hexdigest(), size


def accept_sample(first, payload, size):
    current = sample_identity(payload, size)
    if first is not None and first != current:
        raise ValueError("MCP result content/bytes changed between repetitions")
    return current


def require_source_free(payload):
    spans = payload.get("source_spans")
    included = spans.get("source_snippets_included") is True or bool(spans.get("spans")) if isinstance(spans, dict) else bool(spans)
    if payload.get("output", {}).get("source_snippets_included") is True or included:
        raise ValueError("benchmark requires source-free output")


def profile(args):
    pins = {"binary_sha256": sha256(args.binary), "harness_sha256": sha256(__file__)}
    index = fingerprint(args.project / ".repogrammar/repogrammar.sqlite")
    pins["active_index_sha256"] = hashlib.sha256(canonical_bytes(index)).hexdigest()
    pins["resource_helper_sha256"] = sha256(Path(__file__).with_name("autosync_eval.py"))
    pins["index_helper_sha256"] = sha256(Path(__file__).with_name("compare_generations.py"))
    environment = dict(os.environ, REPOGRAMMAR_TELEMETRY="0", DO_NOT_TRACK="1")
    args.out.mkdir(parents=True, exist_ok=True)
    native = args.out / "serve.native.txt"
    command = ["/usr/bin/time", "-l" if sys.platform == "darwin" else "-v", "-o", str(native), str(args.binary), "serve", "--project", str(args.project)]
    with (args.out / "serve.stderr").open("wb") as stderr:
        process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, env=environment, bufsize=0, start_new_session=True)
        os.set_blocking(process.stdin.fileno(), False)
        pending, sequence = bytearray(), 0

        def rpc(method, params):
            nonlocal sequence
            sequence += 1
            data = (json.dumps({"jsonrpc": "2.0", "id": sequence, "method": method, "params": params}, separators=(",", ":")) + "\n").encode()
            if len(data) > 4096:
                raise ValueError("MCP request exceeds benchmark bound")
            started = time.monotonic()
            deadline = started + 30
            offset = 0
            while offset < len(data):
                remaining = deadline - time.monotonic()
                if remaining <= 0 or not select.select([], [process.stdin], [], max(0, remaining))[1]:
                    raise TimeoutError("MCP request write deadline exceeded")
                offset += os.write(process.stdin.fileno(), data[offset:])
            line = read_frame(process.stdout.fileno(), pending, deadline)
            elapsed = (time.monotonic() - started) * 1000
            result = json.loads(line)
            if result.get("id") != sequence or "error" in result:
                raise ValueError("MCP id/error mismatch")
            return result, len(line), elapsed

        def call(arguments):
            result, wire, elapsed = rpc("tools/call", {"name": "repogrammar_context", "arguments": arguments})
            text = result["result"]["content"][0]["text"]
            payload = json.loads(text)
            require_source_free(payload)
            if payload.get("status") == "UNKNOWN":
                if not payload.get("unknowns") or any("reason" not in item or "recovery" not in item for item in payload["unknowns"]):
                    raise ValueError("UNKNOWN omitted reason/recovery")
                if "follow_up_family_ids" not in payload.get("query_route", {}):
                    raise ValueError("UNKNOWN omitted candidate handles")
            return payload, len(text.encode()), wire, elapsed

        try:
            initialized, _, _ = rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "bounded-read-profile", "version": "1"}})
            probe, _, _, _ = call({"operation": "find_analogues", "target": "framework:fastapi.route", "verbosity": "full"})
            if probe.get("status") != "ok":
                with closing(sqlite3.connect((args.project / ".repogrammar/repogrammar.sqlite").as_uri() + "?mode=ro", uri=True)) as db:
                    ids = [row[0] for row in db.execute("SELECT f.family_id FROM families f JOIN index_generations g ON g.generation_id=f.generation_id WHERE g.status='active' ORDER BY f.supported_member_count DESC,f.family_id LIMIT 16")]
                for family in ids:
                    probe, _, _, _ = call({"operation": "show_family", "target": family, "verbosity": "full"})
                    if probe.get("status") == "ok":
                        break
                else:
                    raise ValueError("no fresh family in bounded probe")
            family = probe["family"]["family_id"]
            member = probe["members"][0]["code_unit_id"]
            path = (probe["evidence"] or probe["read_plan"]["items"])[0]["path"]
            cases = [("readiness", {"operation": "inspect_readiness"}), ("exact_family", {"operation": "show_family", "target": family}), ("exact_member", {"operation": "find_analogues", "target": member}), ("exact_path", {"operation": "find_analogues", "target": path}), ("nl_fuzzy", {"operation": "find_analogues", "target": "How are FastAPI routes implemented?"}), ("unknown", {"operation": "find_analogues", "target": "How are React hooks memoized?"})]
            rows = []
            for category, arguments in cases:
                for tier in ["default"] if category == "readiness" else args.tier:
                    arguments = dict(arguments)
                    if tier != "default":
                        arguments.update(mode="compact", verbosity=tier.split("_")[1])
                    samples, sizes, wires, first = [], [], [], None
                    for _ in range(args.repetitions):
                        result, size, wire, elapsed = call(arguments)
                        first = accept_sample(first, result, size)
                        samples.append(elapsed)
                        sizes.append(size)
                        wires.append(wire)
                    warm = sorted(samples[2:])
                    route = result.get("query_route", {})
                    rows.append({"category": category, "tier": tier, "repetitions": args.repetitions, "warmup_discarded": 2, "latency_ms": samples, "p50_ms": warm[math.ceil(.5 * len(warm)) - 1], "p95_ms": warm[math.ceil(.95 * len(warm)) - 1], "payload_bytes": sizes, "wire_bytes": wires, "payload_sha256": first[0], "estimated_payload_tokens_bytes4": [math.ceil(size / 4) for size in sizes], "status": result.get("status"), "route": route.get("route"), "selected_family_count": int(bool(result.get("family", {}).get("family_id"))), "candidate_count": len(route.get("candidate_family_ids", [])), "read_plan_item_count": len(result.get("read_plan", {}).get("items", [])), "source_snippets_included": result.get("output", {}).get("source_snippets_included")})
            rpc("shutdown", {})
            process.stdin.close()
            process.wait(timeout=10)
            if process.returncode or pins["binary_sha256"] != sha256(args.binary):
                raise ValueError("MCP process failed or binary changed")
            resources = native_resources(native.read_text(), sys.platform)
            if resources["state"] != "MEASURED":
                raise ValueError("native resource measurement unavailable")
            if index != fingerprint(args.project / ".repogrammar/repogrammar.sqlite"):
                raise ValueError("active index changed during profile")
            target_hashes = {"family": hashlib.sha256(family.encode()).hexdigest(), "member": hashlib.sha256(member.encode()).hexdigest(), "path": hashlib.sha256(path.encode()).hexdigest()}
            return {"schema": "warm-mcp-profile.v2", **pins, "selected_target_sha256": target_hashes, "initialize_version": initialized["result"]["serverInfo"]["version"], "condition": "one persistent process, sequential calls; two warmups excluded per case/tier; index fingerprint outside native process lifetime", "native_resources": resources, "rows": rows}
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)
            process.stdin.close()
            process.stdout.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", dest="binary", required=True, type=Path)
    parser.add_argument("--project", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--repetitions", type=int, default=8)
    parser.add_argument("--tier", choices=["default", "compact_minimal", "compact_standard"], action="append")
    args = parser.parse_args()
    if sys.platform not in ("darwin", "linux") or not 4 <= args.repetitions <= 12:
        parser.error("native macOS/Linux and 4..12 repetitions required")
    args.binary, args.project = args.binary.resolve(strict=True), args.project.resolve(strict=True)
    args.tier = args.tier or ["default"]
    report = profile(args)
    (args.out / "warm-mcp-profile.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print("warm MCP profile complete")


if __name__ == "__main__":
    main()
