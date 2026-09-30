"""Offline tests; scripted transcripts are not adoption observations."""
import json
from pathlib import Path
import re
import unittest

from adoption_v2 import current_mcp, measure
from parsers import MCP_TOOL_NAME, _parse_mcp_result


def call(name, tid, inp):
    return {"type": "assistant", "message": {"content": [
        {"type": "tool_use", "name": name, "id": tid, "input": inp}]}}


def result(tid, content, error=False):
    return {"type": "user", "message": {"content": [
        {"type": "tool_result", "tool_use_id": tid, "content": content, "is_error": error}]}}


def payload(status="ok", reason=None):
    return json.dumps({"schema_version": "product-schemas.v1", "status": status,
                       "family": {"family_id": "family:python:route"} if status == "ok" else None,
                       "read_plan": {"items": [{"path": "app/routes.py"}]},
                       "unknowns": [{"reason": reason}] if reason else []})


class CurrentContractTests(unittest.TestCase):
    def test_reproduces_historical_shape_gap_without_mutating_rq5(self):
        text = payload()
        self.assertEqual(_parse_mcp_result(text)["read_plan_count"], 0)
        self.assertIsNone(_parse_mcp_result(text)["selected_family"])
        parsed = current_mcp(text, "/study")
        self.assertEqual(parsed["items"], 1)
        self.assertEqual(parsed["family"], "family:python:route")

    def test_real_committed_mcp_pregolden(self):
        source = (Path(__file__).resolve().parents[2] / "rust/interfaces/mcp/mod.rs").read_text()
        text = re.search(r'const FIND_ANALOGUES_FOUND_PREGOLDEN_V0: &str = r#"(.*?)"#;', source, re.S)[1]
        parsed = current_mcp(text, "/study")
        self.assertEqual(parsed["items"], 1)
        self.assertEqual(parsed["paths"], {"src/routes/a.ts"})
        self.assertEqual(parsed["family"], "family:typescript:express_route:express")

    def test_stale_and_unknown_unread_edit(self):
        for reason in ("StaleEvidence", "InsufficientSupport"):
            events = [call(MCP_TOOL_NAME, "m", {}), result("m", payload("UNKNOWN", reason)),
                      call("Edit", "e", {"file_path": "/study/app/routes.py"})]
            counts = measure(events, "/study")["counts"]
            self.assertEqual(counts["unknown_unread_edits"], 1)
            self.assertEqual(counts["stale_unread_edits"], int(reason == "StaleEvidence"))

    def test_successful_read_control_and_failed_read_negative(self):
        for error in (False, True):
            events = [call(MCP_TOOL_NAME, "m", {}), result("m", payload("UNKNOWN", "StaleEvidence")),
                      call("Read", "r", {"file_path": "app/routes.py"}), result("r", "x", error),
                      call("Edit", "e", {"file_path": "app/routes.py"})]
            self.assertEqual(measure(events, "/study")["counts"]["stale_unread_edits"], int(error))

    def test_turn_partition_missing_usage_and_source_free_output(self):
        events = [call("Grep", "g", {"pattern": "secret"}), result("g", "private source"),
                  call(MCP_TOOL_NAME, "m", {}), result("m", payload()),
                  call("Read", "r", {"file_path": "app/routes.py", "offset": 1, "limit": 2}),
                  result("r", "é"), call("Bash", "b", {"command": "cat secret.py"})]
        measured = measure(events, "/study")
        self.assertEqual(measured["first_invocation_turn"], 2)
        self.assertEqual(measured["counts"]["broad_before"], 1)
        self.assertEqual(measured["counts"]["broad_after"], 0)
        self.assertEqual(measured["counts"]["read_result_bytes"], 2)
        self.assertEqual(measured["counts"]["unclassified_shell_calls"], 1)
        self.assertEqual(measured["distinct_files_acquired"], 1)
        self.assertIsNone(measured["host"]["turns"])
        self.assertIsNone(measured["host"]["cost_usd"])
        encoded = json.dumps(measured)
        for text in ("private source", "app/routes.py", "secret.py", "family:python:route"):
            self.assertNotIn(text, encoded)

    def test_malformed_and_conflicting_output(self):
        for text in ("not JSON", '{"schema_version":"future"}',
                     payload().replace('"read_plan": {"items":', '"read_plan": {"items_bad":')[:-1]):
            measured = measure([call(MCP_TOOL_NAME, "m", {}), result("m", text)], "/study")
            self.assertEqual(measured["counts"]["mcp_parse_errors"], 1)
        p = json.loads(payload())
        p["query_route"] = {"selected_family_id": "another"}
        with self.assertRaises(ValueError):
            current_mcp(json.dumps(p), "/study")

    def test_unscoped_safety_is_explicit_and_incidental_text_is_not_a_reason(self):
        p = json.loads(payload())
        p["note"] = "StaleEvidence UNKNOWN InsufficientSupport"
        self.assertFalse(current_mcp(json.dumps(p), "/study")["stale"])
        p.update(status="UNKNOWN", read_plan=None)
        measured = measure([call(MCP_TOOL_NAME, "m", {}), result("m", json.dumps(p))], "/study")
        self.assertEqual(measured["counts"]["unscoped_abstentions"], 1)

    def test_denied_index_attempt_is_counted_not_successful_acquisition(self):
        events = [call("Read", "r", {"file_path": ".repogrammar/repogrammar.sqlite"}),
                  result("r", "denied", True)]
        measured = measure(events, "/study")
        self.assertEqual(measured["counts"]["index_access_attempts"], 1)
        self.assertEqual(measured["distinct_files_acquired"], 0)
        measured = measure([call("Read", "r", {"file_path": ".repogrammar-example.md"}),
                            call("Bash", "b", {"command": "echo .repogrammar"})], "/study")
        self.assertEqual(measured["counts"]["index_access_attempts"], 0)
        self.assertEqual(measured["counts"]["shell_index_keyword_flags"], 1)

    def test_host_negative_or_nonfinite_usage_rejected(self):
        for value in (-1, float("nan"), True, "prompt text"):
            with self.assertRaises(ValueError):
                measure([{"type": "result", "usage": {"input_tokens": value}}], "/study")

    def test_unpaired_or_duplicate_tools_refused(self):
        with self.assertRaises(ValueError):
            measure([result("unknown", "body")], "/study")
        with self.assertRaises(ValueError):
            measure([call("Read", "r", {}), call("Read", "r", {})], "/study")
        with self.assertRaises(ValueError):
            measure([call("Read", "r", {}), result("r", ""), call(MCP_TOOL_NAME, "r", {})], "/study")

    def test_read_started_before_stale_result_is_not_fresh_recovery(self):
        events = [call(MCP_TOOL_NAME, "m", {}), call("Read", "r", {"file_path": "app/routes.py"}),
                  result("m", payload("UNKNOWN", "StaleEvidence")), result("r", "old source"),
                  call("Edit", "e", {"file_path": "app/routes.py"})]
        self.assertEqual(measure(events, "/study")["counts"]["stale_unread_edits"], 1)

    def test_duplicate_stale_results_count_one_unread_edit(self):
        events = [call(MCP_TOOL_NAME, "m1", {}), result("m1", payload("UNKNOWN", "StaleEvidence")),
                  call(MCP_TOOL_NAME, "m2", {}), result("m2", payload("UNKNOWN", "StaleEvidence")),
                  call("Edit", "e", {"file_path": "app/routes.py"})]
        self.assertEqual(measure(events, "/study")["counts"]["stale_unread_edits"], 1)

    def test_malformed_items_and_unknowns_rejected(self):
        for key, value in (("read_plan", {"items": ["bad"]}),
                           ("read_plan", {"items": [{"path": {}}]}), ("unknowns", {})):
            p = json.loads(payload())
            p[key] = value
            with self.assertRaises(ValueError):
                current_mcp(json.dumps(p), "/study")
        for key in ("read_plan", "source_spans", "family", "query_route", "resolved_target"):
            for value in ([], False, 0, ""):
                p = json.loads(payload())
                p[key] = value
                with self.assertRaises(ValueError):
                    current_mcp(json.dumps(p), "/study")

    def test_scoped_stale_omission_and_duplicate_json_refusal(self):
        for owner, key in (("source_spans", "omissions"), ("read_plan", "line_range_omissions")):
            p = json.loads(payload())
            p.setdefault(owner, {})[key] = [{"path": "app/other.py", "reason": "stale_evidence"}]
            events = [call(MCP_TOOL_NAME, "m", {}), result("m", json.dumps(p)),
                      call("Edit", "e", {"file_path": "app/routes.py"}),
                      call("Edit", "e2", {"file_path": "app/other.py"})]
            self.assertEqual(measure(events, "/study")["counts"]["stale_unread_edits"], 1)
        with self.assertRaises(ValueError):
            current_mcp(payload().replace('"status": "ok"', '"status":"UNKNOWN","status":"ok"'), "/study")

    def test_invalid_read_bounds_remain_broad_and_drive_paths_are_refused(self):
        measured = measure([call("Read", "r", {"offset": None, "limit": -1})], "/study")
        self.assertEqual(measured["counts"]["broad_before"], 1)
        p = json.loads(payload())
        p["read_plan"]["items"][0]["path"] = "C:/private/source.py"
        with self.assertRaises(ValueError):
            current_mcp(json.dumps(p), "/study")


if __name__ == "__main__":
    unittest.main()
