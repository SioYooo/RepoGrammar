"""`agent-study-run.v1` record schema, builder, privacy guard, and JSONL writer.

The committed per-run record contains only hashes, counts, repo-relative paths,
and pinned public SHAs. It MUST NOT contain prompt text, source text, patch
text, tool-result text, or absolute private paths (design §6 / §6 record
schema). `validate_record` enforces the shape and the privacy invariants;
`append_record` writes one JSON object per line to an append-only ledger.
"""

from __future__ import annotations

import json
import os
import re
from typing import Any, Dict, List

SCHEMA_VERSION = "agent-study-run.v1"

# Outcome vocabulary from design §6 record schema.
OUTCOMES = {
    "gate_pass",
    "gate_fail",
    "patch_no_apply",
    "no_patch",
    "timeout",
    "infra_failure",
    "budget_exceeded",  # pilot addition: CLI --max-budget-usd cap hit (recorded, not a bug)
}

# Absolute-path / private-path markers that must never appear in a committed record.
_ABS_PATH_RE = re.compile(r"(^|[\"' :=])(/Users/|/home/|/private/tmp/|/tmp/|/var/folders/|[A-Za-z]:\\\\)")


def new_record() -> Dict[str, Any]:
    """Return a fully-populated record skeleton with null/zero defaults."""
    return {
        "schema_version": SCHEMA_VERSION,
        "run_id": None,
        "rep_index": 0,
        "task_id": None,
        "task_type": None,
        "repo_id": None,
        "repo_sha": None,
        "worktree_sha256": None,
        "seeded_delta_sha256": None,
        "arm": None,
        "mcp_config_sha256": None,
        "agent": {"cli": "claude-code", "cli_version": None, "model": None},
        "prompt_template_id": None,
        "prompt_sha256": None,
        "started_at": None,
        "finished_at": None,
        "duration_ms": 0,
        "outcome": None,
        "oracle": {"oracle_id": None, "commands_sha256": None, "tests_passed": 0, "tests_failed": 0},
        "diff": {"sha256": None, "files_touched": 0, "insertions": 0, "deletions": 0},
        "tokens": {
            "source": "host_reported",
            "input": None,
            "output": None,
            "cache_read": None,
            "cache_creation": None,
            "total_cost_usd": None,
        },
        "context": {
            "files_read": 0,
            "bytes_read": 0,
            "search_calls": 0,
            "read_tool_calls": 0,
            "bash_read_calls": 0,
            "tool_result_tokens_estimated": 0,
        },
        "mcp": {
            "calls": 0,
            "by_operation": {},
            "unknown_count": 0,
            "stale_count": 0,
            "partial_context_count": 0,
            "spans_requested": 0,
            "spans_rendered": 0,
            "first_call_before_first_read": False,
            # Additive columns (added 2026-07-18 after adversarial review S5):
            # design §10 requires MCP result bytes as their own column (kept out
            # of context.bytes_read); design §6 defines read-plan item count and
            # the selected family id. The 8 as-run pilot records predate these
            # columns; a v1 reader treats missing keys as null (additive rule).
            "result_bytes": 0,
            "read_plan_item_count": 0,
            "selected_family_ids": [],
        },
        "safety": {"unknown_override": 0, "stale_evidence_use": 0, "index_peek": 0},
        "compaction_count": 0,
        "num_turns": 0,
        "transcript_path": None,  # local untracked path, basename only in committed records
        "notes": None,
    }


def _walk_strings(value: Any) -> List[str]:
    out: List[str] = []
    if isinstance(value, str):
        out.append(value)
    elif isinstance(value, dict):
        for k, v in value.items():
            out.append(str(k))
            out.extend(_walk_strings(v))
    elif isinstance(value, list):
        for v in value:
            out.extend(_walk_strings(v))
    return out


def validate_record(record: Dict[str, Any]) -> List[str]:
    """Return a list of validation errors; empty list means valid.

    Enforces required keys, the outcome vocabulary, and the privacy invariant
    that no string field embeds an absolute private path. `transcript_path` is
    exempted only if it is a bare basename (no path separator).
    """
    errors: List[str] = []
    if record.get("schema_version") != SCHEMA_VERSION:
        errors.append(f"schema_version must be {SCHEMA_VERSION!r}")
    for key in ("run_id", "task_id", "task_type", "repo_id", "repo_sha", "arm", "outcome"):
        if not record.get(key):
            errors.append(f"missing required field: {key}")
    if record.get("outcome") not in OUTCOMES:
        errors.append(f"outcome {record.get('outcome')!r} not in {sorted(OUTCOMES)}")

    # Privacy: no absolute private paths anywhere. transcript_path must be a
    # basename in committed records (the full local path lives only in memory).
    tp = record.get("transcript_path")
    if tp is not None and ("/" in tp or "\\" in tp):
        errors.append("transcript_path must be a bare basename in committed records")
    for s in _walk_strings(record):
        if _ABS_PATH_RE.search(s):
            errors.append(f"absolute/private path leaked into record: {s!r}")
            break
    return errors


def append_record(ledger_path: str, record: Dict[str, Any]) -> None:
    """Validate then append one record as a single JSONL line (append-only)."""
    errors = validate_record(record)
    if errors:
        raise ValueError("record failed validation: " + "; ".join(errors))
    os.makedirs(os.path.dirname(os.path.abspath(ledger_path)), exist_ok=True)
    with open(ledger_path, "a", encoding="utf-8") as fh:
        fh.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")
