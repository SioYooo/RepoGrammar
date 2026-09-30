"""Offline checks only; no product/native processes are launched."""

from pathlib import Path
import tempfile
import unittest

from autosync_eval import HarnessError
import incremental_pair as pair


class IncrementalPairTests(unittest.TestCase):
    def test_frozen_expansion_patch_and_alternating_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            left, right = Path(tmp) / "left", Path(tmp) / "right"
            first = pair.prepare_fixture(pair.ROOT / "src/fixtures/evaluation/performance", left)
            second = pair.prepare_fixture(pair.ROOT / "src/fixtures/evaluation/performance", right)
            self.assertEqual(first, second)
            self.assertEqual(pair.tree_sha256(str(left)), pair.tree_sha256(str(right)))
            self.assertEqual(len(list(left.glob("*.py"))), 18)
            patched = pair.variant(first, True)
            self.assertEqual(patched.count(b"return [1]"), 1)
            self.assertEqual(patched.count(b"return []"), 2)
            self.assertEqual(pair.variant(first, False), first)
            self.assertEqual(pair.ordered_arms(0), ("baseline", "candidate"))
            self.assertEqual(pair.ordered_arms(1), ("candidate", "baseline"))
            extra = Path(tmp) / "unexpected"
            extra.mkdir()
            with self.assertRaises(HarnessError):
                pair.prepare_fixture(extra, Path(tmp) / "rejected")

    def test_reparse_regression_and_same_count_semantic_change_fail(self):
        value = {"status": "complete", "sync_mode": "incremental", "fallback_reason": None, "modified_files": 1, "reparsed_files": 1}
        self.assertEqual(pair.qualified_outcome(value)["reparsed_files"], 1)
        for change in ({"status": "ok"}, {"status": "failed"}, {"status": "UNKNOWN"}, {"sync_mode": "full_rebuild"}, {"reparsed_files": 18}, {"modified_files": 0}, {"fallback_reason": "python_context_budget"}):
            with self.assertRaises(HarnessError):
                pair.qualified_outcome({**value, **change})
        self.assertEqual(pair.parity({"rows": 1, "hash": "same"}, {"hash": "same", "rows": 1}), pair.parity({"rows": 1, "hash": "same"}, {"rows": 1, "hash": "same"}))
        with self.assertRaises(HarnessError):
            pair.parity({"rows": 1, "hash": "before"}, {"rows": 1, "hash": "after"})

    def test_paired_summary_uses_deltas_and_keeps_missing_ratio_null(self):
        samples = []
        for before, after in ((1, 2), (3, 2), (2, 2)):
            samples.append({"arms": {arm: {"resources": {field: value for field in ("wall_seconds", "cpu_seconds", "peak_rss_bytes")}} for arm, value in (("baseline", before), ("candidate", after))}})
        result = pair.summary(samples)["wall_seconds"]
        self.assertEqual(result["paired_delta_median"], 0)
        self.assertEqual(result["paired_ratio_median"], 1)
        samples[0]["arms"]["baseline"]["resources"]["wall_seconds"] = 0
        self.assertIsNone(pair.summary(samples)["wall_seconds"]["paired_ratio_median"])


if __name__ == "__main__":
    unittest.main()
