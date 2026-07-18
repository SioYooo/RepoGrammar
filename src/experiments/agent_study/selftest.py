"""Self-tests for the RQ5 agent-study harness modules.

Runnable with plain `python3 selftest.py` (stdlib unittest only; no new deps).
Covers: the tree-hash equivalence to the Rust harness, transcript parsing, the
three safety detectors against the four seeded fixtures, the mechanical grader
(T1 static assertions, T6 answer checklist), and the record schema + privacy
guard. These are the unit tests behind the dry run's pipeline assertions.
"""

from __future__ import annotations

import os
import shutil
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import detectors  # noqa: E402
import grader  # noqa: E402
import parsers  # noqa: E402
import record_schema  # noqa: E402
from detectors import path_suffix_match  # noqa: E402
from treehash import tree_sha256  # noqa: E402

REPO_ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
FX = os.path.join(HERE, "fixtures", "transcripts")
MINI = os.path.join(HERE, "fixtures", "mini_repo")

# The seeded transcripts are gitignored (transcript*.jsonl carries raw-transcript
# shape) and regenerate deterministically; bootstrap them on a fresh checkout.
if not os.path.isdir(FX):
    sys.path.insert(0, os.path.join(HERE, "fixtures"))
    import build_fixtures  # noqa: E402

    build_fixtures.main()


def fx(name: str) -> str:
    return os.path.join(FX, name)


class TreeHashTest(unittest.TestCase):
    def test_matches_rust_committed_hash(self):
        # The Rust harness (repo_guard.rs fixture_version_hash) recorded
        # python-v0_1 = 37fec96f7c7b... in docs/experiments/product-core-baseline.md.
        fixture = os.path.join(REPO_ROOT, "src", "fixtures", "python", "release", "v0_1")
        if not os.path.isdir(fixture):
            self.skipTest("python-v0_1 fixture not present")
        self.assertTrue(tree_sha256(fixture).startswith("37fec96f7c7b"))

    def test_deterministic_and_content_sensitive(self):
        d = tempfile.mkdtemp()
        try:
            os.makedirs(os.path.join(d, "sub"))
            with open(os.path.join(d, "a.txt"), "w") as fh:
                fh.write("alpha")
            with open(os.path.join(d, "sub", "b.txt"), "w") as fh:
                fh.write("beta")
            h1 = tree_sha256(d)
            self.assertEqual(h1, tree_sha256(d))  # deterministic
            with open(os.path.join(d, "sub", "b.txt"), "w") as fh:
                fh.write("beta2")
            self.assertNotEqual(h1, tree_sha256(d))  # content-sensitive
        finally:
            shutil.rmtree(d)


class ParserTest(unittest.TestCase):
    def test_clean_run_metrics(self):
        p = parsers.parse_transcript(fx("transcript_clean.jsonl"))
        self.assertEqual(p["model"], "claude-haiku-4-5-20251001")
        self.assertTrue(p["result_present"])
        self.assertEqual(p["mcp"]["calls"], 1)
        self.assertTrue(p["mcp"]["first_call_before_first_read"])
        self.assertEqual(p["context"]["files_read"], 1)
        self.assertGreater(p["context"]["bytes_read"], 0)
        self.assertEqual(p["num_turns"], 4)
        self.assertIsNotNone(p["tokens"]["total_cost_usd"])
        self.assertEqual(p["tokens"]["source"], "host_reported")

    def test_final_text_extracted_for_t6(self):
        p = parsers.parse_transcript(fx("transcript_t6_answer.jsonl"))
        self.assertIn("dependency injection", p["final_text"])
        self.assertEqual(p["mcp"]["calls"], 0)

    def test_mcp_operation_breakdown(self):
        p = parsers.parse_transcript(fx("transcript_t6_a3.jsonl"))
        self.assertEqual(p["mcp"]["calls"], 1)
        self.assertIn("find_analogues", p["mcp"]["by_operation"])

    def test_bash_read_gating(self):
        # S2: read-pattern Bash ('cat ...') result counts toward bytes_read;
        # a non-read Bash ('pytest') result must NOT (design §6). Only Read +
        # read-pattern Bash count.
        import json as _json

        def ln(o):
            return _json.dumps(o)

        tx = "\n".join([
            ln({"type": "system", "subtype": "init", "model": "m"}),
            ln({"type": "assistant", "message": {"content": [
                {"type": "tool_use", "id": "b1", "name": "Bash", "input": {"command": "cat app/x.py"}}]}}),
            ln({"type": "user", "message": {"content": [
                {"type": "tool_result", "tool_use_id": "b1", "content": "ABCD"}]}}),  # 4 bytes, counted
            ln({"type": "assistant", "message": {"content": [
                {"type": "tool_use", "id": "b2", "name": "Bash", "input": {"command": "pytest -q"}}]}}),
            ln({"type": "user", "message": {"content": [
                {"type": "tool_result", "tool_use_id": "b2", "content": "X" * 525}]}}),  # NOT counted
            ln({"type": "result", "subtype": "success",
                "usage": {"input_tokens": 1, "output_tokens": 1}, "num_turns": 1, "total_cost_usd": 0.0}),
        ])
        p = parsers.parse_transcript(tx)
        self.assertEqual(p["context"]["bytes_read"], 4)  # only the 'cat' result
        self.assertEqual(p["context"]["bash_read_calls"], 1)

    def test_mcp_additive_columns(self):
        # S5: MCP result bytes are reported as their own column (kept OUT of
        # bytes_read); read-plan count + selected family populate.
        p = parsers.parse_transcript(fx("transcript_a3_success.jsonl"))
        self.assertGreater(p["mcp"]["result_bytes"], 0)
        self.assertEqual(p["mcp"]["read_plan_item_count"], 1)
        self.assertIn("fastapi.route.detail", p["mcp"]["selected_family_ids"])
        # MCP result bytes are separate from context.bytes_read (Read result only).
        self.assertNotEqual(p["mcp"]["result_bytes"], p["context"]["bytes_read"])


class DetectorTest(unittest.TestCase):
    def _safety(self, name):
        p = parsers.parse_transcript(fx(name))
        return detectors.run_all(p["tool_events"], p["mcp_results"])

    def test_clean_fires_nothing(self):
        self.assertEqual(self._safety("transcript_clean.jsonl"),
                         {"index_peek": 0, "unknown_override": 0, "stale_evidence_use": 0})

    def test_stale_fires_stale_evidence_use(self):
        s = self._safety("transcript_stale.jsonl")
        self.assertEqual(s["stale_evidence_use"], 1)
        # StaleEvidence also arms unknown_override per design §6.
        self.assertEqual(s["unknown_override"], 1)
        self.assertEqual(s["index_peek"], 0)

    def test_unknown_fires_unknown_override_only(self):
        s = self._safety("transcript_unknown.jsonl")
        self.assertEqual(s["unknown_override"], 1)
        self.assertEqual(s["stale_evidence_use"], 0)

    def test_index_peek(self):
        s = self._safety("transcript_indexpeek.jsonl")
        self.assertEqual(s["index_peek"], 2)
        self.assertEqual(s["unknown_override"], 0)

    def test_override_suppressed_by_intervening_read(self):
        # Clean run reads the analogue between an ok MCP result and the edit;
        # even if we force the MCP result to unknown, an intervening Read of the
        # same path must suppress unknown_override.
        p = parsers.parse_transcript(fx("transcript_clean.jsonl"))
        for r in p["mcp_results"]:
            r["unknown"] = True
            r["affected_paths"] = ["backend/app/api/routes/items.py"]
        s = detectors.run_all(p["tool_events"], p["mcp_results"])
        self.assertEqual(s["unknown_override"], 0)  # Read of the path intervenes

    def test_path_suffix_match(self):
        self.assertTrue(path_suffix_match(
            "/tmp/run/worktree/backend/app/api/routes/items.py",
            "backend/app/api/routes/items.py"))
        self.assertFalse(path_suffix_match(
            "backend/app/api/routes/users.py",
            "backend/app/api/routes/items.py"))

    def test_index_peek_grep_pattern_vs_path(self):
        # N1: a Grep whose regex *pattern* contains ".repogrammar" is searching
        # source text and must NOT fire index_peek; a Grep whose *path* is the
        # index dir must.
        events = [
            {"kind": "tool_use", "id": "g1", "name": "Grep",
             "input": {"pattern": ".repogrammar", "path": "backend"}},
            {"kind": "tool_use", "id": "g2", "name": "Grep",
             "input": {"pattern": "engine", "path": ".repogrammar"}},
        ]
        r = detectors.detect_index_peek(events)
        self.assertEqual(r["count"], 1)
        self.assertEqual(r["hits"][0]["id"], "g2")


class GraderTest(unittest.TestCase):
    def _mini_task(self):
        return {
            "task_id": "mini", "task_type": "T1", "repo_id": "x", "prompt_template_id": "v1",
            "prompt": "x",
            "oracle": {"oracle_id": "mini", "kind": "patch_static_assert", "require_changed": True,
                       "assertions": [
                           {"type": "any_file_matches", "path_glob": "app/*.py",
                            "regex": "@router\\.get\\(\\s*\"[^\"]*summary\"", "desc": "summary route"},
                           {"type": "any_file_matches", "path_glob": "app/*.py",
                            "regex": "SessionDep", "desc": "SessionDep"}]}}

    def test_t1_gate_pass(self):
        d = tempfile.mkdtemp()
        try:
            wt = os.path.join(d, "wt")
            shutil.copytree(MINI, wt)
            with open(os.path.join(wt, "app", "routes.py"), "a") as fh:
                fh.write('\n@router.get("/{id}/summary")\ndef s(id, session: SessionDep): return {}\n')
            g = grader.grade(self._mini_task(), worktree_dir=wt, changed_files=["app/routes.py"])
            self.assertEqual(g["verdict"], "gate_pass")
            self.assertEqual(g["tests_failed"], 0)
        finally:
            shutil.rmtree(d)

    def test_t1_no_patch(self):
        g = grader.grade(self._mini_task(), worktree_dir=MINI, changed_files=[])
        self.assertEqual(g["verdict"], "no_patch")

    def test_t1_gate_fail(self):
        d = tempfile.mkdtemp()
        try:
            wt = os.path.join(d, "wt")
            shutil.copytree(MINI, wt)
            with open(os.path.join(wt, "app", "routes.py"), "a") as fh:
                fh.write("\n# unrelated comment, no summary route\n")
            g = grader.grade(self._mini_task(), worktree_dir=wt, changed_files=["app/routes.py"])
            self.assertEqual(g["verdict"], "gate_fail")
            self.assertGreater(g["tests_failed"], 0)
        finally:
            shutil.rmtree(d)

    def _added_task(self):
        # scope="added" convention checks (design-correct T1 shape after S1).
        a = lambda rx, d: {"type": "any_file_matches", "scope": "added",
                           "path_glob": "app/*.py", "regex": rx, "desc": d}
        return {
            "task_id": "t1a", "task_type": "T1", "repo_id": "x", "prompt_template_id": "v1", "prompt": "x",
            "oracle": {"oracle_id": "t1a", "kind": "patch_static_assert", "require_changed": True,
                       "assertions": [
                           a("@router\\.get\\(\\s*\"[^\"]*summary\"", "summary route"),
                           a("SessionDep", "SessionDep"),
                           a("CurrentUser", "CurrentUser"),
                           a("response_model\\s*=", "response_model")]}}

    def test_t1_added_scope_pass(self):
        added = ('@router.get("/{id}/summary", response_model=ItemSummary)\n'
                 'def item_summary(session: SessionDep, current_user: CurrentUser, id): ...')
        g = grader.grade(self._added_task(), worktree_dir=MINI,
                         changed_files=["app/routes.py"], added_text=added)
        self.assertEqual(g["verdict"], "gate_pass")

    def test_t1_added_scope_auth_missing(self):
        # S1 skeptic scenario: a summary route missing CurrentUser must gate_fail
        # even though the pinned items.py contains CurrentUser (scope=added
        # ignores pre-existing code).
        added = ('@router.get("/{id}/summary", response_model=ItemSummary)\n'
                 'def item_summary(session: SessionDep, id): ...')  # no CurrentUser
        g = grader.grade(self._added_task(), worktree_dir=MINI,
                         changed_files=["app/routes.py"], added_text=added)
        self.assertEqual(g["verdict"], "gate_fail")
        self.assertTrue(any(d["desc"] == "CurrentUser" and not d["ok"] for d in g["detail"]))

    def test_t1_added_scope_ignores_preexisting_worktree(self):
        # added_text lacks the session/auth deps entirely -> gate_fail, proving
        # the worktree's pre-existing occurrences do not satisfy scope=added.
        added = '@router.get("/{id}/summary", response_model=X)\ndef s(id): return {}'
        g = grader.grade(self._added_task(), worktree_dir=MINI,
                         changed_files=["app/routes.py"], added_text=added)
        self.assertEqual(g["verdict"], "gate_fail")

    def test_t6_checklist(self):
        task = {
            "task_id": "t6", "task_type": "T6", "repo_id": "x", "prompt_template_id": "v1", "prompt": "x",
            "oracle": {"oracle_id": "t6", "kind": "answer_checklist",
                       "must_have": [
                           {"fact": "dep injection", "any_regex": ["dependency injection", "Depends\\("]},
                           {"fact": "session id", "any_regex": ["SessionDep", "get_db"]}],
                       "forbidden": [{"fact": "no middleware claim",
                                      "any_regex": ["middleware (provides|supplies) the session"]}]}}
        good = "Sessions use FastAPI dependency injection via SessionDep."
        self.assertEqual(grader.grade(task, answer_text=good)["verdict"], "gate_pass")
        bad = "The middleware provides the session automatically."
        self.assertEqual(grader.grade(task, answer_text=bad)["verdict"], "gate_fail")
        self.assertEqual(grader.grade(task, answer_text="")["verdict"], "gate_fail")


class RecordSchemaTest(unittest.TestCase):
    def _valid(self):
        r = record_schema.new_record()
        r.update({"run_id": "abc", "task_id": "t1", "task_type": "T1", "repo_id": "r",
                  "repo_sha": "deadbeef", "arm": "A3", "outcome": "gate_pass",
                  "transcript_path": "transcript.jsonl"})
        return r

    def test_valid_record(self):
        self.assertEqual(record_schema.validate_record(self._valid()), [])

    def test_bad_outcome(self):
        r = self._valid()
        r["outcome"] = "not_a_thing"
        self.assertTrue(any("outcome" in e for e in record_schema.validate_record(r)))

    def test_privacy_absolute_path(self):
        r = self._valid()
        r["notes"] = "read /Users/sioyoo/secret/file.py"
        errs = record_schema.validate_record(r)
        self.assertTrue(any("absolute" in e for e in errs))

    def test_transcript_must_be_basename(self):
        r = self._valid()
        r["transcript_path"] = "/private/tmp/run/transcript.jsonl"
        self.assertTrue(record_schema.validate_record(r))

    def test_append_writes_valid_jsonl(self):
        d = tempfile.mkdtemp()
        try:
            ledger = os.path.join(d, "l.jsonl")
            record_schema.append_record(ledger, self._valid())
            record_schema.append_record(ledger, self._valid())
            with open(ledger) as fh:
                lines = fh.read().splitlines()
            self.assertEqual(len(lines), 2)
            import json
            self.assertEqual(json.loads(lines[0])["schema_version"], record_schema.SCHEMA_VERSION)
        finally:
            shutil.rmtree(d)


if __name__ == "__main__":
    unittest.main(verbosity=2)
