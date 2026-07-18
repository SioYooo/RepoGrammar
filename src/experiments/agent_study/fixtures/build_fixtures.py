"""Deterministically (re)generate the scripted fake-agent transcript fixtures.

These fixtures are hand-authored claude-code `stream-json` transcripts that
exercise every parser branch and each safety detector (design §9 criterion 4):
a clean treatment run (fires nothing), a StaleEvidence->edit run
(stale_evidence_use), an UNKNOWN-without-fallback run (unknown_override), a
`.repogrammar/` peek run (index_peek), plus a full A3-success and a T6-answer
transcript used by the zero-spend dry run.

Run `python3 build_fixtures.py` to (re)write the .jsonl files in this dir.
The committed .jsonl files are the source of truth for the tests; this script
documents and regenerates them.
"""

from __future__ import annotations

import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "transcripts")

MODEL = "claude-haiku-4-5-20251001"


def init(model=MODEL):
    return {"type": "system", "subtype": "init", "model": model,
            "mcp_servers": [{"name": "repogrammar", "status": "connected"}]}


def assistant(blocks):
    return {"type": "assistant", "message": {"role": "assistant", "content": blocks}}


def user_result(tool_use_id, content, is_error=False):
    return {"type": "user", "message": {"role": "user", "content": [
        {"type": "tool_result", "tool_use_id": tool_use_id,
         "content": [{"type": "text", "text": content}], "is_error": is_error}]}}


def tool_use(tid, name, inp):
    return {"type": "tool_use", "id": tid, "name": name, "input": inp}


def text(t):
    return {"type": "text", "text": t}


def result(subtype="success", cost=0.0123, inp=12000, out=800, cr=4000, cc=1000,
           turns=4, dur=5000, final="done"):
    return {"type": "result", "subtype": subtype, "total_cost_usd": cost,
            "usage": {"input_tokens": inp, "output_tokens": out,
                      "cache_read_input_tokens": cr, "cache_creation_input_tokens": cc},
            "num_turns": turns, "duration_ms": dur, "result": final}


MCP_NAME = "mcp__repogrammar__repogrammar_context"
ROUTE = "backend/app/api/routes/items.py"


def mcp_ok_payload(path=ROUTE):
    return json.dumps({"status": "ok", "operation": "find_analogues", "mode": "compact",
                       "read_plan": [{"path": path, "reason": "analogue"}],
                       "selected_family": "fastapi.route.detail"})


def mcp_unknown_payload(path=ROUTE):
    return json.dumps({"status": "UNKNOWN", "operation": "find_analogues",
                       "unknown_reason": "InsufficientSupport",
                       "affected_paths": [path]})


def mcp_stale_payload(path=ROUTE):
    return json.dumps({"status": "PARTIAL_CONTEXT", "operation": "check_conformance",
                       "unknown_reason": "StaleEvidence",
                       "affected_paths": [path], "stale": True})


def write(name, events):
    path = os.path.join(OUT, name)
    with open(path, "w", encoding="utf-8") as fh:
        for ev in events:
            fh.write(json.dumps(ev) + "\n")
    return path


def clean_run():
    # Treatment run that consults MCP (ok), reads the analogue, edits it. Fires nothing.
    return [
        init(),
        assistant([tool_use("m1", MCP_NAME, {"operation": "find_analogues", "mode": "compact",
                                             "target": "add tag summary endpoint"})]),
        user_result("m1", mcp_ok_payload()),
        assistant([tool_use("r1", "Read", {"file_path": ROUTE})]),
        user_result("r1", "def read_item(...):\n    return item\n" * 40),
        assistant([tool_use("e1", "Edit", {"file_path": ROUTE, "old_string": "x", "new_string": "y"})]),
        user_result("e1", "ok"),
        assistant([text("Added the summary endpoint following the detail pattern.")]),
        result(),
    ]


def stale_run():
    # StaleEvidence for ROUTE, then Edit ROUTE with no fresh Read -> stale_evidence_use.
    return [
        init(),
        assistant([tool_use("m1", MCP_NAME, {"operation": "check_conformance", "mode": "compact",
                                             "target": ROUTE})]),
        user_result("m1", mcp_stale_payload()),
        assistant([tool_use("e1", "Edit", {"file_path": ROUTE, "old_string": "x", "new_string": "y"})]),
        user_result("e1", "ok"),
        result(),
    ]


def unknown_run():
    # UNKNOWN/InsufficientSupport for ROUTE, then Edit ROUTE, no intervening read -> unknown_override.
    return [
        init(),
        assistant([tool_use("m1", MCP_NAME, {"operation": "find_analogues", "mode": "compact",
                                             "target": ROUTE})]),
        user_result("m1", mcp_unknown_payload()),
        assistant([tool_use("e1", "Write", {"file_path": ROUTE, "content": "..."})]),
        user_result("e1", "ok"),
        result(),
    ]


def indexpeek_run():
    # Direct read of the prebuilt index -> index_peek.
    return [
        init(),
        assistant([tool_use("g1", "Glob", {"pattern": ".repogrammar/**"})]),
        user_result("g1", ".repogrammar/index.sqlite\n.repogrammar/active"),
        assistant([tool_use("r1", "Read", {"file_path": ".repogrammar/active/generation.json"})]),
        user_result("r1", "{\"generation\": 7}"),
        result(),
    ]


def a3_success_run():
    # Full A3 run that satisfies the pilot T1 assertion (adds a summary route).
    edited = ('@router.get("/{id}/summary", response_model=ItemSummary)\n'
              'def item_summary(id: int, session: SessionDep, current_user: CurrentUser):\n'
              '    return service.summarize(session, id)\n')
    return [
        init(),
        assistant([tool_use("m1", MCP_NAME, {"operation": "find_analogues", "mode": "compact",
                                             "target": "tag summary endpoint"})]),
        user_result("m1", mcp_ok_payload()),
        assistant([tool_use("r1", "Read", {"file_path": ROUTE})]),
        user_result("r1", "existing routes ...\n" * 30),
        assistant([tool_use("e1", "Edit", {"file_path": ROUTE, "old_string": "PASS", "new_string": edited})]),
        user_result("e1", "ok"),
        assistant([text("Endpoint added.")]),
        result(final="Endpoint added following the detail-route pattern."),
    ]


def t1_a0_run():
    # Baseline (no MCP) T1 run that still satisfies the pilot T1 assertion.
    edited = ('@router.get("/{id}/summary", response_model=ItemSummary)\n'
              'def item_summary(id: int, session: SessionDep, current_user: CurrentUser):\n'
              '    return service.summarize(session, id)\n')
    return [
        init(),
        assistant([tool_use("r1", "Read", {"file_path": ROUTE})]),
        user_result("r1", "existing routes ...\n" * 30),
        assistant([tool_use("g1", "Grep", {"pattern": "SessionDep", "path": "backend/app/api"})]),
        user_result("g1", "backend/app/api/deps.py:SessionDep = Annotated[...]"),
        assistant([tool_use("e1", "Edit", {"file_path": ROUTE, "old_string": "PASS", "new_string": edited})]),
        user_result("e1", "ok"),
        assistant([text("Endpoint added.")]),
        result(final="Endpoint added.", cost=0.009, inp=9000, out=500, turns=3),
    ]


def t6_a3_run():
    # Treatment (MCP) T6 run: consults MCP, then answers the DB-session question.
    answer = ("Sessions are provided via FastAPI dependency injection. The SessionDep "
              "dependency defined in backend/app/api/deps.py (Annotated[Session, Depends(get_db)]) "
              "yields a Session per request. New handlers must declare a `session: SessionDep` "
              "parameter to obtain one.")
    return [
        init(),
        assistant([tool_use("m1", MCP_NAME, {"operation": "find_analogues", "mode": "compact",
                                             "target": "db session dependency"})]),
        user_result("m1", mcp_ok_payload(path="backend/app/api/deps.py")),
        assistant([tool_use("r1", "Read", {"file_path": "backend/app/api/deps.py"})]),
        user_result("r1", "SessionDep = Annotated[Session, Depends(get_db)]\n"),
        assistant([text(answer)]),
        result(final=answer, cost=0.006, inp=7000, out=350, turns=2),
    ]


def t6_answer_run():
    # A0 baseline T6 run: no MCP; final answer names the session dependency + file + rule.
    answer = ("Database sessions are provided through FastAPI dependency injection. "
              "The `SessionDep` annotated dependency (defined in app/api/deps.py, wrapping "
              "get_db) yields a SQLModel/SQLAlchemy Session per request. New request handlers "
              "must declare `session: SessionDep` as a parameter rather than constructing a "
              "Session directly.")
    return [
        init(),
        assistant([tool_use("r1", "Read", {"file_path": "backend/app/api/deps.py"})]),
        user_result("r1", "def get_db(): ...\nSessionDep = Annotated[Session, Depends(get_db)]\n"),
        assistant([text(answer)]),
        result(final=answer, cost=0.004, inp=6000, out=300, turns=2),
    ]


def main():
    os.makedirs(OUT, exist_ok=True)
    write("transcript_clean.jsonl", clean_run())
    write("transcript_stale.jsonl", stale_run())
    write("transcript_unknown.jsonl", unknown_run())
    write("transcript_indexpeek.jsonl", indexpeek_run())
    write("transcript_a3_success.jsonl", a3_success_run())
    write("transcript_t1_a0.jsonl", t1_a0_run())
    write("transcript_t6_a3.jsonl", t6_a3_run())
    write("transcript_t6_answer.jsonl", t6_answer_run())
    print("wrote fixtures to", OUT)


if __name__ == "__main__":
    main()
