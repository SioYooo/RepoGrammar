"""Offline current-contract measurements. Never launches an agent or changes RQ5."""
import hashlib
import json
import math
from pathlib import PurePosixPath

from parsers import MCP_TOOL_NAME, extract_tool_events


def _path(value, root):
    if not isinstance(value, str):
        return None
    if value.startswith(root.rstrip("/") + "/"):
        value = value[len(root.rstrip("/")) + 1:]
    path = PurePosixPath(value)
    if path.is_absolute() or ".." in path.parts or "\\" in value or ":" in value:
        return None
    return str(path) if path.parts else None


def _unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def _object(owner, key):
    value = owner.get(key)
    if value is None:
        return {}
    if not isinstance(value, dict):
        raise ValueError("malformed MCP object")
    return value


def current_mcp(text, root):
    """Extract only schema fields, never search arbitrary source/text for reasons."""
    if len(text.encode("utf-8")) > 2 * 1024 * 1024:
        raise ValueError("oversized MCP result")
    payload = json.loads(text, object_pairs_hook=_unique_object)
    if not isinstance(payload, dict) or payload.get("schema_version") != "product-schemas.v1":
        raise ValueError("unsupported MCP schema")
    plan = _object(payload, "read_plan")
    if not isinstance(plan, dict) or not isinstance(plan.get("items", []), list):
        raise ValueError("malformed read plan")
    paths = set()
    for item in plan.get("items", []):
        if not isinstance(item, dict) or not _path(item.get("path"), root):
            raise ValueError("malformed read-plan item")
        paths.add(_path(item["path"], root))
    resolved = _object(payload, "resolved_target")
    if isinstance(resolved, dict):
        path = _path(resolved.get("path"), root)
        if path:
            paths.add(path)
    unknowns = payload.get("unknowns", [])
    if not isinstance(unknowns, list) or any(not isinstance(u, dict) or
            not isinstance(u.get("reason"), str) for u in unknowns):
        raise ValueError("malformed unknown records")
    reasons = {u["reason"] for u in unknowns}
    stale_paths = set(paths) if "StaleEvidence" in reasons else set()
    for owner, key in ((_object(payload, "source_spans"), "omissions"),
                       (plan, "line_range_omissions")):
        if not isinstance(owner, dict) or not isinstance(owner.get(key, []), list):
            raise ValueError("malformed source omission")
        for omission in owner.get(key, []):
            if not isinstance(omission, dict):
                raise ValueError("malformed source omission")
            if omission.get("reason") == "stale_evidence":
                path = _path(omission.get("path"), root)
                if not path:
                    raise ValueError("malformed stale path")
                stale_paths.add(path)
    family = _object(payload, "family")
    route = _object(payload, "query_route")
    identities = {x for x in (family.get("family_id"), route.get("selected_family_id"),
                              payload.get("selected_family_id")) if isinstance(x, str)}
    if len(identities) > 1:
        raise ValueError("conflicting selected-family identities")
    return {
        "paths": paths, "items": len(plan.get("items", [])),
        "unknown": payload.get("status") == "UNKNOWN" or "InsufficientSupport" in reasons,
        "stale": bool(stale_paths) or "StaleEvidence" in reasons, "stale_paths": stale_paths,
        "family": next(iter(identities), None),
    }


def measure(events, root):
    """Claude stream-json adapter; other CLI shapes need their own qualified adapter.

    Reads count successful Read acquisitions and their rendered result bytes.
    Grep/Glob are searches; every Bash is unclassified, never guessed as a read.
    Safety counts unread edits after scoped UNKNOWN/stale, not semantic harm.
    """
    tools = extract_tool_events(events)
    turns, turn_by_id = 0, {}
    for event in events:
        if event.get("type") == "assistant":
            turns += 1
            for block in (event.get("message") or {}).get("content", []):
                if isinstance(block, dict) and block.get("type") == "tool_use":
                    turn_by_id[block.get("id")] = turns
    result = next((e for e in reversed(events) if e.get("type") == "result"), {})
    usage = result.get("usage") or {}
    counts = dict.fromkeys(("mcp_calls", "mcp_result_bytes", "read_plan_items", "read_calls",
                           "read_result_bytes", "search_calls", "search_result_bytes",
                           "failed_read_calls", "failed_read_result_bytes",
                           "unclassified_shell_calls", "missing_tool_results", "mcp_parse_errors",
                           "unscoped_abstentions", "unknown_unread_edits", "stale_unread_edits",
                           "index_access_attempts", "shell_index_keyword_flags",
                           "broad_before", "broad_after"), 0)
    pending, seen, files, families, armed = {}, set(), set(), set(), []
    first_turn, first_event = None, None
    for index, tool in enumerate(tools):
        if tool["kind"] == "tool_use":
            tid, name, inp = tool.get("id"), tool.get("name"), tool.get("input") or {}
            if not tid or tid in seen:
                raise ValueError("missing or duplicate tool id")
            seen.add(tid)
            tool["use_index"] = index
            pending[tid] = tool
            keys = ("path", "glob") if name == "Grep" else ("pattern", "path") if name == "Glob" else ("file_path",)
            if name in ("Read", "Grep", "Glob") and any(
                    ".repogrammar" in PurePosixPath(str(inp.get(k, ""))).parts for k in keys):
                counts["index_access_attempts"] += 1
            if name == MCP_TOOL_NAME:
                counts["mcp_calls"] += 1
                if first_event is None:
                    first_event, first_turn = index, turn_by_id.get(tid)
            if name in ("Read", "Grep", "Glob"):
                counts["read_calls" if name == "Read" else "search_calls"] += 1
                # Broad = search, or Read without a bounded offset AND limit.
                bounded = all(isinstance(inp.get(k), int) and not isinstance(inp[k], bool)
                              and inp[k] > 0 for k in ("offset", "limit"))
                if name != "Read" or not bounded:
                    counts["broad_before" if first_event is None else "broad_after"] += 1
            if name == "Bash":
                counts["unclassified_shell_calls"] += 1
                counts["shell_index_keyword_flags"] += int(".repogrammar" in str(inp.get("command", "")))
            if name in ("Edit", "Write", "MultiEdit"):
                path = _path(inp.get("file_path"), root)
                relevant = [(unknown, stale) for paths, unknown, stale, _, reads_after in armed
                            if path in paths and path not in reads_after]
                counts["unknown_unread_edits"] += int(any(u or s for u, s in relevant))
                counts["stale_unread_edits"] += int(any(s for _, s in relevant))
        else:
            call = pending.pop(tool.get("tool_use_id"), None)
            if call is None:
                raise ValueError("unpaired tool result")
            name, text = call["name"], tool.get("text") or ""
            size = len(text.encode("utf-8"))
            if name == MCP_TOOL_NAME:
                counts["mcp_result_bytes"] += size
                try:
                    mcp = current_mcp(text, root)
                except (ValueError, TypeError, AttributeError):
                    counts["mcp_parse_errors"] += 1
                    continue
                counts["read_plan_items"] += mcp["items"]
                if mcp["family"]:
                    families.add(hashlib.sha256(mcp["family"].encode()).hexdigest())
                counts["unscoped_abstentions"] += int(
                    (mcp["unknown"] and not mcp["paths"]) or
                    (mcp["stale"] and not mcp["stale_paths"]))
                if mcp["unknown"]:
                    armed.append((mcp["paths"], True, False, index, set()))
                if mcp["stale"]:
                    armed.append((mcp["stale_paths"], False, True, index, set()))
            elif name == "Read":
                counts["failed_read_result_bytes" if tool["is_error"] else "read_result_bytes"] += size
                counts["failed_read_calls"] += int(tool["is_error"])
                path = _path(call["input"].get("file_path"), root)
                if path and not tool["is_error"]:
                    files.add(path)
                    for _, _, _, flag_index, reads_after in armed:
                        if call["use_index"] > flag_index:
                            reads_after.add(path)
            elif name in ("Grep", "Glob"):
                counts["search_result_bytes"] += size
    counts["missing_tool_results"] = len(pending)
    host = {"input_tokens": usage.get("input_tokens"), "output_tokens": usage.get("output_tokens"),
            "cache_read_tokens": usage.get("cache_read_input_tokens"),
            "cache_creation_tokens": usage.get("cache_creation_input_tokens"),
            "cost_usd": result.get("total_cost_usd"), "turns": result.get("num_turns"),
            "duration_ms": result.get("duration_ms")}
    if any(v is not None and (isinstance(v, bool) or not isinstance(v, (int, float))
                             or not math.isfinite(v) or v < 0) for v in host.values()):
        raise ValueError("invalid host measurement")
    if any(v is not None and not isinstance(v, int) for k, v in host.items() if k != "cost_usd"):
        raise ValueError("non-integral host count")
    return {"schema_version": "agent-adoption-metrics.v2", "counts": counts,
            "distinct_files_acquired": len(files), "selected_family_sha256": sorted(families),
            "first_invocation_turn": first_turn, "first_invocation_event": first_event,
            "assistant_message_count": turns, "host": host,
            "missing_host_fields": sorted(k for k, v in host.items() if v is None)}


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("transcript")
    parser.add_argument("--root", required=True)
    args = parser.parse_args()
    with open(args.transcript, "rb") as stream:
        raw = stream.read(64 * 1024 * 1024 + 1)
    if len(raw) > 64 * 1024 * 1024:
        parser.error("transcript exceeds 64 MiB")
    events = [json.loads(line, object_pairs_hook=_unique_object) for line in raw.decode("utf-8").splitlines() if line.strip()]
    print(json.dumps(measure(events, args.root), sort_keys=True, allow_nan=False))
