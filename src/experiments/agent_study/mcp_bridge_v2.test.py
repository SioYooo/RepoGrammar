"""Offline socket/product controls. Optional --product-binary uses a temp project."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import socket
import stat
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest import mock

from mcp_bridge_v2 import Bridge, MAX_LINE_BYTES, ROOT, Refusal, admitted_request


PRODUCT_BINARY = None

# Exact public initialize envelope captured from the pinned native CLI. Contains
# no task/source/answer or credentials; the private original remains outside Git.
CAPTURED_INITIALIZE = b'''{"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{"roots":{"listChanged":true},"elicitation":{}},"clientInfo":{"name":"claude-code","title":"Claude Code","version":"2.1.286","description":"Anthropic's agentic coding tool","websiteUrl":"https://claude.com/claude-code"}},"jsonrpc":"2.0","id":0}\n'''


def frame(method, identifier=1, params=None):
    request = {"jsonrpc": "2.0", "method": method}
    if identifier is not None:
        request["id"] = identifier
    if params is not None:
        request["params"] = params
    return json.dumps(request, separators=(",", ":")).encode() + b"\n"


CALL = {"name": "repogrammar_context", "arguments": {"operation": "find_analogues", "target": "probe.py", "mode": "compact"}}


def fake_product(root: Path, mode="normal") -> Path:
    executable = root / "fake-product"
    executable.write_text(f'''#!{sys.executable}
import json, os, sys, time
mode = {mode!r}
for line in sys.stdin:
    request = json.loads(line)
    method = request["method"]
    if mode == "hang":
        time.sleep(20)
    if mode == "exit":
        sys.exit(7)
    if method == "notifications/initialized":
        continue
    if mode == "oversized":
        print("x" * 1048576, flush=True)
        continue
    if mode == "duplicate":
        print('{{"jsonrpc":"2.0","id":1,"id":1,"result":{{}}}}', flush=True)
        continue
    response = {{"jsonrpc": "2.0", "id": request["id"]}}
    if mode == "wrong_id":
        response["id"] = "wrong"
    if method == "initialize":
        response["result"] = {{"serverInfo": {{"name": "repogrammar"}},
             "isolated_env": not any(k in os.environ for k in ("ANTHROPIC_API_KEY", "OPENAI_API_KEY", "HTTP_PROXY")),
             "pinned_command": sys.argv[1:] == ["serve", "--project", os.getcwd()]}}
    elif method == "tools/list":
        response["result"] = {{"tools": [{{"name": "repogrammar_context"}}]}}
    elif method == "ping":
        response["error"] = {{"code": -32601, "message": "unsupported method"}}
    else:
        response["result"] = {{"content": [{{"type": "text", "text": '{{"status":"FALLBACK_TO_CODE_SEARCH"}}'}}], "isError": False}}
    print(json.dumps(response, separators=(",", ":")), flush=True)
''')
    executable.chmod(0o700)
    return executable


class BridgeTests(unittest.TestCase):
    def bridge(self, root, mode="normal", timeout=2, binary=None):
        worktree = root / "worktree"
        worktree.mkdir()
        (worktree / "probe.py").write_text("def probe():\n    return 1\n")
        return Bridge(binary or fake_product(root, mode), worktree, root / "mcp.sock", root / "evidence", timeout=timeout)

    def exchange(self, bridge, requests):
        with bridge, socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
            connection.settimeout(2)
            connection.connect(str(bridge.socket_path))
            connection.sendall(b"".join(requests))
            connection.shutdown(socket.SHUT_WR)
            with connection.makefile("rb") as stream:
                responses = list(stream)
            bridge.thread.join(timeout=2)
        return responses

    def test_handshake_notification_tool_and_product_error_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            bridge = self.bridge(Path(directory))
            responses = self.exchange(bridge, [frame("initialize", 1, {"protocolVersion": "2025-06-18"}),
                                              frame("notifications/initialized", None), frame("tools/list", "list"),
                                              frame("tools/call", 3, CALL), frame("ping", 4)])
            payloads = [json.loads(line) for line in responses]
            self.assertEqual([p["id"] for p in payloads], [1, "list", 3, 4])
            self.assertTrue(payloads[0]["result"]["isolated_env"])
            self.assertTrue(payloads[0]["result"]["pinned_command"])
            self.assertEqual(payloads[-1]["error"]["code"], -32601)
            self.assertEqual(bridge.summary["status"], "COMPLETE")
            self.assertEqual(bridge.summary["method_counts"]["notifications/initialized"], 1)
            self.assertEqual(bridge.summary["frames"][1]["status"], "NOTIFICATION_FORWARDED")
            self.assertIsNone(bridge.summary["frames"][1]["result_sha256"])
            self.assertEqual(bridge.summary["process_cleanup"], "PASS")
            self.assertIsNotNone(bridge.process.poll())
            self.assertFalse(bridge.socket_path.exists())
            summary = (bridge.output_dir / "bridge.summary.json").read_text()
            for forbidden in (directory, "probe.py", "isolated_env", "pinned_command", "serverInfo"):
                self.assertNotIn(forbidden, summary)
            self.assertTrue(bridge.summary["requests_sha256"])
            self.assertTrue(bridge.summary["results_sha256"])

    def test_denied_requests_never_reach_product(self):
        for denied in [frame("shutdown"), frame("tools/call", 1, {"name": "Bash", "arguments": {}}),
                       b'{"jsonrpc":"2.0","id":1,"id":2,"method":"initialize"}\n',
                       frame("initialize", True)]:
            with self.subTest(request_hash=len(denied)), tempfile.TemporaryDirectory() as directory:
                bridge = self.bridge(Path(directory))
                self.assertEqual(self.exchange(bridge, [denied]), [])
                self.assertEqual(bridge.summary["status"], "FAILED")
                self.assertEqual(sum(bridge.summary["method_counts"].values()), 0)
                self.assertEqual(bridge.summary["process_cleanup"], "PASS")
                self.assertEqual((bridge.output_dir / "requests.jsonl").read_bytes(), denied)

    def test_schema_and_inclusive_frame_bound(self):
        valid = frame("initialize", 1, {"capabilities": {"padding": ""}})
        exact = frame("initialize", 1, {"capabilities": {"padding": "x" * (MAX_LINE_BYTES - len(valid))}})
        self.assertEqual(len(exact), MAX_LINE_BYTES)
        admitted_request(exact)
        bad = [exact[:-1] + b"x\n", frame("initialize", None), frame("notifications/initialized", 1),
               frame("initialize", 1, {"unknown": True}), frame("tools/call", 1, {"name": "repogrammar_context", "arguments": {"operation": "resync"}})]
        for key, value in (("token_budget", True), ("token_budget", 200001), ("mode", "source"),
                           ("target", "x" * 8193), ("target", "\u0085"), ("target", "\ud800"),
                           ("include_source_spans", 1), ("against", "family:unsafe"), ("unknown", "field")):
            call = copy.deepcopy(CALL)
            call["arguments"][key] = value
            bad.append(frame("tools/call", 1, call))
        for line in bad:
            with self.assertRaises(Refusal):
                admitted_request(line)

    def test_product_failure_cleanup_and_id_pairing(self):
        for mode, reason in (("wrong_id", "PRODUCT_RESPONSE"), ("duplicate", "DUPLICATE_KEY"),
                             ("oversized", "PRODUCT_LINE_BOUND"), ("exit", "PRODUCT_EOF"), ("hang", "DEADLINE")):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as directory:
                bridge = self.bridge(Path(directory), mode, timeout=0.3 if mode == "hang" else 2)
                self.assertEqual(self.exchange(bridge, [frame("initialize")]), [])
                self.assertEqual(bridge.summary["reason"], reason)
                self.assertEqual(bridge.summary["process_cleanup"], "PASS")
                self.assertFalse(bridge.thread.is_alive())
        with tempfile.TemporaryDirectory() as directory:
            bridge = self.bridge(Path(directory))
            responses = self.exchange(bridge, [frame("initialize"), frame("tools/list")])
            self.assertEqual(len(responses), 1)
            self.assertEqual(bridge.summary["reason"], "ID_REUSE")

    def test_one_connection_and_close_without_client(self):
        with tempfile.TemporaryDirectory() as directory:
            bridge = self.bridge(Path(directory), timeout=20).start()
            started = time.monotonic()
            bridge.close()
            self.assertLess(time.monotonic() - started, 2)
            self.assertFalse(bridge.thread.is_alive())
            self.assertEqual(bridge.summary["status"], "CLOSED")
            self.assertEqual(bridge.summary["process_cleanup"], "PASS")
        with tempfile.TemporaryDirectory() as directory:
            bridge = self.bridge(Path(directory)).start()
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as first:
                first.settimeout(2)
                first.connect(str(bridge.socket_path))
                first.sendall(frame("initialize"))
                self.assertTrue(first.recv(MAX_LINE_BYTES))
                with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as second:
                    with self.assertRaises(OSError):
                        second.connect(str(bridge.socket_path))
            bridge.thread.join(timeout=2)
            bridge.close()
            self.assertEqual(bridge.summary["connections"], 1)

    def test_client_stdio_round_trip_and_path_guards(self):
        with tempfile.TemporaryDirectory() as directory:
            bridge = self.bridge(Path(directory))
            with bridge:
                result = subprocess.run([sys.executable, str(Path(__file__).with_name("mcp_bridge_v2.py")),
                                         "--socket", str(bridge.socket_path), "--timeout", "2"],
                                        input=frame("initialize") + frame("notifications/initialized", None) + frame("tools/list", 2),
                                        capture_output=True, timeout=3, env={"PATH": "/usr/bin:/bin"})
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(len(result.stdout.splitlines()), 2)
                bridge.thread.join(timeout=2)
            self.assertEqual(bridge.summary["status"], "COMPLETE")
            with self.assertRaises(ValueError):
                bridge.start()
            with self.assertRaises(ValueError):
                Bridge(bridge.binary, ROOT, Path(directory) / "new.sock", Path(directory) / "new")
            with self.assertRaises(ValueError):
                Bridge(bridge.binary, bridge.worktree, Path(directory) / "new.sock", bridge.worktree / "private")
            link = Path(directory) / "binary-link"
            link.symlink_to(bridge.binary)
            with self.assertRaises(ValueError):
                Bridge(link, bridge.worktree, Path(directory) / "new.sock", Path(directory) / "new")

    def test_binary_drift_is_refused_before_launch(self):
        with tempfile.TemporaryDirectory() as directory:
            bridge = self.bridge(Path(directory))
            bridge.binary.write_text("#!/bin/sh\nexit 0\n")
            with self.assertRaises(Refusal):
                bridge.start()
            self.assertIsNone(bridge.process)
            self.assertEqual(bridge.summary["status"], "START_FAILURE")

    def test_client_missing_response_is_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            bridge = self.bridge(Path(directory))
            with bridge:
                result = subprocess.run([sys.executable, str(Path(__file__).with_name("mcp_bridge_v2.py")),
                                         "--socket", str(bridge.socket_path), "--timeout", "2"],
                                        input=frame("initialize") + frame("tools/list"),
                                        capture_output=True, timeout=3, env={"PATH": "/usr/bin:/bin"})
                self.assertEqual(result.returncode, 1)
                self.assertEqual(len(result.stdout.splitlines()), 1)
                bridge.thread.join(timeout=2)
            self.assertEqual(bridge.summary["reason"], "ID_REUSE")

    def test_client_audit_captures_native_input_before_schema_refusal(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bridge = self.bridge(root)
            audit = root / "native-input.jsonl"
            native = frame("initialize", 1, {"protocolVersion": "2025-06-18", "capabilities": {},
                                              "clientInfo": {"name": "native-probe", "version": "1", "unqualified": "field"}})
            with bridge:
                result = subprocess.run([sys.executable, str(Path(__file__).with_name("mcp_bridge_v2.py")),
                                         "--socket", str(bridge.socket_path), "--timeout", "2", "--audit-file", str(audit)],
                                        input=native, capture_output=True, timeout=3, env={"PATH": "/usr/bin:/bin"})
                self.assertEqual(result.returncode, 1)
                self.assertEqual(result.stdout, b"")
                self.assertIn(b"MCP_BRIDGE_CLIENT_FAILURE:PARAMS_DENIED", result.stderr)
                bridge.thread.join(timeout=2)
            self.assertEqual(audit.read_bytes(), native)
            self.assertEqual(stat.S_IMODE(audit.stat().st_mode), 0o600)
            self.assertEqual(sum(bridge.summary["method_counts"].values()), 0)

    def test_client_audit_is_bounded_and_refuses_existing_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bridge = self.bridge(root)
            audit = root / "native-input.jsonl"
            notification = frame("notifications/initialized", None)
            with bridge:
                result = subprocess.run([sys.executable, str(Path(__file__).with_name("mcp_bridge_v2.py")),
                                         "--socket", str(bridge.socket_path), "--timeout", "2", "--audit-file", str(audit)],
                                        input=notification * 129, capture_output=True, timeout=3, env={"PATH": "/usr/bin:/bin"})
                self.assertEqual(result.returncode, 1)
                self.assertIn(b"MCP_BRIDGE_CLIENT_FAILURE:FRAME_BOUND", result.stderr)
                bridge.thread.join(timeout=2)
            self.assertEqual(audit.read_bytes(), notification * 128)
            from mcp_bridge_v2 import _audit_stream
            with self.assertRaises(FileExistsError):
                _audit_stream(audit)
            self.assertEqual(audit.read_bytes(), notification * 128)
            with self.assertRaises(Refusal):
                _audit_stream(ROOT / "forbidden-audit.jsonl")
            with self.assertRaises(Refusal):
                _audit_stream(ROOT / "forbidden-audit.jsonl", root)

    def test_relocated_client_uses_explicit_pinned_audit_authority(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runtime = root / "runtime"
            runtime.mkdir()
            copied = runtime / "mcp_bridge_v2.py"
            copied.write_bytes(Path(__file__).with_name("mcp_bridge_v2.py").read_bytes())
            audit = root / "native-home-audit.jsonl"
            native = frame("initialize", 1, {"clientInfo": {"name": "native-probe", "version": "1", "unqualified": "field"}})
            bridge = self.bridge(root)
            with bridge:
                result = subprocess.run([sys.executable, str(copied), "--socket", str(bridge.socket_path),
                                         "--timeout", "2", "--audit-file", str(audit), "--repository-root", str(ROOT)],
                                        input=native, capture_output=True, timeout=3, env={"PATH": "/usr/bin:/bin"})
                self.assertEqual(result.returncode, 1)
                self.assertIn(b"MCP_BRIDGE_CLIENT_FAILURE:PARAMS_DENIED", result.stderr)
                self.assertNotIn(b"AUDIT_PATH", result.stderr)
                bridge.thread.join(timeout=2)
            self.assertEqual(audit.read_bytes(), native)
            self.assertEqual(sum(bridge.summary["method_counts"].values()), 0)

    def test_captured_native_initialize_preserves_bytes_through_copied_client(self):
        self.assertEqual(hashlib.sha256(CAPTURED_INITIALIZE).hexdigest(),
                         "73cf2d70a8f63a5a31bd0f366276ba28d4531e11df24f3ea62a6ffa07cbd49e1")
        self.assertEqual(admitted_request(CAPTURED_INITIALIZE)["id"], 0)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runtime = root / "runtime"
            runtime.mkdir()
            copied = runtime / "mcp_bridge_v2.py"
            copied.write_bytes(Path(__file__).with_name("mcp_bridge_v2.py").read_bytes())
            audit = root / "native-home-audit.jsonl"
            bridge = self.bridge(root)
            with bridge:
                result = subprocess.run([sys.executable, str(copied), "--socket", str(bridge.socket_path),
                                         "--timeout", "2", "--audit-file", str(audit), "--repository-root", str(ROOT)],
                                        input=CAPTURED_INITIALIZE, capture_output=True, timeout=3,
                                        env={"PATH": "/usr/bin:/bin"})
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(result.stdout)["id"], 0)
                bridge.thread.join(timeout=2)
            self.assertEqual(audit.read_bytes(), CAPTURED_INITIALIZE)
            self.assertEqual((bridge.output_dir / "requests.jsonl").read_bytes(), CAPTURED_INITIALIZE)
            self.assertEqual(bridge.summary["method_counts"]["initialize"], 1)

    def test_client_metadata_stays_closed_and_bounded(self):
        captured = json.loads(CAPTURED_INITIALIZE)
        for key in ("name", "version"):
            missing = copy.deepcopy(captured)
            del missing["params"]["clientInfo"][key]
            with self.assertRaises(Refusal):
                admitted_request(json.dumps(missing).encode() + b"\n")
        for key in ("name", "version", "title", "description", "websiteUrl"):
            for value in (None, 1, True, {}, [], "", "x" * 129, "bad\nmetadata"):
                malformed = copy.deepcopy(captured)
                malformed["params"]["clientInfo"][key] = value
                with self.subTest(key=key, value_type=type(value).__name__), self.assertRaises(Refusal):
                    admitted_request(json.dumps(malformed).encode() + b"\n")
        unknown = copy.deepcopy(captured)
        unknown["params"]["clientInfo"]["extra"] = "refused"
        with self.assertRaises(Refusal):
            admitted_request(json.dumps(unknown).encode() + b"\n")

    def test_client_waits_for_deferred_frames_on_inherited_nonblocking_stdio(self):
        from mcp_bridge_v2 import _product_line
        with tempfile.TemporaryDirectory() as directory:
            bridge = self.bridge(Path(directory), timeout=3)
            input_read, input_write = os.pipe()
            output_read, output_write = os.pipe()
            os.set_blocking(input_read, False)
            os.set_blocking(output_write, False)
            os.write(input_write, CAPTURED_INITIALIZE + frame("notifications/initialized", None) + frame("tools/list", 1))
            process = None
            try:
                with bridge:
                    process = subprocess.Popen([sys.executable, str(Path(__file__).with_name("mcp_bridge_v2.py")),
                                                "--socket", str(bridge.socket_path), "--timeout", "3"],
                                               stdin=input_read, stdout=output_write, stderr=subprocess.PIPE,
                                               env={"PATH": "/usr/bin:/bin"})
                    os.close(input_read)
                    input_read = None
                    os.close(output_write)
                    output_write = None
                    buffered = bytearray()
                    replies = [json.loads(_product_line(output_read, buffered, time.monotonic() + 2)) for _ in range(2)]
                    self.assertEqual([r["id"] for r in replies], [0, 1])
                    # An open stdin with no current bytes is not transport EOF.
                    time.sleep(0.15)
                    self.assertIsNone(process.poll(), "client closed before the deferred tool call")
                    os.write(input_write, frame("tools/call", 2, CALL))
                    reply = json.loads(_product_line(output_read, buffered, time.monotonic() + 2))
                    self.assertEqual(reply["id"], 2)
                    os.close(input_write)
                    input_write = None
                    _, stderr = process.communicate(timeout=3)
                    self.assertEqual(process.returncode, 0, stderr)
                    bridge.thread.join(timeout=2)
                self.assertEqual(bridge.summary["method_counts"]["tools/call"], 1)
            finally:
                for descriptor in (input_read, input_write, output_read, output_write):
                    if descriptor is not None:
                        os.close(descriptor)
                if process is not None:
                    if process.poll() is None:
                        process.terminate()
                    process.communicate(timeout=3)

    def test_client_waits_for_deferred_frames_on_aliased_duplex_stdio(self):
        from mcp_bridge_v2 import _product_line
        with tempfile.TemporaryDirectory() as directory:
            bridge = self.bridge(Path(directory), timeout=3)
            peer, transport = socket.socketpair()
            peer.settimeout(2)
            process = None
            try:
                with bridge:
                    process = subprocess.Popen([sys.executable, str(Path(__file__).with_name("mcp_bridge_v2.py")),
                                                "--socket", str(bridge.socket_path), "--timeout", "3"],
                                               stdin=transport, stdout=transport, stderr=subprocess.PIPE,
                                               env={"PATH": "/usr/bin:/bin"})
                    transport.close()
                    buffered = bytearray()
                    peer.sendall(CAPTURED_INITIALIZE)
                    first = json.loads(_product_line(peer.fileno(), buffered, time.monotonic() + 2))
                    self.assertEqual(first["id"], 0)
                    peer.sendall(frame("notifications/initialized", None) + frame("tools/list", 1))
                    listed = json.loads(_product_line(peer.fileno(), buffered, time.monotonic() + 2))
                    self.assertEqual(listed["id"], 1)
                    time.sleep(0.15)
                    self.assertIsNone(process.poll(), "aliased stdout flags closed stdin before the tool call")
                    peer.sendall(frame("tools/call", 2, CALL))
                    reply = json.loads(_product_line(peer.fileno(), buffered, time.monotonic() + 2))
                    self.assertEqual(reply["id"], 2)
                    peer.shutdown(socket.SHUT_WR)
                    _, stderr = process.communicate(timeout=3)
                    self.assertEqual(process.returncode, 0, stderr)
                    bridge.thread.join(timeout=2)
                self.assertEqual(bridge.summary["method_counts"]["tools/call"], 1)
            finally:
                peer.close()
                transport.close()
                if process is not None:
                    if process.poll() is None:
                        process.terminate()
                    process.communicate(timeout=3)

    def test_bounded_fd_reader_retries_eagain_and_preserves_partial_frames(self):
        from mcp_bridge_v2 import _product_line
        read, write = os.pipe()
        os.set_blocking(read, False)
        native_read = os.read
        first = True
        complete = frame("tools/list")
        os.write(write, complete[:12])

        def interrupted_once(descriptor, size):
            nonlocal first
            if first:
                first = False
                raise BlockingIOError()
            return native_read(descriptor, size)

        def finish_frame():
            time.sleep(0.05)
            os.write(write, complete[12:])

        writer = threading.Thread(target=finish_frame)
        writer.start()
        try:
            buffered = bytearray()
            with mock.patch("mcp_bridge_v2.os.read", side_effect=interrupted_once):
                self.assertEqual(_product_line(read, buffered, time.monotonic() + 2, allow_clean_eof=True), complete)
            writer.join(timeout=2)
            self.assertFalse(writer.is_alive())
            with self.assertRaisesRegex(Refusal, "DEADLINE"):
                _product_line(read, buffered, time.monotonic() + 0.03, allow_clean_eof=True)
            os.write(write, b"partial")
            os.close(write)
            write = None
            with self.assertRaisesRegex(Refusal, "TRUNCATED_LINE"):
                _product_line(read, buffered, time.monotonic() + 2, allow_clean_eof=True)
            self.assertEqual(buffered, b"partial")
            buffered.clear()
            self.assertIsNone(_product_line(read, buffered, time.monotonic() + 2, allow_clean_eof=True))
            with self.assertRaisesRegex(Refusal, "PRODUCT_EOF"):
                _product_line(read, buffered, time.monotonic() + 2)
        finally:
            writer.join(timeout=2)
            os.close(read)
            if write is not None:
                os.close(write)
        with tempfile.TemporaryFile() as oversized:
            oversized.write(b"x" * (MAX_LINE_BYTES + 1))
            oversized.seek(0)
            with self.assertRaisesRegex(Refusal, "LINE_BOUND"):
                _product_line(oversized.fileno(), bytearray(), time.monotonic() + 2, allow_clean_eof=True)

    def test_actual_product_handshake_and_readonly_fallback(self):
        if PRODUCT_BINARY is None:
            self.skipTest("optional pinned product binary not supplied")
        with tempfile.TemporaryDirectory() as directory:
            bridge = self.bridge(Path(directory), binary=PRODUCT_BINARY)
            responses = self.exchange(bridge, [frame("initialize", 1, {"protocolVersion": "2025-06-18"}),
                                              frame("notifications/initialized", None), frame("tools/list", 2),
                                              frame("tools/call", 3, CALL)])
            payloads = [json.loads(line) for line in responses]
            self.assertEqual(payloads[0]["result"]["serverInfo"]["name"], "repogrammar")
            self.assertEqual([t["name"] for t in payloads[1]["result"]["tools"]], ["repogrammar_context"])
            self.assertEqual(json.loads(payloads[2]["result"]["content"][0]["text"])["status"], "FALLBACK_TO_CODE_SEARCH")
            self.assertFalse((bridge.worktree / ".repogrammar").exists())
            self.assertEqual(bridge.summary["status"], "COMPLETE")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--product-binary", type=Path)
    options, remaining = parser.parse_known_args()
    PRODUCT_BINARY = options.product_binary
    unittest.main(argv=[sys.argv[0]] + remaining)
