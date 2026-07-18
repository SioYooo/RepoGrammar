"""Mechanical transcript parsing for claude-code `--output-format stream-json`.

All RQ5 metrics derive mechanically from the agent CLI's structured transcript
(design §6). This module reads the JSONL transcript, normalizes it into an
ordered sequence of tool events, and computes the context/token/MCP fields.
It persists nothing: the caller feeds the parsed metrics into a record and the
transcript stays in a local untracked archive.

claude-code stream-json shapes handled (verified against CLI 2.1.x):
  - {"type":"system","subtype":"init","model":..,"tools":[..],"mcp_servers":[..]}
  - {"type":"assistant","message":{"content":[{"type":"text"|"tool_use",..}]}}
  - {"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":..,
        "content": <str | [ {"type":"text","text":..}, .. ]>, "is_error": bool}]}}
  - {"type":"result","subtype":..,"total_cost_usd":..,"usage":{..},
        "num_turns":..,"duration_ms":..}

MCP tool calls are `tool_use` blocks named
`mcp__repogrammar__repogrammar_context`; their paired `tool_result` carries the
repogrammar context payload (status / read plan / unknown reason).
"""

from __future__ import annotations

import json
import math
import re
from typing import Any, Dict, List, Optional

MCP_TOOL_NAME = "mcp__repogrammar__repogrammar_context"
READ_TOOLS = {"Read"}
SEARCH_TOOLS = {"Grep", "Glob"}
EDIT_TOOLS = {"Edit", "Write", "MultiEdit", "NotebookEdit"}
BASH_TOOLS = {"Bash"}
# design §6: read-pattern bash commands counted as reads.
BASH_READ_RE = re.compile(r"\b(cat|head|tail|less|sed -n)\b")
INDEX_PEEK_RE = re.compile(r"\.repogrammar(/|\b)")


def iter_events(source: Any) -> List[Dict[str, Any]]:
    """Parse a transcript into a list of JSON event objects.

    `source` may be a file path (str ending in a newline-free path that exists),
    an iterable of lines, or a single multi-line string. Non-JSON lines are
    skipped (some CLIs emit stray log lines on stderr-merged streams).
    """
    if isinstance(source, str):
        # Treat as raw text if it contains a newline or is not an existing file.
        import os

        if "\n" not in source and os.path.exists(source):
            with open(source, "r", encoding="utf-8") as fh:
                lines = fh.readlines()
        else:
            lines = source.splitlines()
    else:
        lines = list(source)
    events: List[Dict[str, Any]] = []
    for line in lines:
        line = line.strip()
        if not line:
            continue
        try:
            events.append(json.loads(line))
        except (ValueError, TypeError):
            continue
    return events


def _content_blocks(message: Dict[str, Any]) -> List[Dict[str, Any]]:
    content = message.get("content")
    if isinstance(content, list):
        return [b for b in content if isinstance(b, dict)]
    return []


def _tool_result_text(block: Dict[str, Any]) -> str:
    content = block.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts = []
        for b in content:
            if isinstance(b, dict) and isinstance(b.get("text"), str):
                parts.append(b["text"])
            elif isinstance(b, str):
                parts.append(b)
        return "".join(parts)
    return ""


def extract_tool_events(events: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
    """Return the ordered stream of tool_use / tool_result events.

    Each element is either:
      {"kind":"tool_use","id":str,"name":str,"input":dict}
      {"kind":"tool_result","tool_use_id":str,"text":str,"is_error":bool}
    Order is preserved so the syntactic safety detectors can reason over it.
    """
    out: List[Dict[str, Any]] = []
    for ev in events:
        etype = ev.get("type")
        if etype not in ("assistant", "user"):
            continue
        message = ev.get("message")
        if not isinstance(message, dict):
            continue
        for block in _content_blocks(message):
            btype = block.get("type")
            if btype == "tool_use":
                out.append(
                    {
                        "kind": "tool_use",
                        "id": block.get("id"),
                        "name": block.get("name"),
                        "input": block.get("input") if isinstance(block.get("input"), dict) else {},
                    }
                )
            elif btype == "tool_result":
                out.append(
                    {
                        "kind": "tool_result",
                        "tool_use_id": block.get("tool_use_id"),
                        "text": _tool_result_text(block),
                        "is_error": bool(block.get("is_error", False)),
                    }
                )
    return out


def _parse_mcp_result(text: str) -> Dict[str, Any]:
    """Best-effort parse of a repogrammar_context MCP result payload.

    Returns a normalized dict: {status, unknown, stale, partial, affected_paths,
    spans_rendered}. Tolerant of shape drift: recognizes documented tokens
    (design §6: ok | PARTIAL_CONTEXT | UNKNOWN | StaleEvidence |
    InsufficientSupport) whether they arrive as a structured JSON field or as
    substrings of a text payload.
    """
    norm = {
        "status": None,
        "unknown": False,
        "stale": False,
        "partial": False,
        "affected_paths": [],
        "spans_rendered": 0,
        "read_plan_count": 0,
        "selected_family": None,
    }
    payload: Optional[Any] = None
    try:
        payload = json.loads(text)
    except (ValueError, TypeError):
        payload = None

    def collect_paths(obj: Any) -> List[str]:
        paths: List[str] = []
        if isinstance(obj, dict):
            for key in ("affected_paths", "read_plan_paths"):
                v = obj.get(key)
                if isinstance(v, list):
                    paths.extend([p for p in v if isinstance(p, str)])
            rp = obj.get("read_plan")
            if isinstance(rp, list):
                for item in rp:
                    if isinstance(item, dict) and isinstance(item.get("path"), str):
                        paths.append(item["path"])
            tgt = obj.get("target")
            if isinstance(tgt, dict) and isinstance(tgt.get("path"), str):
                paths.append(tgt["path"])
            elif isinstance(tgt, str) and "/" in tgt:
                paths.append(tgt)
        return paths

    if isinstance(payload, dict):
        status = payload.get("status")
        norm["status"] = status if isinstance(status, str) else None
        reason = str(payload.get("unknown_reason") or "")
        blob = json.dumps(payload)
        norm["unknown"] = (
            str(status).upper() == "UNKNOWN"
            or "InsufficientSupport" in blob
            or "InsufficientSupport" in reason
        )
        norm["stale"] = "StaleEvidence" in blob or bool(payload.get("stale"))
        norm["partial"] = str(status).upper() == "PARTIAL_CONTEXT"
        norm["affected_paths"] = sorted(set(collect_paths(payload)))
        spans = payload.get("spans_rendered")
        if isinstance(spans, int):
            norm["spans_rendered"] = spans
        rp = payload.get("read_plan")
        if isinstance(rp, list):
            norm["read_plan_count"] = len(rp)
        fam = payload.get("selected_family") or payload.get("selected_family_id")
        if isinstance(fam, str):
            norm["selected_family"] = fam
    else:
        upper = text.upper()
        norm["unknown"] = "UNKNOWN" in upper or "INSUFFICIENTSUPPORT" in upper
        norm["stale"] = "STALEEVIDENCE" in upper
        norm["partial"] = "PARTIAL_CONTEXT" in upper
        if "UNKNOWN" in upper:
            norm["status"] = "UNKNOWN"
        elif "PARTIAL_CONTEXT" in upper:
            norm["status"] = "PARTIAL_CONTEXT"
    return norm


def parse_transcript(source: Any) -> Dict[str, Any]:
    """Parse a full transcript into the metric fields RQ5 records.

    Returns a dict with: model, result (final event fields), context counts,
    mcp usage, and the ordered `tool_events` + normalized `mcp_results` used by
    the safety detectors.
    """
    events = iter_events(source)
    tool_events = extract_tool_events(events)

    model = None
    result_ev: Optional[Dict[str, Any]] = None
    compaction_count = 0
    last_assistant_text = ""
    for ev in events:
        if ev.get("type") == "system" and ev.get("subtype") == "init":
            model = ev.get("model") or model
        if ev.get("type") == "result":
            result_ev = ev
        if ev.get("type") == "assistant" and isinstance(ev.get("message"), dict):
            texts = [b.get("text", "") for b in _content_blocks(ev["message"]) if b.get("type") == "text"]
            joined = "".join(t for t in texts if isinstance(t, str))
            if joined.strip():
                last_assistant_text = joined
        # compaction markers can surface as a system subtype or a boolean flag.
        if ev.get("type") == "system" and "compact" in str(ev.get("subtype", "")).lower():
            compaction_count += 1

    # The agent's final answer (for T6 answer_checklist grading): prefer the
    # result event's `result` string; fall back to the last assistant text.
    final_text = ""
    if result_ev and isinstance(result_ev.get("result"), str):
        final_text = result_ev["result"]
    if not final_text.strip():
        final_text = last_assistant_text

    # --- context metrics ---
    read_files = set()
    read_tool_calls = 0
    bash_read_calls = 0
    search_calls = 0
    bytes_read = 0
    tool_result_tokens_estimated = 0

    # index by tool_use_id so we can attribute tool_result bytes to a read.
    tool_use_by_id: Dict[str, Dict[str, Any]] = {}
    for te in tool_events:
        if te["kind"] == "tool_use" and te.get("id"):
            tool_use_by_id[te["id"]] = te

    mcp_results: List[Dict[str, Any]] = []  # normalized, in call order
    mcp_calls = 0
    mcp_by_operation: Dict[str, int] = {}
    mcp_unknown = 0
    mcp_stale = 0
    mcp_partial = 0
    spans_requested = 0
    spans_rendered = 0
    mcp_result_bytes = 0
    read_plan_item_count = 0
    selected_family_ids: List[str] = []

    first_mcp_index = None
    first_read_index = None

    for idx, te in enumerate(tool_events):
        if te["kind"] == "tool_use":
            name = te.get("name") or ""
            inp = te.get("input") or {}
            if name in READ_TOOLS:
                read_tool_calls += 1
                fp = inp.get("file_path")
                if isinstance(fp, str):
                    read_files.add(fp)
                if first_read_index is None:
                    first_read_index = idx
            elif name in SEARCH_TOOLS:
                search_calls += 1
                if first_read_index is None:
                    first_read_index = idx
            elif name in BASH_TOOLS:
                cmd = inp.get("command", "")
                if isinstance(cmd, str) and BASH_READ_RE.search(cmd):
                    bash_read_calls += 1
                    for tok in re.findall(r"[\w./-]+", cmd):
                        if "/" in tok or tok.endswith((".py", ".ts", ".js", ".rs", ".md", ".json")):
                            read_files.add(tok)
                    if first_read_index is None:
                        first_read_index = idx
            elif name == MCP_TOOL_NAME:
                mcp_calls += 1
                op = inp.get("operation") or "unknown"
                mcp_by_operation[op] = mcp_by_operation.get(op, 0) + 1
                if inp.get("include_source_spans"):
                    spans_requested += 1
                if first_mcp_index is None:
                    first_mcp_index = idx
        elif te["kind"] == "tool_result":
            src = tool_use_by_id.get(te.get("tool_use_id") or "")
            src_name = (src or {}).get("name") or ""
            text = te.get("text") or ""
            # design §6: read bytes = Read results + *read-pattern* Bash results
            # only. Gate the Bash branch on the paired command matching the same
            # BASH_READ_RE used for bash_read_calls (a pytest/build Bash result is
            # not a "read" and must not inflate bytes_read).
            counts_as_read = False
            if src_name in READ_TOOLS:
                counts_as_read = True
            elif src_name in BASH_TOOLS:
                cmd = (src.get("input") or {}).get("command", "") if src else ""
                counts_as_read = isinstance(cmd, str) and bool(BASH_READ_RE.search(cmd))
            if counts_as_read:
                nbytes = len(text.encode("utf-8"))
                bytes_read += nbytes
                tool_result_tokens_estimated += math.ceil(nbytes / 4)
            if src_name == MCP_TOOL_NAME:
                norm = _parse_mcp_result(text)
                norm["_index"] = idx
                mcp_results.append(norm)
                if norm["unknown"]:
                    mcp_unknown += 1
                if norm["stale"]:
                    mcp_stale += 1
                if norm["partial"]:
                    mcp_partial += 1
                spans_rendered += norm.get("spans_rendered", 0)
                # design §10 / §6 additive metrics: MCP result bytes (kept OUT of
                # bytes_read by construction), read-plan size, selected family.
                mcp_result_bytes += len(text.encode("utf-8"))
                read_plan_item_count += norm.get("read_plan_count", 0)
                fam = norm.get("selected_family")
                if fam and fam not in selected_family_ids:
                    selected_family_ids.append(fam)

    first_call_before_first_read = False
    if first_mcp_index is not None:
        first_call_before_first_read = (
            first_read_index is None or first_mcp_index < first_read_index
        )

    # --- final result event ---
    tokens = {
        "source": "host_reported",
        "input": None,
        "output": None,
        "cache_read": None,
        "cache_creation": None,
        "total_cost_usd": None,
    }
    num_turns = 0
    duration_ms = 0
    result_subtype = None
    result_is_error = False
    if result_ev:
        usage = result_ev.get("usage") or {}
        tokens["input"] = usage.get("input_tokens")
        tokens["output"] = usage.get("output_tokens")
        tokens["cache_read"] = usage.get("cache_read_input_tokens")
        tokens["cache_creation"] = usage.get("cache_creation_input_tokens")
        tokens["total_cost_usd"] = result_ev.get("total_cost_usd")
        num_turns = result_ev.get("num_turns") or 0
        duration_ms = result_ev.get("duration_ms") or 0
        result_subtype = result_ev.get("subtype")
        result_is_error = bool(result_ev.get("is_error", False))

    return {
        "model": model,
        "result_present": result_ev is not None,
        "result_subtype": result_subtype,
        "result_is_error": result_is_error,
        "final_text": final_text,
        "tokens": tokens,
        "num_turns": num_turns,
        "duration_ms": duration_ms,
        "compaction_count": compaction_count,
        "context": {
            "files_read": len(read_files),
            "bytes_read": bytes_read,
            "search_calls": search_calls,
            "read_tool_calls": read_tool_calls,
            "bash_read_calls": bash_read_calls,
            "tool_result_tokens_estimated": tool_result_tokens_estimated,
        },
        "mcp": {
            "calls": mcp_calls,
            "by_operation": mcp_by_operation,
            "unknown_count": mcp_unknown,
            "stale_count": mcp_stale,
            "partial_context_count": mcp_partial,
            "spans_requested": spans_requested,
            "spans_rendered": spans_rendered,
            "first_call_before_first_read": first_call_before_first_read,
            "result_bytes": mcp_result_bytes,
            "read_plan_item_count": read_plan_item_count,
            "selected_family_ids": selected_family_ids,
        },
        "tool_events": tool_events,
        "mcp_results": mcp_results,
    }
