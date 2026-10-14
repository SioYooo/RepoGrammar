#!/usr/bin/env python3
"""Small offline checks for descriptive recall and evidence identity."""

import copy
import unittest

import recall


def fixture():
    queries = [
        {"id": "a", "corpus": "sample", "target": "role a", "intent": "retrieval",
         "expected": "ok", "family_prefix": "family:a:"},
        {"id": "b", "corpus": "sample", "target": "role b", "intent": "retrieval",
         "expected": "ok", "family_prefix": "family:b:"},
        {"id": "negative", "corpus": "sample", "target": "unsupported", "intent": "abstention",
         "expected": "unknown"},
    ]
    preregistration = {"baseline_binary_sha256": "old-sha", "corpora": [{"id": "sample"}],
                       "prospective_queries": queries}
    capture = {"preregistration_sha256": recall.PREREGISTRATION_SHA256, "pins_rechecked_after": True,
               "binary_sha256": {"old": "old-sha"}, "analyses": [{"fixture": "sample",
               "source_sha256": "source-sha", "materialized_source_sha256": "source-sha",
               "old_candidate_equal": True}]}
    inventory = {"status": "ok", "command": "families", "implemented": True,
                 "fresh_count": 4, "stale_count": 0, "cannot_verify_count": 0,
                 "families": [{"family_id": value} for value in
                              ("family:a:1", "family:a:2", "family:a:3", "family:b:1")]}
    results = {}
    for arm in ("old", "candidate"):
        rows = []
        for query in queries:
            candidates = {"a": ["family:a:1", "family:a:1", "family:b:1", "family:a:2"],
                          "b": ["family:b:1"], "negative": []}[query["id"]]
            if arm == "candidate" and query["id"] == "a":
                candidates = ["family:a:1", "family:a:2", "family:a:3"]
            expected = {"outcome": query["expected"]}
            if "family_prefix" in query:
                expected["family_prefix"] = query["family_prefix"]
            rows.append({"query_id": query["id"], "fixture_id": "sample", "target": query["target"],
                         "intent": query["intent"], "operation": "find", "expected": expected,
                         "actual": {"outcome": "unknown", "candidate_families": candidates,
                                    "candidate_family_count": len(candidates), "selected_family": None}})
        results[arm] = {"schema_version": "product-eval-results.v3", "condition": f"{arm}_product",
                        "baseline": None, "fixtures": [{"fixture_id": "sample", "fixture_version": "source-sha"}],
                        "results": rows}
    return preregistration, capture, results, {"sample": (inventory, copy.deepcopy(inventory))}


class RecallTests(unittest.TestCase):
    def test_multiple_variants_micro_macro_and_abstention_exclusion(self):
        report = recall.summarize(*fixture())
        self.assertEqual(report["evidence_label"], "POSTHOC_DESCRIPTIVE_ROLE_PREFIX")
        self.assertEqual(report["excluded_non_retrieval_count"], 1)
        self.assertEqual(report["aggregates"]["old"]["micro_recall_at_5"], {"num": 3, "den": 4, "value": .75})
        self.assertAlmostEqual(report["aggregates"]["old"]["macro_recall_at_5"], 5 / 6)
        self.assertEqual(report["aggregates"]["old"]["hit_at_5"], {"num": 2, "den": 2, "value": 1})
        self.assertEqual(report["aggregates"]["candidate"]["micro_recall_at_5"]["value"], 1)
        self.assertEqual(report["rows"][0]["old"]["product_outcome"], "unknown")

    def test_duplicate_candidates_consume_ranks_without_inflating_hits(self):
        result = recall.score({"a", "f"}, ["a", "a", "b", "c", "d", "f"])
        self.assertEqual(result["hit_family_ids"], ["a"])
        self.assertEqual(result["recall_at_5"], .5)
        self.assertEqual(recall.score({"a"}, [])["recall_at_5"], 0)
        with self.assertRaises(ValueError):
            recall.score(set(), ["a"])

    def test_wrong_query_arm_source_gold_and_operational_error_rejected(self):
        for field, value in (("query_id", "wrong"), ("fixture_id", "wrong"), ("target", "changed"),
                             ("intent", "abstention"), ("operation", "check"), ("expected", {})):
            with self.subTest(field=field):
                args = fixture()
                args[2]["old"]["results"][0][field] = value
                with self.assertRaises(ValueError):
                    recall.summarize(*args)
        for field, value in (("outcome", "error"), ("candidate_families", ["family:absent"]),
                             ("candidate_family_count", 99)):
            with self.subTest(field=field):
                args = fixture()
                args[2]["old"]["results"][0]["actual"][field] = value
                with self.assertRaises(ValueError):
                    recall.summarize(*args)
        args = fixture()
        args[2]["old"]["condition"] = "candidate_product"
        with self.assertRaises(ValueError):
            recall.summarize(*args)
        args = fixture()
        args[2]["old"]["fixtures"][0]["fixture_version"] = "wrong-source"
        with self.assertRaises(ValueError):
            recall.summarize(*args)

    def test_duplicate_missing_queries_and_empty_relevance_rejected(self):
        for change in ("duplicate", "missing", "empty-relevance"):
            with self.subTest(change=change):
                args = fixture()
                if change == "duplicate":
                    args[2]["old"]["results"].append(copy.deepcopy(args[2]["old"]["results"][0]))
                elif change == "missing":
                    args[2]["old"]["results"].pop()
                else:
                    args[0]["prospective_queries"][0]["family_prefix"] = "family:absent:"
                    for arm in args[2].values():
                        arm["results"][0]["expected"]["family_prefix"] = "family:absent:"
                with self.assertRaises(ValueError):
                    recall.summarize(*args)

    def test_changed_duplicate_or_incomplete_inventory_rejected(self):
        for change in ("changed", "duplicate", "incomplete", "stale"):
            with self.subTest(change=change):
                args = fixture()
                candidate = args[3]["sample"][1]
                if change == "changed":
                    candidate["families"][0]["family_id"] = "family:other"
                elif change == "duplicate":
                    candidate["families"][1] = copy.deepcopy(candidate["families"][0])
                elif change == "incomplete":
                    candidate["families"].pop()
                else:
                    candidate["stale_count"] = 1
                with self.assertRaises(ValueError):
                    recall.summarize(*args)


if __name__ == "__main__":
    unittest.main()
