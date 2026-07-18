"""Re-derive pilot verdicts/metrics from saved transcripts (zero API spend).

Grading and parsing are deterministic functions of the saved transcript (+ the
saved per-run worktree for T1). This tool re-derives every committed record's
verdict, safety counters, and metrics with the CURRENT (post-review) parser,
grader, and calibrated checklists, and writes a committed, hash-anchored
summary artifact. It launches no agent and needs no network.

Why this exists (adversarial review N2 + S1): the committed `agent-study-run.v1`
ledger holds the AS-RUN verdicts (T1 `gate_pass`, T6 `gate_fail`). The as-run
T6 checklist was miscalibrated (order-sensitive fact-4 regex) and the as-run T1
grader over-credited (worktree scope). This artifact backs the doc's claims —
that the calibrated T6 checklist yields gate_pass 4/4 and the scoped T1 grader
does not flip the four T1 verdicts — with reproducible, committed evidence
rather than a manual assertion.

Inputs are the committed ledger + the local (untracked) transcript archive under
--work-base. Output: `docs/experiments/data/agent-study-regrade.v1.json`.

Usage:
  python3 regrade.py --work-base /path/to/work-base [--ledger <path>] [--out <path>]
"""

from __future__ import annotations

import argparse
import glob
import hashlib
import json
import os
import subprocess
import sys
from typing import Any, Dict, List, Optional

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import detectors  # noqa: E402
import grader  # noqa: E402
import parsers  # noqa: E402


def _sha256_file(path: str) -> Optional[str]:
    try:
        h = hashlib.sha256()
        with open(path, "rb") as fh:
            for chunk in iter(lambda: fh.read(65536), b""):
                h.update(chunk)
        return h.hexdigest()
    except OSError:
        return None


def _git_added_text(worktree: str) -> Optional[str]:
    if not os.path.isdir(os.path.join(worktree, ".git")):
        return None
    patch = subprocess.run(["git", "-C", worktree, "diff"], stdout=subprocess.PIPE,
                           stderr=subprocess.DEVNULL, text=True).stdout
    names = subprocess.run(["git", "-C", worktree, "diff", "--name-only"], stdout=subprocess.PIPE,
                           stderr=subprocess.DEVNULL, text=True).stdout.split()
    added = "\n".join(ln[1:] for ln in patch.splitlines()
                      if ln.startswith("+") and not ln.startswith("+++"))
    return json.dumps({"added_text": added, "changed_files": names})


def _find_run_dir(work_base: str, run_id: str) -> Optional[str]:
    for base in ("real", ""):
        pat = os.path.join(work_base, base, f"run-*-{run_id[:8]}")
        hits = glob.glob(pat)
        if hits:
            return hits[0]
    return None


def regrade(work_base: str, ledger: str, out_path: str) -> int:
    t1_task = json.load(open(os.path.join(HERE, "tasks", "t1_item_summary.json")))
    t6_task = json.load(open(os.path.join(HERE, "tasks", "t6_db_session.json")))
    with open(ledger) as fh:
        recs = [json.loads(ln) for ln in fh]

    rows: List[Dict[str, Any]] = []
    for r in recs:
        run_dir = _find_run_dir(work_base, r["run_id"])
        row: Dict[str, Any] = {
            "run_id": r["run_id"], "task_type": r["task_type"], "arm": r["arm"],
            "rep_index": r["rep_index"], "as_run_outcome": r["outcome"],
        }
        if not run_dir:
            row["rederived_outcome"] = "transcript_unavailable"
            rows.append(row)
            continue
        transcript = os.path.join(run_dir, "transcript.jsonl")
        row["transcript_sha256"] = _sha256_file(transcript)
        parsed = parsers.parse_transcript(transcript)
        row["safety_rederived"] = detectors.run_all(parsed["tool_events"], parsed["mcp_results"])
        row["mcp_calls"] = parsed["mcp"]["calls"]
        row["files_read"] = parsed["context"]["files_read"]
        row["bytes_read"] = parsed["context"]["bytes_read"]
        row["mcp_result_bytes"] = parsed["mcp"]["result_bytes"]
        if r["task_type"] == "T1":
            wt = os.path.join(run_dir, "worktree")
            info = _git_added_text(wt)
            if info is None:
                row["rederived_outcome"] = "worktree_unavailable"
            else:
                d = json.loads(info)
                g = grader.grade(t1_task, worktree_dir=wt, changed_files=d["changed_files"],
                                 added_text=d["added_text"])
                row["rederived_outcome"] = g["verdict"]
                row["tests_passed"] = g["tests_passed"]
                row["tests_failed"] = g["tests_failed"]
        else:
            g = grader.grade(t6_task, answer_text=parsed.get("final_text", ""))
            row["rederived_outcome"] = g["verdict"]
            row["tests_passed"] = g["tests_passed"]
            row["tests_failed"] = g["tests_failed"]
        rows.append(row)

    t1 = [x for x in rows if x["task_type"] == "T1"]
    t6 = [x for x in rows if x["task_type"] == "T6"]
    summary = {
        "schema_version": "agent-study-regrade.v1",
        "note": ("Deterministic re-derivation of pilot verdicts with the post-review "
                 "parser/grader/calibrated checklists. Zero API spend. Inputs: the "
                 "committed agent-study-run.v1 ledger + the local untracked transcript "
                 "archive (transcript_sha256 anchors each run)."),
        "t1_scoped_grader_flips": sum(1 for x in t1
                                      if x.get("rederived_outcome") not in (x["as_run_outcome"], None)),
        "t6_calibrated_gate_pass": sum(1 for x in t6 if x.get("rederived_outcome") == "gate_pass"),
        "t6_total": len(t6),
        "runs": rows,
    }
    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(summary, fh, indent=2, sort_keys=True)
        fh.write("\n")

    print(f"wrote {out_path}")
    print(f"T1 scoped-grader verdict flips vs as-run: {summary['t1_scoped_grader_flips']}")
    print(f"T6 calibrated-checklist gate_pass: {summary['t6_calibrated_gate_pass']}/{summary['t6_total']}")
    for x in rows:
        print(f"  {x['task_type']}/{x['arm']}/r{x['rep_index']}: as_run={x['as_run_outcome']} "
              f"-> rederived={x.get('rederived_outcome')}")
    return 0


def main(argv: Optional[List[str]] = None) -> int:
    import tempfile
    ap = argparse.ArgumentParser(description="Re-derive pilot verdicts from saved transcripts (zero spend)")
    ap.add_argument("--work-base", default=os.path.join(tempfile.gettempdir(), "repogrammar-agent-study"))
    ap.add_argument("--ledger", default=os.path.abspath(
        os.path.join(HERE, "..", "..", "..", "docs", "experiments", "data", "agent-study-run.v1.jsonl")))
    ap.add_argument("--out", default=os.path.abspath(
        os.path.join(HERE, "..", "..", "..", "docs", "experiments", "data", "agent-study-regrade.v1.json")))
    args = ap.parse_args(argv)
    return regrade(args.work_base, args.ledger, args.out)


if __name__ == "__main__":
    raise SystemExit(main())
