"""Offline falsification checks; tests never start a real RepoGrammar daemon."""

import importlib.util
from contextlib import closing
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest


spec = importlib.util.spec_from_file_location("autosync_eval", Path(__file__).with_name("autosync_eval.py"))
evaluation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(evaluation)


class AutosyncEvaluationTests(unittest.TestCase):
    def test_resource_units_and_unsupported_are_not_invented(self):
        mac = evaluation.native_resources("1.0 real 0.2 user 0.1 sys\n4096 maximum resident set size\n", "darwin")
        linux = evaluation.native_resources("User time (seconds): 0.2\nSystem time (seconds): 0.1\nElapsed (wall clock) time (h:mm:ss or m:ss): 0:01.0\nMaximum resident set size (kbytes): 4\n", "linux")
        self.assertEqual(mac["peak_rss_bytes"], linux["peak_rss_bytes"])
        self.assertEqual(mac["state"], "MEASURED")
        for text, platform in (("0.00 real 0.00 user 0.00 sys", "darwin"), ("1.0 real 0.2 user 0.1 sys\n-1 maximum resident set size\n", "darwin"), ("", "linux"), ("", "win32")):
            result = evaluation.native_resources(text, platform)
            self.assertEqual(result["state"], "NOT_MEASURED")
            self.assertNotIn("peak_rss_bytes", result)

    def test_deadline_uses_injected_monotonic_clock(self):
        now = [0.0]
        sleep = lambda seconds: now.__setitem__(0, now[0] + seconds)
        self.assertEqual(evaluation.wait_until(lambda: "ready" if now[0] >= 0.2 else False, 1, lambda: now[0], sleep), "ready")
        with self.assertRaises(evaluation.HarnessError):
            evaluation.wait_until(lambda: False, 0.3, lambda: now[0], sleep)
        self.assertLessEqual(now[0], 0.50000001)

    def test_foreign_or_dead_or_malformed_lock_never_authorizes_stop(self):
        lock = {"kind": "autosync_daemon", "phase": "ready", "pid": 42}
        self.assertEqual(evaluation.owned_lock_pid(lock, 7, True, lambda pid: 7), 42)
        for altered, alive, group in ((lock, False, 7), (lock, True, 8), ({**lock, "pid": True}, True, 7), ({**lock, "phase": "starting"}, True, 7), ({**lock, "kind": "foreign"}, True, 7)):
            with self.assertRaises(evaluation.HarnessError):
                evaluation.owned_lock_pid(altered, 7, alive, lambda pid: group)

    def test_bounded_read_refuses_oversized_without_whole_file_read(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "data"
            path.write_bytes(b"x" * 9)
            with self.assertRaises(evaluation.HarnessError):
                evaluation.bounded_read(path, 8)
            self.assertEqual(evaluation.bounded_read(path, 9), b"x" * 9)

    def test_readonly_snapshot_and_work_counts_match_real_log_shape(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            state = repo / ".repogrammar"
            state.mkdir()
            database = state / "repogrammar.sqlite"
            with closing(sqlite3.connect(database)) as connection:
                connection.executescript("CREATE TABLE index_generations(generation_id TEXT, status TEXT); CREATE TABLE indexed_files(generation_id TEXT, path TEXT, content_hash TEXT); INSERT INTO index_generations VALUES('gen-000001','validated'),('gen-000002','active'); INSERT INTO indexed_files VALUES('gen-000002','app.py','hash');")
            result = evaluation.state_snapshot(repo)
            self.assertEqual(result["generation_count"], 2)
            self.assertEqual(result["hashes"], {"app.py": "hash"})
            (state / "logs").mkdir()
            (state / "logs/daemon.log").write_text("autosync: native events enabled; periodic metadata reconciliation every 60s\nautosync: incremental sync +0 ~1 -0 unchanged 1 copied 1 reparsed 1 file(s), 4 unit(s) in 25ms (generation gen-000002)\n", encoding="utf-8")
            summary = evaluation.log_summary(repo)
            self.assertEqual(summary["syncs"], 1)
            self.assertEqual(summary["reparsed_files"], 1)
            self.assertEqual(summary["copied_forward_files"], 1)
            self.assertEqual(summary["sync_wall_ms"], 25)
            self.assertEqual(summary["generations"], ["gen-000002"])
            self.assertTrue(summary["native_events"])
            self.assertNotIn("app.py", json.dumps(summary))


if __name__ == "__main__":
    unittest.main()
