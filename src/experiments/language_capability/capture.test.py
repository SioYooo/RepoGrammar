import hashlib
import json
from pathlib import Path
import tempfile
import unittest

import capture


def raw(value):
    return json.dumps(value, separators=(",", ":")).encode()


class CaptureContracts(unittest.TestCase):
    def test_only_exact_metadata_path_and_value_are_allowed(self):
        old = {"product_readiness":{"query_retrieval":{"vocabulary_version":"query-vocabulary.v2"}}, "family_id":"same"}
        new = {"product_readiness":{"query_retrieval":{"vocabulary_version":"query-vocabulary.v3"}}, "family_id":"same"}
        self.assertTrue(capture.parity(raw(old), raw(new))["equal"])
        new["family_id"] = "different"
        self.assertFalse(capture.parity(raw(old), raw(new))["equal"])
        self.assertFalse(capture.parity(raw({"other":{"vocabulary_version":"query-vocabulary.v2"}}), raw({"other":{"vocabulary_version":"query-vocabulary.v3"}}))["equal"])

    def test_parity_does_not_erase_order_or_unrelated_metadata(self):
        self.assertFalse(capture.parity(b'{"a":1,"b":2}', b'{"b":2,"a":1}')["equal"])
        self.assertFalse(capture.parity(raw({"query_retrieval":{"vocabulary_version":"old"}}), raw({"query_retrieval":{"vocabulary_version":"query-vocabulary.v3"}}))["equal"])

    def test_abstention_requires_successful_index_query_and_no_selection(self):
        payload = {"status":"UNKNOWN", "unknowns":[{"reason":"InsufficientSupport"}], "family":None}
        self.assertTrue(capture.true_abstention(payload, 0, True))
        self.assertFalse(capture.true_abstention(payload, 1, True))
        self.assertFalse(capture.true_abstention(payload, 0, False))
        for owner in ("family", "query_route", "certificate", "alignment_certificate"):
            key = "family_id" if owner in ("family", "certificate") else "selected_family_id"
            self.assertFalse(capture.true_abstention({**payload, owner:{key:"family:fake"}}, 0, True))
        self.assertFalse(capture.true_abstention({"status":"FALLBACK", "unknowns":payload["unknowns"]}, 0, True))
        for invalid in ({**payload, "certificate":{"family_id":"family:wrong"}}, {"status":"FALLBACK"}, {"status":"error"}):
            with self.assertRaises(ValueError):
                capture.qualify_outcome(invalid, 0, True)

    def test_read_plan_bounds_hash_and_source_guard(self):
        with tempfile.TemporaryDirectory() as directory:
            project = Path(directory).resolve()
            data = b"def safe():\n    return 1\n"
            (project / "app.py").write_bytes(data)
            item = {"path":"app.py", "start_byte":0, "end_byte":len(data),
                    "content_hash":"sha256:" + hashlib.sha256(data).hexdigest(), "source_snippets_included":False,
                    "purpose":"target_body_required_for_edit", "start_line":1, "end_line":2,
                    "source_required_before_edit":True, "estimated_tokens":42, "why":"read the resolved source before editing"}
            payload = {"schema_version":"product-schemas.v1", "status":"PARTIAL_CONTEXT", "read_plan":{"items":[item]}}
            self.assertEqual(capture.validate_payload(raw(payload), project)["items"], 1)
            item["end_byte"] += 1
            with self.assertRaisesRegex(ValueError, "bounds"):
                capture.validate_payload(raw(payload), project)
            item["end_byte"] -= 1
            item["content_hash"] = hashlib.sha256(data).hexdigest()
            with self.assertRaisesRegex(ValueError, "hash"):
                capture.validate_payload(raw(payload), project)
            payload.update(status="UNKNOWN", unknowns=[{"reason":"StaleEvidence"}])
            with self.assertRaisesRegex(ValueError, "hash"):
                capture.validate_payload(raw(payload), project, "app.py")
            item["content_hash"] = "sha256:" + "0" * 64
            with self.assertRaisesRegex(ValueError, "hash"):
                capture.validate_payload(raw(payload), project, "other.py")
            self.assertEqual(capture.validate_payload(raw(payload), project, "app.py")["stale_items"], 1)
            payload["family"] = {"family_id":"family:example"}
            with self.assertRaisesRegex(ValueError, "hash"):
                capture.validate_payload(raw(payload), project, "app.py")
            with self.assertRaisesRegex(ValueError, "source-bearing"):
                capture.validate_payload(raw({"text":capture.SENTINEL}), project)

    def test_selected_family_requires_nonempty_read_plan(self):
        for plan in (None, {"items":[]}):
            payload = {"schema_version":"product-schemas.v1", "status":"ok", "family":{"family_id":"family:example"}}
            if plan is not None:
                payload.update(schema_version="product-schemas.v1", read_plan=plan)
            with self.assertRaisesRegex(ValueError, "read plan"):
                capture.validate_payload(raw(payload), Path("/unread"))

    def test_public_manifest_gold_and_source_are_verified(self):
        with tempfile.TemporaryDirectory() as directory:
            sources = Path(directory)
            snapshot = sources / "public" / "snapshot"
            snapshot.mkdir(parents=True)
            (snapshot / "source.py").write_text("value = 1\n")
            prereg = {"corpora":[{"id":"public", "url":"https://example.invalid/repo", "commit":"a" * 40}], "prospective_queries":[{
                "id":"q", "corpus":"public", "target":"HTTP handler", "intent":"retrieval", "expected":"ok", "family_prefix":"family:python:fastapi_route"}]}
            public = {"fixtures":[{"fixture_id":"public", "root":str(snapshot)}], "queries":[{
                "query_id":"q", "fixture_id":"public", "target":"HTTP handler", "intent":"retrieval", "operation":"find", "mode":"compact",
                "expected":{"outcome":"ok", "family_prefix":"family:python:fastapi_route"}}]}
            provenance = {"preregistration_sha256":capture.PREREGISTRATION_SHA256, "corpora":[{
                "corpus_id":"public", "url":"https://example.invalid/repo", "commit":"a" * 40,
                "fixture_version":capture.tree_sha256(str(snapshot))}]}
            capture.verify_public_manifest(prereg, public, sources, provenance)
            provenance["corpora"][0]["commit"] = "b" * 40
            with self.assertRaisesRegex(ValueError, "commit"):
                capture.verify_public_manifest(prereg, public, sources, provenance)
            provenance["corpora"][0]["commit"] = "a" * 40
            public["queries"][0]["expected"]["outcome"] = "unknown"
            with self.assertRaisesRegex(ValueError, "gold"):
                capture.verify_public_manifest(prereg, public, sources, provenance)
            public["queries"][0]["expected"]["outcome"] = "ok"
            (snapshot / "source.py").write_text("value = 2\n")
            with self.assertRaisesRegex(ValueError, "identity"):
                capture.verify_public_manifest(prereg, public, sources, provenance)


if __name__ == "__main__":
    unittest.main()
