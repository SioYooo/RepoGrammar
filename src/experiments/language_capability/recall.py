#!/usr/bin/env python3
"""Offline, post-hoc role-prefix candidate recall; never changes primary gold."""

import argparse
import hashlib
import json
from pathlib import Path


HERE = Path(__file__).resolve().parent
PREREGISTRATION_SHA256 = "9854a75da31e451b78b6ba5d46905fef98cc905bb07dac1719cf1cdf7f0c2e64"
K = 5


def unique_rows(rows, key):
    result = {}
    for row in rows:
        identity = row.get(key)
        if not isinstance(identity, str) or not identity or identity in result:
            raise ValueError(f"missing or duplicate {key}")
        result[identity] = row
    return result


def complete_inventory(payload):
    families = payload.get("families")
    if (payload.get("status") != "ok" or payload.get("command") != "families"
            or payload.get("implemented") is not True or not isinstance(families, list)
            or payload.get("stale_count") != 0 or payload.get("cannot_verify_count") != 0
            or payload.get("fresh_count") != len(families)):
        raise ValueError("family inventory is unavailable, stale, or incomplete")
    identities = unique_rows(families, "family_id")
    if any(not identity.startswith("family:") for identity in identities):
        raise ValueError("malformed inventory family ID")
    return set(identities)


def score(relevant, returned):
    if not relevant:
        raise ValueError("empty relevance set cannot define recall")
    if not isinstance(returned, list) or any(not isinstance(x, str) or not x for x in returned):
        raise ValueError("malformed ordered candidate IDs")
    # Slice before set conversion: duplicate candidates consume ranks, not hits.
    top = returned[:K]
    hits = sorted(relevant.intersection(top))
    return {"returned_top_k": top, "hit_family_ids": hits, "hit_count": len(hits),
            "recall_at_5": len(hits) / len(relevant), "hit_at_5": bool(hits)}


def verify_result(result, frozen, inventories, versions, condition):
    if (result.get("schema_version") != "product-eval-results.v3"
            or result.get("condition") != condition or result.get("baseline") is not None):
        raise ValueError("wrong product result schema or arm identity")
    fixtures = unique_rows(result["fixtures"], "fixture_id")
    if set(fixtures) != set(inventories) or any(
            row.get("fixture_version") != versions[identity] for identity, row in fixtures.items()):
        raise ValueError("result fixture identity differs from captured source")
    rows = unique_rows(result["results"], "query_id")
    if set(rows) != {q["id"] for q in frozen}:
        raise ValueError("result query denominator differs from preregistration")
    for query in frozen:
        row = rows[query["id"]]
        expected = {"outcome": query["expected"]}
        if "family_prefix" in query:
            expected["family_prefix"] = query["family_prefix"]
        if (row.get("fixture_id"), row.get("target"), row.get("intent"),
                row.get("operation"), row.get("expected")) != (
                query["corpus"], query["target"], query["intent"], "find", expected):
            raise ValueError("result query identity or original gold changed")
        actual = row["actual"]
        if actual.get("outcome") not in ("ok", "unknown", "partial_context"):
            raise ValueError("operational error is not a valid candidate ranking")
        candidates = actual.get("candidate_families")
        if (not isinstance(candidates, list)
                or any(not isinstance(x, str) or x not in inventories[query["corpus"]]
                       for x in candidates)
                or actual.get("candidate_family_count") != len(candidates)):
            raise ValueError("candidate IDs or count do not match the captured inventory")
    return rows


def summarize(preregistration, capture, results, inventories):
    if (capture.get("preregistration_sha256") != PREREGISTRATION_SHA256
            or capture.get("pins_rechecked_after") is not True
            or capture.get("binary_sha256", {}).get("old") != preregistration["baseline_binary_sha256"]):
        raise ValueError("capture identity or producer pins differ from frozen evaluation")
    queries = preregistration["prospective_queries"]
    unique_rows(queries, "id")
    corpus_ids = set(unique_rows(preregistration["corpora"], "id"))
    if set(inventories) != corpus_ids:
        raise ValueError("inventory corpus identities differ from preregistration")
    analyses = unique_rows(capture["analyses"], "fixture")
    versions, baseline_ids = {}, {}
    for corpus in sorted(corpus_ids):
        analysis = analyses[corpus]
        if (analysis.get("old_candidate_equal") is not True
                or analysis["source_sha256"] != analysis["materialized_source_sha256"]):
            raise ValueError("captured canonical analyses or source identities differ")
        versions[corpus] = analysis["source_sha256"]
        old, candidate = inventories[corpus]
        baseline_ids[corpus] = complete_inventory(old)
        if complete_inventory(candidate) != baseline_ids[corpus] or old != candidate:
            raise ValueError("candidate complete inventory differs from baseline")
    arms = {arm: verify_result(results[arm], queries, baseline_ids, versions, condition)
            for arm, condition in (("old", "old_product"), ("candidate", "candidate_product"))}
    rows = []
    for query in queries:
        if query["intent"] != "retrieval":
            continue
        relevant = {identity for identity in baseline_ids[query["corpus"]]
                    if identity.startswith(query["family_prefix"])}
        row = {"query_id": query["id"], "fixture_id": query["corpus"],
               "family_prefix": query["family_prefix"], "relevant_family_ids": sorted(relevant),
               "gold_set_size": len(relevant)}
        for arm, records in arms.items():
            actual = records[query["id"]]["actual"]
            row[arm] = {**score(relevant, actual["candidate_families"]),
                        "product_outcome": actual["outcome"],
                        "selected_family": actual.get("selected_family")}
        rows.append(row)
    if not rows:
        raise ValueError("no retrieval queries to score")
    aggregates = {}
    for arm in arms:
        numerator = sum(row[arm]["hit_count"] for row in rows)
        denominator = sum(row["gold_set_size"] for row in rows)
        hit_queries = sum(row[arm]["hit_at_5"] for row in rows)
        aggregates[arm] = {
            "micro_recall_at_5": {"num": numerator, "den": denominator, "value": numerator / denominator},
            "macro_recall_at_5": sum(row[arm]["recall_at_5"] for row in rows) / len(rows),
            "hit_at_5": {"num": hit_queries, "den": len(rows), "value": hit_queries / len(rows)},
        }
    return {
        "schema_version": "qualified-phrase-supplementary-recall.v1",
        "evidence_label": "POSTHOC_DESCRIPTIVE_ROLE_PREFIX", "metric_kind": "DERIVED", "k": K,
        "definition": "Relevance is every baseline inventory family ID matching the same frozen prefix. "
                      "Intersect unique relevant IDs with the first five recorded candidates without reordering "
                      "or rank compaction. Micro recall sums hits/gold sizes; macro recall averages query recalls. "
                      "Hit@5 is the fraction of retrieval queries with at least one relevant candidate.",
        "claim_limit": "Candidate-list coverage only, including rankings from UNKNOWN outcomes. "
                       "Not primary-grader recall, independent task gold, committed-answer success, exact "
                       "variant/member correctness, or a causal/generalization claim. Original gold and "
                       "the primary public Recall@5 NOT_MEASURED result remain unchanged.",
        "query_count": len(queries), "retrieval_query_count": len(rows),
        "excluded_non_retrieval_count": len(queries) - len(rows),
        "complete_inventory_equal": True,
        "inventory_family_counts": {corpus: len(ids) for corpus, ids in baseline_ids.items()},
        "rows": rows, "aggregates": aggregates,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preregistration", type=Path, default=HERE / "preregistration.json")
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--capture", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.out.resolve().is_relative_to(HERE.parents[2]):
        parser.error("supplementary evidence must stay outside the repository")
    script = Path(__file__).resolve()
    files = {"script": {"path": str(script), "sha256": hashlib.sha256(script.read_bytes()).hexdigest()}}

    def load(name, path):
        raw = path.read_bytes()
        if len(raw) > 16 * 1024 * 1024:
            raise ValueError("input exceeds the local evidence size bound")
        files[name] = {"path": str(path.resolve()), "sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw)}
        return json.loads(raw)

    preregistration = load("preregistration", args.preregistration)
    if files["preregistration"]["sha256"] != PREREGISTRATION_SHA256:
        parser.error("frozen preregistration changed")
    capture = load("capture", args.capture / "capture-summary.json")
    results = {"old": load("old_result", args.baseline), "candidate": load("candidate_result", args.candidate)}
    inventories = {corpus["id"]: tuple(load(f"{corpus['id']}_{arm}_inventory",
                   args.capture / corpus["id"] / arm / "families.stdout") for arm in ("old", "candidate"))
                   for corpus in preregistration["corpora"]}
    if any(files[f"{corpus}_old_inventory"]["sha256"] != files[f"{corpus}_candidate_inventory"]["sha256"]
           for corpus in inventories):
        raise ValueError("old/candidate inventory bytes differ")
    report = summarize(preregistration, capture, results, inventories)
    report["inventory_raw_bytes_equal"] = True
    if any(hashlib.sha256(Path(entry["path"]).read_bytes()).hexdigest() != entry["sha256"]
           for entry in files.values()):
        raise ValueError("input or script changed during offline scoring")
    report["inputs"] = files
    report["input_hashes_rechecked_after"] = True
    with args.out.open("x") as output:
        json.dump(report, output, indent=2)
        output.write("\n")
    print(json.dumps(report["aggregates"]))


if __name__ == "__main__":
    main()
