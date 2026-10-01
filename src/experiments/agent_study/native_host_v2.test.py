"""Portable scripted-native contract checks; actual host requires native CLI run."""
import json
import os
import signal
import sys
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

import native_host_v2 as host


class NativeHostTest(unittest.TestCase):
    def test_environment_and_flags_preserve_discovery_without_real_auth(self):
        with tempfile.TemporaryDirectory() as tmp:
            with patch.dict(host.os.environ, {"ANTHROPIC_API_KEY": "not-inherited", "HOME": "/foreign",
                                               "HTTPS_PROXY": "http://foreign", "CLAUDE_CONFIG_DIR": "/foreign"}):
                env = host.host_environment(Path(tmp), 12345)
            self.assertEqual(env["ANTHROPIC_API_KEY"], host.DUMMY_KEY)
            self.assertEqual(env["ANTHROPIC_BASE_URL"], "http://127.0.0.1:12345")
            self.assertEqual(env["CLAUDE_CODE_TMPDIR"], env["TMPDIR"])
            self.assertNotIn("HTTPS_PROXY", env)
            args = host.native_command(Path(tmp) / "cli", Path(tmp) / "mcp.json", env)
            self.assertIn("--strict-mcp-config", args)
            for forbidden in ("--bare", "--safe-mode", "--append-system-prompt", "--dangerously-skip-permissions"):
                self.assertNotIn(forbidden, args)

    def test_capture_distinguishes_actual_user_guide_from_mcp_instructions(self):
        guide = b"<!-- marker -->\n## RepoGrammar\nunique conditional guide\n<!-- end -->"
        tool_names = ["Read", "Grep", "Glob", "Bash", host.MCP]
        body = {"tools": [{"name": n} for n in tool_names], "messages": [{"role": "user", "content": host.guide_body(guide)}]}
        self.assertEqual(host.assess_requests([body], guide, True)["global_discovery"], "PRESENT")
        body["messages"] = []
        body["system"] = "ordinary MCP initialize guide"
        self.assertEqual(host.assess_requests([body], guide, True)["global_discovery"], "ABSENT")
        body["tools"].append({"name": "mcp__foreign__hidden"})
        self.assertEqual(host.assess_requests([body], guide, True)["tool_inventory"], "FAIL")
        self.assertEqual(host.assess_requests([], guide, False)["global_discovery"], "NOT_MEASURED")

    def test_synthetic_stream_and_missing_results_never_pass(self):
        calls = host.scripted_tools(Path("/synthetic"), Path("/index"), Path("/product"), Path("/private"), False)
        stream = host.sse_message(calls, 1).decode()
        self.assertIn('"stop_reason": "tool_use"', stream)
        self.assertNotIn(host.DUMMY_KEY, stream)
        self.assertTrue(all(v == "NOT_MEASURED" for v in host.assess_results("", calls, "/synthetic").values()))
        events = [{"type": "assistant", "message": {"content": calls}}, {"type": "user", "message": {"content": [
            {"type": "tool_result", "tool_use_id": calls[0]["id"], "content": "allowed", "is_error": True}]}}]
        result = host.assess_results("\n".join(json.dumps(e) for e in events), calls, "/synthetic")
        self.assertEqual(result["positive_read"], "FAIL")

    def test_denial_rejects_acquired_data_and_duplicate_results(self):
        calls = [{"type": "tool_use", "id": "control_index_read", "name": "Read", "input": {"file_path": "/synthetic/index"}}]
        blocks = [{"type": "tool_result", "tool_use_id": calls[0]["id"],
                   "content": "secret acquired then Permission denied", "is_error": True}]
        events = [{"type": "assistant", "message": {"content": calls}}, {"type": "user", "message": {"content": blocks}}]
        raw = "\n".join(json.dumps(e) for e in events)
        self.assertEqual(host.assess_results(raw, calls, "/synthetic")["index_read"], "FAIL")
        events[1]["message"]["content"] = blocks * 2
        with self.assertRaises(ValueError):
            host.assess_results("\n".join(json.dumps(e) for e in events), calls, "/synthetic")

    def test_manifest_requires_empty_plugins_and_builtin_agents(self):
        event = {"type": "system", "subtype": "init", "plugins": [], "agents": [],
                 "skills": [], "slash_commands": [], "mcp_servers": []}
        self.assertEqual(host.assess_manifest(json.dumps(event), False)["status"], "PASS")
        event["agents"] = ["claude"]
        self.assertEqual(host.assess_manifest(json.dumps(event), False)["status"], "CONTROL_ISOLATION_BLOCKED")
        event["agents"] = []
        event["plugins"] = [{"name": "builtin"}]
        self.assertEqual(host.assess_manifest(json.dumps(event), False)["status"], "CONTROL_ISOLATION_BLOCKED")

    @unittest.skipUnless(hasattr(os, "mkfifo"), "Unix special file control")
    def test_inventory_tags_types_and_rejects_special_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "target.txt").write_text("target.txt")
            before = host.inventory(root)
            (root / "target.txt").unlink()
            (root / "target.txt").symlink_to("target.txt")
            self.assertNotEqual(before, host.inventory(root))
            os.mkfifo(root / "fifo")
            with self.assertRaises(ValueError):
                host.inventory(root)

    @unittest.skipUnless(hasattr(os, "fork"), "Unix owned process groups")
    def test_completed_parent_cleans_sigterm_ignoring_descendant(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            code = """import os,signal,time
child=os.fork()
if child==0:
    signal.signal(signal.SIGTERM,signal.SIG_IGN)
    open('child.pid','w').write(str(os.getpid()))
    time.sleep(30)
else:
    while not os.path.exists('child.pid'): time.sleep(.01)
    os._exit(0)
"""
            try:
                result = host.run_owned([sys.executable, "-I", "-S", "-c", code], root, {}, "",
                                        root / "stdout", root / "stderr", timeout=5)
                self.assertEqual(result[:2], (0, "COMPLETE"))
                self.assertEqual(result[2], "PASS")
                with self.assertRaises(ProcessLookupError):
                    os.kill(int((root / "child.pid").read_text()), 0)
            finally:
                if (root / "child.pid").exists():
                    try:
                        os.kill(int((root / "child.pid").read_text()), signal.SIGKILL)
                    except ProcessLookupError:
                        pass


if __name__ == "__main__":
    unittest.main()
