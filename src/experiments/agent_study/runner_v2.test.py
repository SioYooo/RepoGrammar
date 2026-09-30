"""Offline runner regression tests; native denial probes use the separate CLI."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

import runner_v2 as runner


class RunnerTest(unittest.TestCase):
    def test_native_control_rejects_incomplete_or_leaking_probe_results(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            product = root / "product"
            product.write_bytes(b"sentinel")
            complete = dict.fromkeys(runner.CHECKS, True)
            connect = runner.sqlite3.connect
            connections = []

            def tracked_connect(*args, **kwargs):
                connection = connect(*args, **kwargs)
                connections.append(connection)
                return connection

            for ordinal, checks in enumerate(({"read": True}, dict(complete, read=False), complete)):
                with patch.object(runner.sys, "platform", "darwin"), patch.object(Path, "is_file", return_value=True):
                    proc = SimpleNamespace(stdout=json.dumps(checks).encode(), stderr=b"", returncode=0)
                    with patch.object(runner.subprocess, "run", return_value=proc), patch.object(runner.sqlite3, "connect", side_effect=tracked_connect):
                        result = runner.native_control(root / str(ordinal), "B0", b"guide", product)
                self.assertEqual(result["status"], "PASS" if ordinal == 2 else "CONTROL_LEAK")
            for connection in connections:
                with self.assertRaises(runner.sqlite3.ProgrammingError):
                    connection.execute("select 1")

    def test_fresh_environment_does_not_inherit_host_state(self):
        with tempfile.TemporaryDirectory() as tmp:
            with patch.dict(os.environ, {"ANTHROPIC_API_KEY": "sentinel", "AGENT_STUDY_AUTH_MODE": "real",
                                         "REPOGRAMMAR_DIR": "/sentinel", "PYTHONPATH": "/sentinel"}):
                env = runner.environment(Path(tmp) / "cell")
            self.assertEqual(set(env), {"HOME", "CODEX_HOME", "CLAUDE_CONFIG_DIR", "XDG_CONFIG_HOME",
                                       "XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "TMPDIR",
                                       "PATH", "LANG", "LC_ALL"})
            self.assertTrue(all(Path(env[key]).is_dir() for key in env if key not in ("PATH", "LANG", "LC_ALL")))

    def test_refuse_aliases_special_files_and_bad_seed_paths(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "source"
            source.write_bytes(b"source")
            link = root / "link"
            link.symlink_to(source)
            with self.assertRaises(OSError):
                runner.regular_bytes(link, 100)
            link.unlink()
            os.link(source, link)
            with self.assertRaises(ValueError):
                runner.regular_bytes(source, 100)
            with self.assertRaises(ValueError):
                runner.admit_tree(root)
            os.mkfifo(root / "fifo")
            with self.assertRaises(ValueError):
                runner.regular_bytes(root / "fifo", 100)
            for i, name in enumerate(("../escape", "/absolute", "a/.claude/CLAUDE.md", "a\\b", "a:b")):
                with self.assertRaises(ValueError):
                    runner.materialize(root / str(i), {name: "bad"})

    def test_output_ownership_and_guide_hash_fail_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output = runner.new_output(root / "output")
            with self.assertRaises(FileExistsError):
                runner.new_output(output)
            with self.assertRaises(ValueError):
                runner.new_output(runner.ROOT / "forbidden")
            with self.assertRaises(ValueError):
                runner.new_output(runner.ROOT.parent)
            guide = root / "guide"
            guide.write_bytes(b"fake v4")
            with self.assertRaises(ValueError):
                runner.qualify(root / "unused", guide, "0" * 64, root / "missing")
            self.assertFalse((root / "unused").exists())

    def test_plan_identity_order_privacy_and_no_live_execution(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            guide = (b"<!-- BEGIN REPOGRAMMAR MANAGED SECTION -->\n"
                     b"<!-- REPOGRAMMAR MANAGED CONTENT VERSION: 4 -->\n"
                     b"shape only test\n<!-- END REPOGRAMMAR MANAGED SECTION -->")
            (root / "guide").write_bytes(guide)
            (root / "product").write_bytes(b"offline binary sentinel")
            control = lambda cell, arm, guide, product: {"arm": arm, "status": "SANDBOX_UNAVAILABLE"}
            with patch.object(runner, "native_control", side_effect=control):
                first = runner.qualify(root / "first", root / "guide", runner.sha(guide), root / "product")
                runner.qualify(root / "second", root / "guide", runner.sha(guide), root / "product")
            self.assertEqual((root / "first/plan.json").read_bytes(), (root / "second/plan.json").read_bytes())
            plan = json.loads((root / "first/plan.json").read_text())
            self.assertEqual(len(plan["runs"]), 160)
            for offset in range(0, 160, 4):
                block = plan["runs"][offset:offset + 4]
                self.assertEqual({r["arm"] for r in block}, set(runner.ARMS))
                self.assertEqual(len({r["source_sha256"] for r in block}), 1)
                self.assertEqual(len({r["base_prompt_sha256"] for r in block}), 1)
                self.assertEqual(len({r["delivered_prompt_sha256"] for r in block if r["arm"] != "B3"}), 1)
                self.assertNotEqual(next(r for r in block if r["arm"] == "B3")["delivered_prompt_sha256"], block[0]["base_prompt_sha256"])
            for row in plan["runs"]:
                cell = root / "first/cells" / str(row["ordinal"])
                self.assertEqual((cell / "home/.claude/CLAUDE.md").exists(), row["arm"] == "B2")
                self.assertFalse(any(p.name in ("AGENTS.md", "CLAUDE.md", "oracle.json") for p in (cell / "worktree").rglob("*")))
                self.assertIsNone(row["task_success"])
                self.assertIsNone(row["cost_usd"])
            self.assertEqual(first["status"], "CONTROL_ISOLATION_BLOCKED")
            self.assertEqual(first["global_file_discovery"], "NOT_MEASURED")
            rendered = json.dumps(first)
            self.assertNotIn(str(root), rendered)
            self.assertNotIn("shape only test", rendered)
            self.assertNotIn("prompt.txt", rendered)


if __name__ == "__main__":
    unittest.main()
