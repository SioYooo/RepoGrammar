"""Scripted, loopback-only native Claude controls. No real model or auth request.

This qualifies a particular macOS CLI boundary, never adoption or billed usage.
Native HTTP bodies and transcripts remain outside Git. The fixture's usage and
tool choices are synthetic and must never enter the adoption metrics ledger.
"""
from __future__ import annotations

import argparse
from contextlib import nullcontext
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import re
import shutil
import signal
import sqlite3
import stat
import subprocess
import sys
import threading
import time

from adoption_v2 import _unique_object, current_mcp
from mcp_bridge_v2 import Bridge
import runner_v2 as offline
from treehash import tree_sha256

MODEL = "claude-haiku-4-5-20251001"
MCP = "mcp__repogrammar__repogrammar_context"
PROMPT = "Inspect the local route registration convention using the available tools."
SETTINGS = {"disableAllHooks": True, "autoMemoryEnabled": False,
            "includeGitInstructions": False, "attribution": {"commit": "", "pr": ""},
            "enabledPlugins": {"cc-plugin-agents-md@builtin": False, "cc-plugin-plugin-authoring@builtin": False}}
DUMMY_KEY = "sk-ant-offline-fixture-not-a-secret"
MAX_REQUEST = 8 * 1024 * 1024
EXPECTED_CLI_SHA256 = "75e3016e9d2570767b08e43a7467d4817a4f149232c169ca295f2c95fef21433"


def guide_body(guide):
    return re.sub(r"<!--.*?-->", "", guide.decode(), flags=re.S).strip()


def text_blocks(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for item in value:
            yield from text_blocks(item)
    elif isinstance(value, dict):
        for item in value.values():
            yield from text_blocks(item)


def scripted_tools(worktree, index, product, private_file, enabled):
    path = lambda name: str(worktree / name)
    calls = [
        ("positive_read", "Read", {"file_path": path("allowed.txt")}),
        ("positive_grep", "Grep", {"pattern": "allowed", "path": path("allowed.txt"), "output_mode": "content"}),
        ("positive_glob", "Glob", {"pattern": "allowed.txt", "path": str(worktree)}),
        ("positive_bash", "Bash", {"command": "/bin/cat allowed.txt", "timeout": 5000}),
        ("positive_sqlite", "Bash", {"command": "/usr/bin/sqlite3 :memory: \"select 'allowed';\"", "timeout": 5000}),
        ("index_read", "Read", {"file_path": str(index)}),
        ("index_grep", "Grep", {"pattern": "secret", "path": str(index), "output_mode": "content"}),
        ("index_glob", "Glob", {"pattern": ".repogrammar/*", "path": str(worktree)}),
        ("index_bash", "Bash", {"command": "/bin/cat .repogrammar/control.txt", "timeout": 5000}),
        ("index_sqlite", "Bash", {"command": "/usr/bin/sqlite3 .repogrammar/control.sqlite 'select value from sentinel;'", "timeout": 5000}),
        ("private_read", "Read", {"file_path": str(private_file)}),
        ("product_read", "Read", {"file_path": str(product)}),
    ]
    if enabled:
        calls.append(("mcp", MCP, {"operation": "find_analogues", "target": "routes.py", "mode": "compact"}))
    return [{"type": "tool_use", "id": "control_" + key, "name": name, "input": arguments}
            for key, name, arguments in calls]


def sse_message(content, ordinal):
    events = [("message_start", {"type": "message_start", "message": {
        "id": f"msg_offline_{ordinal}", "type": "message", "role": "assistant",
        "model": MODEL, "content": [], "stop_reason": None, "stop_sequence": None,
        "usage": {"input_tokens": 1, "output_tokens": 0}}})]
    for index, block in enumerate(content):
        start = dict(block)
        if block["type"] == "tool_use":
            start["input"] = {}
            delta = {"type": "input_json_delta", "partial_json": json.dumps(block["input"])}
        else:
            start["text"] = ""
            delta = {"type": "text_delta", "text": block["text"]}
        events.extend([
            ("content_block_start", {"type": "content_block_start", "index": index, "content_block": start}),
            ("content_block_delta", {"type": "content_block_delta", "index": index, "delta": delta}),
            ("content_block_stop", {"type": "content_block_stop", "index": index})])
    reason = "tool_use" if any(b["type"] == "tool_use" for b in content) else "end_turn"
    events.extend([("message_delta", {"type": "message_delta", "delta": {
        "stop_reason": reason, "stop_sequence": None}, "usage": {"output_tokens": 1}}),
        ("message_stop", {"type": "message_stop"})])
    return "".join(f"event: {name}\ndata: {json.dumps(event)}\n\n" for name, event in events).encode()


class ScriptedProvider:
    """One owned loopback endpoint; rejects non-fixture traffic and bounded bodies."""
    def __init__(self, calls, output):
        self.calls, self.output, self.requests = calls, output, []
        self.messages, self.invalid, self.message_requests = 0, 0, []
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                self.connection.settimeout(5)
                try:
                    length = int(self.headers.get("Content-Length", "0"))
                    if (not 0 < length <= MAX_REQUEST or len(owner.requests) >= 16
                            or self.headers.get("x-api-key") != DUMMY_KEY):
                        raise ValueError("unqualified request")
                    raw = self.rfile.read(length)
                    if len(raw) != length:
                        raise ValueError("partial request")
                    body = json.loads(raw, object_pairs_hook=_unique_object)
                    if not isinstance(body, dict) or body.get("model") != MODEL:
                        raise ValueError("model drift")
                    route = self.path.split("?", 1)[0]
                    if route not in ("/v1/messages", "/v1/messages/count_tokens"):
                        raise ValueError("unexpected route")
                    owner.requests.append(body)
                    (owner.output / f"request-{len(owner.requests)}.json").write_bytes(raw)
                    if route.endswith("count_tokens"):
                        data, mime = b'{"input_tokens":1}', "application/json"
                    else:
                        owner.messages += 1
                        owner.message_requests.append(body)
                        content = owner.calls if owner.messages == 1 else [{"type": "text", "text": "OFFLINE_CONTROL_DONE"}]
                        data, mime = sse_message(content, owner.messages), "text/event-stream"
                    self.send_response(200)
                    self.send_header("Content-Type", mime)
                    self.send_header("Content-Length", str(len(data)))
                    self.end_headers()
                    self.wfile.write(data)
                except (ValueError, OSError):
                    owner.invalid += 1
                    self.send_error(400)

        self.server = HTTPServer(("127.0.0.1", 0), Handler)
        self.port = self.server.server_address[1]
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *_):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)


def host_environment(cell, port):
    env = offline.environment(cell)
    Path(env["TMPDIR"]).chmod(0o700)
    env.update({"ANTHROPIC_API_KEY": DUMMY_KEY,
                "ANTHROPIC_BASE_URL": f"http://127.0.0.1:{port}",
                "CLAUDE_CODE_TMPDIR": env["TMPDIR"],
                "CLAUDE_CODE_DISABLE_AUTO_MEMORY": "1",
                "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
                "CLAUDE_CODE_DISABLE_OFFICIAL_MARKETPLACE_AUTOINSTALL": "1",
                "CLAUDE_CODE_DISABLE_POLICY_SKILLS": "1", "DISABLE_UPDATES": "1",
                "CLAUDE_AGENT_SDK_DISABLE_BUILTIN_AGENTS": "1",
                "ENABLE_TOOL_SEARCH": "false"})
    return env


def host_profile(worktree, env, cli, relay, config, port, socket_path, helpers=()):
    quote = json.dumps
    profile = offline.sandbox_profile(worktree, env, relay)
    ancestors = {str(p) for path in (worktree, Path(env["HOME"]), cli, config) for p in path.parents}
    profile += "(allow file-read-metadata " + " ".join(f"(literal {quote(p)})" for p in sorted(ancestors)) + ")\n"
    profile += f"(allow file-read* (literal {quote(str(cli.parent))}))\n"
    executables = [str(cli), "/bin/bash", "/bin/zsh", "/usr/bin/sqlite3", "/usr/bin/grep", "/usr/bin/env"]
    profile += "(allow process-exec " + " ".join(f"(literal {quote(p)})" for p in executables) + ")\n"
    profile += "(allow file-read* " + " ".join(f"(literal {quote(str(p))})" for p in (cli, config)) + ")\n"
    profile += f"(allow file-write* (subpath {quote(env['HOME'])}))\n"
    guide = Path(env["CLAUDE_CONFIG_DIR"]) / "CLAUDE.md"
    profile += f"(deny file-write* (literal {quote(str(guide))}))\n"
    profile += f"(deny file-write-unlink (literal {quote(env['HOME'])}) (literal {quote(env['CLAUDE_CONFIG_DIR'])}))\n"
    if helpers:
        profile += "(allow file-read* " + " ".join(f"(literal {quote(str(p))})" for p in helpers) + ")\n"
    profile += f'(allow network-outbound (remote ip "localhost:{port}"))\n'
    if socket_path is not None:
        profile += f"(allow file-read-metadata (literal {quote(str(socket_path))}))\n"
        profile += f"(allow network-outbound (remote unix-socket (path-literal {quote(str(socket_path))})))\n"
    return profile


def native_command(cli, config, env):
    return [str(cli), "-p", "--model", MODEL, "--output-format", "stream-json", "--verbose",
            "--strict-mcp-config", "--mcp-config", str(config), "--setting-sources", "user",
            "--settings", json.dumps(SETTINGS), "--disable-slash-commands", "--no-session-persistence",
            "--no-chrome", "--tools", "Read,Grep,Glob,Bash",
            "--allowedTools", "Read", "Grep", "Glob", "Bash", MCP]


def stop_owned_group(identifier):
    for sig, grace in ((signal.SIGTERM, 0.3), (signal.SIGKILL, 1.0)):
        try:
            os.killpg(identifier, sig)
        except ProcessLookupError:
            return "PASS"
        deadline = time.monotonic() + grace
        while time.monotonic() < deadline:
            try:
                os.killpg(identifier, 0)
            except ProcessLookupError:
                return "PASS"
            time.sleep(0.01)
    return "GROUP_STILL_PRESENT"


def run_owned(command, cwd, env, prompt, stdout, stderr, timeout=60):
    with stdout.open("wb") as out, stderr.open("wb") as err:
        proc = subprocess.Popen(command, cwd=cwd, env=env, stdin=subprocess.PIPE, stdout=out,
                                stderr=err, start_new_session=True)
        try:
            proc.communicate(prompt.encode(), timeout=timeout)
            status = "COMPLETE"
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGTERM)
            try:
                proc.communicate(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGKILL)
                proc.communicate()
            status = "TIMEOUT"
        return proc.returncode, status, stop_owned_group(proc.pid)


def inventory(root):
    """Exact control tree outside the documented mutable query aggregate only."""
    result = {}
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root)
        if relative.parts[:3] == (".repogrammar", "telemetry", "local-metrics"):
            continue
        mode = path.lstat().st_mode
        if stat.S_ISLNK(mode):
            result[str(relative)] = {"kind": "SYMLINK", "sha256": offline.sha(os.readlink(path).encode())}
        elif stat.S_ISREG(mode):
            result[str(relative)] = {"kind": "FILE", "sha256": offline.sha(path.read_bytes())}
        elif not stat.S_ISDIR(mode):
            raise ValueError("special file in protected control tree")
    return result


def kernel_gate(worktree, index, product, private_file, other_file, guide, env, profile, os_probe, service_probe, output):
    (worktree / "index-alias").symlink_to(".repogrammar/control.sqlite")
    arguments = [str(worktree), str(index), str(product), str(private_file), str(other_file),
                 offline.sha(guide) if (Path(env["CLAUDE_CONFIG_DIR"]) / "CLAUDE.md").exists() else ""]
    command = ["/usr/bin/sandbox-exec", "-f", str(profile), str(offline.PYTHON), "-I", "-S", str(os_probe)] + arguments
    proc = subprocess.run(command, cwd=worktree, env=env, capture_output=True, timeout=20)
    (output / "os.stdout").write_bytes(proc.stdout)
    (output / "os.stderr").write_bytes(proc.stderr)
    try:
        checks = json.loads(proc.stdout)
    except ValueError:
        checks = {}
    os_pass = proc.returncode == 0 and set(checks) == offline.CHECKS and all(v is True for v in checks.values())
    prefix = [str(offline.PYTHON), "-I", "-S", str(service_probe)]
    positive = subprocess.run(prefix, env=env, capture_output=True, timeout=5)
    negative = subprocess.run(["/usr/bin/sandbox-exec", "-f", str(profile)] + prefix,
                              env=env, capture_output=True, timeout=5)
    for label, result in (("service-positive", positive), ("service-negative", negative)):
        (output / (label + ".stdout")).write_bytes(result.stdout)
        (output / (label + ".stderr")).write_bytes(result.stderr)
    try:
        before, after = json.loads(positive.stdout), json.loads(negative.stdout)
        service_pass = positive.returncode == negative.returncode == 0 and before == {
            "lookup_return": 0, "received_right": True, "deallocate_return": 0} and after == {
            "lookup_return": 1100, "received_right": False, "deallocate_return": None}
    except ValueError:
        service_pass = False
    return {"os_controls": "PASS" if os_pass else "FAIL", "passed": sum(v is True for v in checks.values()),
            "required": len(offline.CHECKS), "securityserver_capability": "PASS" if service_pass else "NOT_QUALIFIED"}


def assess_requests(requests, guide, enabled):
    if not requests:
        return {"global_discovery": "NOT_MEASURED", "tool_inventory": "NOT_MEASURED"}
    required = {"Read", "Grep", "Glob", "Bash"} | ({MCP} if enabled else set())
    inventories = [{t.get("name") for t in r.get("tools", []) if isinstance(t, dict)} for r in requests]
    inventory = all(required <= names and names <= required | {"EndConversation"} for names in inventories)
    counts = [sum(guide_body(guide) in text for text in text_blocks(r.get("messages", []))) for r in requests]
    return {"global_discovery": "PRESENT" if all(c == 1 for c in counts) else "ABSENT" if all(c == 0 for c in counts) else "INCONSISTENT",
            "tool_inventory": "PASS" if inventory else "FAIL", "exposed_tool_count": len(inventories[0])}


def assess_results(transcript, expected_calls, root):
    events = [json.loads(line, object_pairs_hook=_unique_object) for line in transcript.splitlines() if line.strip()]
    from parsers import extract_tool_events
    expected_ids = {c["id"] for c in expected_calls}
    results = {}
    for event in extract_tool_events(events):
        if event["kind"] != "tool_result":
            continue
        identifier = event.get("tool_use_id")
        if identifier not in expected_ids or identifier in results:
            raise ValueError("unpaired or duplicate native control result")
        results[identifier] = event
    rows = {}
    for call in expected_calls:
        result = results.get(call["id"])
        key = call["id"].removeprefix("control_")
        if result is None:
            rows[key] = "NOT_MEASURED"
            continue
        text = result.get("text", "")
        if key.startswith("positive_"):
            ok = not result.get("is_error") and "allowed" in text
            if key in ("positive_grep", "positive_glob"):
                ok = ok and "No matches" not in text and "No files" not in text
        elif key == "mcp":
            try:
                mcp = current_mcp(text, str(root))
                ok = bool(mcp["family"]) and mcp["items"] > 0 and not mcp["stale"] and not mcp["unknown"]
            except (ValueError, TypeError):
                ok = False
        elif key in ("index_grep", "index_glob"):
            ok = "secret" not in text and "control.sqlite" not in text and (
                result.get("is_error") or "No matches" in text or "No files" in text)
        else:
            denied = any(reason in text.lower() for reason in ("permission denied", "operation not permitted", "eacces", "eperm", "authorization denied"))
            ok = bool(result.get("is_error")) and denied and "secret" not in text and "hidden oracle" not in text
        rows[key] = "PASS" if ok else "FAIL"
    return rows


def assess_manifest(transcript, enabled):
    events = [json.loads(line, object_pairs_hook=_unique_object) for line in transcript.splitlines() if line.strip()]
    records = [e for e in events if e.get("type") == "system" and e.get("subtype") == "init"]
    if len(records) != 1:
        return {"status": "NOT_MEASURED"}
    record = records[0]
    plugins, skills = record.get("plugins"), record.get("skills")
    agents = record.get("agents")
    servers = record.get("mcp_servers")
    expected_servers = {"repogrammar"} if enabled else set()
    server_ok = isinstance(servers, list) and {s.get("name") for s in servers if isinstance(s, dict)} == expected_servers
    clean = (plugins == [] and skills == [] and agents == []
             and record.get("slash_commands") == [] and server_ok)
    return {"status": "PASS" if clean else "CONTROL_ISOLATION_BLOCKED",
            "plugin_count": len(plugins) if isinstance(plugins, list) else None,
            "skill_count": len(skills) if isinstance(skills, list) else None,
            "extra_agent_count": len(agents) if isinstance(agents, list) else None,
            "mcp_count": len(servers) if isinstance(servers, list) else None}


def qualify(out, cli, product, guide_file):
    guide = offline.regular_bytes(guide_file, 1024)
    if offline.sha(guide) != "5df98f0677189042c7ae58aa74d6e7ac90bd99324a18edf263f9be8db7b4b256":
        raise ValueError("requires the current coordinator-pinned v4 formatter artifact")
    if sys.platform != "darwin":
        raise ValueError("native macOS qualification required")
    cli, product = cli.resolve(strict=True), product.resolve(strict=True)
    out = offline.new_output(out)
    runtime = out / "runtime"
    runtime.mkdir()
    copied_cli = runtime / "claude"
    copied_cli.write_bytes(offline.regular_bytes(cli, 512 * 1024 * 1024))
    copied_cli.chmod(0o500)
    if offline.sha(copied_cli.read_bytes()) != EXPECTED_CLI_SHA256:
        raise ValueError("CLI drift: this control freezes Claude Code 2.1.286")
    relay = runtime / "mcp_bridge_v2.py"
    relay.write_bytes(Path(__file__).with_name("mcp_bridge_v2.py").read_bytes())
    os_probe, service_probe = runtime / "os_probe.py", runtime / "service_probe.py"
    os_probe.write_bytes(Path(__file__).with_name("isolation_probe_v2.py").read_bytes())
    service_probe.write_bytes(Path(__file__).with_name("security_service_probe_v2.py").read_bytes())
    seed = out / "seed"
    source = {"routes.py": (offline.ROOT / "src/fixtures/python/release/v0_1/positive-strong-evidence/routes.py").read_text(), "allowed.txt": "allowed\n"}
    offline.materialize(seed, source)
    source_hash = tree_sha256(str(seed))
    build_env = offline.environment(out / "build-profile")
    build_env["PATH"] = "/opt/homebrew/bin:/usr/bin:/bin"
    with (out / "product-init.log").open("wb") as log:
        subprocess.run([str(product), "init", "--project", str(seed), "--yes", "--no-autosync"], env=build_env,
                       stdout=log, stderr=log, timeout=60, check=True)
    from contextlib import closing
    with closing(sqlite3.connect(seed / ".repogrammar/control.sqlite")) as db, db:
        db.execute("create table sentinel(value text)")
        db.execute("insert into sentinel values ('secret')")
    (seed / ".repogrammar/control.txt").write_text("secret index\n")
    freeze = {"schema_version": "agent-native-offline-freeze.v2", "provider": "SCRIPTED_LOOPBACK",
              "source_sha256": source_hash, "index_sha256": tree_sha256(str(seed / ".repogrammar")),
              "cli_sha256": offline.sha(copied_cli.read_bytes()), "product_sha256": offline.sha(product.read_bytes()),
              "cli_version": "2.1.286", "bridge_sha256": offline.sha(relay.read_bytes()),
              "os_probe_sha256": offline.sha(os_probe.read_bytes()), "service_probe_sha256": offline.sha(service_probe.read_bytes()),
              "guide_sha256": offline.sha(guide), "base_prompt_sha256": offline.sha(PROMPT.encode()),
              "arms": list(offline.ARMS), "live_execution": "BLOCKED_BUDGET_AUTH_HELD_OUT"}
    (out / "freeze.json").write_text(json.dumps(freeze, indent=2, sort_keys=True) + "\n")
    rows = []
    for arm in offline.ARMS:
        cell = out / arm
        worktree = cell / "worktree"
        shutil.copytree(seed, worktree)
        offline.admit_tree(worktree)
        if any((worktree / name).read_text() != value for name, value in source.items()) or tree_sha256(str(worktree / ".repogrammar")) != freeze["index_sha256"]:
            raise ValueError("initial arm source/index drift")
        private = cell / "private"
        private.mkdir()
        private_file = private / "oracle.txt"
        private_file.write_text("hidden oracle\n")
        other_file = private / "other-profile.txt"
        other_file.write_text("other profile\n")
        calls = scripted_tools(worktree, worktree / ".repogrammar/control.txt", product, private_file, arm != "B0")
        config = cell / "mcp.json"
        socket_path = cell / "bridge.sock"
        with ScriptedProvider(calls, private) as provider:
            env = host_environment(cell, provider.port)
            if arm == "B2":
                (Path(env["CLAUDE_CONFIG_DIR"]) / "CLAUDE.md").write_bytes(guide)
            config.write_text(json.dumps({"mcpServers": {} if arm == "B0" else {"repogrammar": {
                "command": str(offline.PYTHON), "args": ["-I", "-S", str(relay), "--socket", str(socket_path),
                    "--audit-file", str(Path(env["HOME"]) / "client-requests.jsonl"),
                    "--repository-root", str(offline.ROOT)]}}}))
            profile = cell / "host.sb"
            profile.write_text(host_profile(worktree, env, copied_cli, relay, config, provider.port,
                                            socket_path if arm != "B0" else None, (os_probe, service_probe)))
            baseline_env = offline.environment(cell)
            kernel = kernel_gate(worktree, worktree / ".repogrammar/control.sqlite", product, private_file,
                                 other_file, guide, baseline_env, profile, os_probe, service_probe, private)
            if kernel["os_controls"] != "PASS" or kernel["securityserver_capability"] != "PASS":
                rows.append({"arm": arm, "status": "CONTROL_ISOLATION_BLOCKED", "kernel": kernel,
                             "native_host": "NOT_RUN"})
                break
            before_inventory = inventory(worktree)
            protected = [copied_cli, relay, config, profile, os_probe, service_probe]
            if arm == "B2":
                protected.append(Path(env["CLAUDE_CONFIG_DIR"]) / "CLAUDE.md")
            protected_hashes = {p: offline.sha(p.read_bytes()) for p in protected}
            bridge = Bridge(product, worktree, socket_path, private / "bridge") if arm != "B0" else nullcontext()
            with bridge:
                command = ["/usr/bin/sandbox-exec", "-f", str(profile)] + native_command(copied_cli, config, env)
                prompt = offline.PREFIX + PROMPT if arm == "B3" else PROMPT
                code, status, cleanup = run_owned(command, worktree, env, prompt, private / "transcript.jsonl", private / "stderr.log")
            discovery = assess_requests(provider.message_requests, guide, arm != "B0")
            try:
                transcript = (private / "transcript.jsonl").read_text()
                results = assess_results(transcript, calls, worktree)
                manifest = assess_manifest(transcript, arm != "B0")
            except (ValueError, TypeError):
                results = {"transcript": "PARSE_FAILURE"}
                manifest = {"status": "PARSE_FAILURE"}
            integrity = (inventory(worktree) == before_inventory and all(
                p.is_file() and not p.is_symlink() and offline.sha(p.read_bytes()) == value
                for p, value in protected_hashes.items()))
            bridge_summary = bridge.summary if arm != "B0" else None
            bridge_pass = (arm == "B0" or (bridge_summary["status"] in ("CLOSED", "COMPLETE")
                           and bridge_summary["process_cleanup"] == "PASS"
                           and bridge_summary["method_counts"]["tools/call"] == 1
                           and all(f["status"] != "PRODUCT_ERROR" for f in bridge_summary["frames"])))
            passed = (code == 0 and status == "COMPLETE" and provider.invalid == 0
                      and cleanup == "PASS"
                      and discovery["tool_inventory"] == "PASS"
                      and manifest["status"] == "PASS"
                      and discovery["global_discovery"] == ("PRESENT" if arm == "B2" else "ABSENT")
                      and all(v == "PASS" for v in results.values())
                      and integrity and bridge_pass)
            rows.append({"arm": arm, "status": "PASS" if passed else "CONTROL_ISOLATION_BLOCKED",
                         "discovery": discovery, "native_controls": results,
                         "native_manifest": manifest,
                         "kernel": kernel, "protected_integrity": "PASS" if integrity else "FAIL",
                         "native_process_cleanup": cleanup,
                         "bridge": bridge_summary,
                         "provider": "SCRIPTED_LOOPBACK", "http_request_count": len(provider.requests),
                         "source_sha256": freeze["source_sha256"], "initial_index_sha256": freeze["index_sha256"],
                         "worktree_inventory_sha256": offline.sha(json.dumps(before_inventory, sort_keys=True).encode()),
                         "delivered_prompt_sha256": offline.sha(prompt.encode()),
                         "sandbox_sha256": offline.sha(profile.read_bytes()),
                         "transcript_sha256": offline.sha((private / "transcript.jsonl").read_bytes()),
                         "stderr_sha256": offline.sha((private / "stderr.log").read_bytes()),
                         "auth": "DUMMY_FIXTURE_ONLY", "adoption": "NOT_MEASURED", "cost_usd": None})
            if not passed:
                break
    summary = {"schema_version": "agent-native-offline-qualification.v2", "scope": "SCRIPTED_CLAUDE_MACOS",
               "status": "PASS" if len(rows) == 4 and all(r["status"] == "PASS" for r in rows) else "CONTROL_ISOLATION_BLOCKED",
               "freeze": freeze, "runs": rows, "runner_sha256": offline.sha(Path(__file__).read_bytes()),
               "real_model": "NOT_RUN", "real_credentials": "NOT_ACCESSED", "live_adoption": "NOT_MEASURED",
               "live_execution": "BLOCKED_BUDGET_AUTH_HELD_OUT", "linux_native": "NOT_RUN"}
    (out / "qualification.summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    return summary


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--product-bin", type=Path, required=True)
    parser.add_argument("--instruction-file", type=Path, required=True)
    args = parser.parse_args()
    result = qualify(args.out, args.cli, args.product_bin, args.instruction_file)
    print(result["status"] + "; SCRIPTED controls only; live adoption NOT_MEASURED")
    raise SystemExit(0 if result["status"] == "PASS" else 1)
