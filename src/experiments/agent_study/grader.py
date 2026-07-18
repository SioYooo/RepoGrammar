"""Offline mechanical acceptance grading (design §6 mechanical gate).

The pilot grades entirely offline with no network and no test execution at task
time (the design's offline-env rule; the pilot denies Bash to the agent, so
there is no repo venv to exercise). Two gradable oracle kinds:

  patch_static_assert (T1) — regex assertions over the agent's post-run
      worktree, plus a "something changed" guard. Verdict: gate_pass if every
      assertion holds and (when required) at least one in-scope file changed;
      no_patch if nothing changed; gate_fail otherwise.

  answer_checklist (T6) — the agent's final answer text is checked against a
      gold must-have fact checklist (each fact satisfied by any of its
      alternative regexes) plus forbidden false-claim guards. Verdict: gate_pass
      iff all must-have facts present and no forbidden claim present.

The grader is generic; the concrete assertions/checklists live in the task
JSON, authored at freeze against the pinned repo. This module never sees the
prompt text and returns only counts + a verdict.
"""

from __future__ import annotations

import glob
import os
import re
from typing import Any, Dict, List, Optional


def _matched_files(root: str, path_glob: str) -> List[str]:
    pattern = os.path.join(root, path_glob)
    return [p for p in glob.glob(pattern, recursive=True) if os.path.isfile(p)]


def _read(path: str) -> str:
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as fh:
            return fh.read()
    except OSError:
        return ""


def _eval_assertion(root: str, assertion: Dict[str, Any], added_text: Optional[str]) -> bool:
    """Evaluate one assertion.

    scope="added" (design-correct for convention checks) runs the regex over the
    diff's added lines (`added_text`) so a pre-existing occurrence elsewhere in a
    changed file cannot satisfy it — the AGENT's added code must carry the
    convention. scope="worktree" (default) runs over matched files' full content.
    """
    atype = assertion.get("type")
    regex = assertion.get("regex", "")
    rx = re.compile(regex)
    scope = assertion.get("scope", "worktree")
    if scope == "added":
        if added_text is None:
            raise ValueError("assertion scope 'added' requires added_text")
        any_match = bool(rx.search(added_text))
    elif scope == "worktree":
        files = _matched_files(root, assertion.get("path_glob", ""))
        any_match = any(rx.search(_read(p)) for p in files)
    else:
        raise ValueError(f"unknown assertion scope: {scope}")
    if atype == "any_file_matches":
        return any_match
    if atype == "no_file_matches":
        return not any_match
    raise ValueError(f"unknown assertion type: {atype}")


def grade_patch_static(
    task: Dict[str, Any], worktree_dir: str, changed_files: Optional[List[str]] = None,
    added_text: Optional[str] = None,
) -> Dict[str, Any]:
    oracle = task["oracle"]
    assertions: List[Dict[str, Any]] = oracle.get("assertions", [])
    require_changed = oracle.get("require_changed", True)

    if require_changed and not changed_files:
        return {
            "verdict": "no_patch",
            "tests_passed": 0,
            "tests_failed": len(assertions),
            "detail": [{"desc": "worktree changed", "ok": False}],
        }

    passed = 0
    detail = []
    for a in assertions:
        ok = _eval_assertion(worktree_dir, a, added_text)
        detail.append({"desc": a.get("desc", a.get("type")), "ok": ok, "scope": a.get("scope", "worktree")})
        passed += 1 if ok else 0
    failed = len(assertions) - passed
    verdict = "gate_pass" if failed == 0 else "gate_fail"
    return {"verdict": verdict, "tests_passed": passed, "tests_failed": failed, "detail": detail}


def grade_answer_checklist(task: Dict[str, Any], answer_text: str) -> Dict[str, Any]:
    oracle = task["oracle"]
    must_have: List[Dict[str, Any]] = oracle.get("must_have", [])
    forbidden: List[Dict[str, Any]] = oracle.get("forbidden", [])
    text = answer_text or ""

    detail = []
    present = 0
    for fact in must_have:
        alts = fact.get("any_regex", [])
        ok = any(re.search(rx, text, re.IGNORECASE) for rx in alts)
        detail.append({"desc": fact.get("fact"), "ok": ok, "kind": "must_have"})
        present += 1 if ok else 0

    forbidden_hit = 0
    for fact in forbidden:
        alts = fact.get("any_regex", [])
        hit = any(re.search(rx, text, re.IGNORECASE) for rx in alts)
        detail.append({"desc": fact.get("fact"), "ok": not hit, "kind": "forbidden"})
        forbidden_hit += 1 if hit else 0

    missing = len(must_have) - present
    verdict = "gate_pass" if (missing == 0 and forbidden_hit == 0) else "gate_fail"
    return {
        "verdict": verdict,
        "tests_passed": present,
        "tests_failed": missing + forbidden_hit,
        "detail": detail,
    }


def grade(
    task: Dict[str, Any],
    worktree_dir: Optional[str] = None,
    answer_text: Optional[str] = None,
    changed_files: Optional[List[str]] = None,
    added_text: Optional[str] = None,
) -> Dict[str, Any]:
    """Dispatch on the task's oracle kind and return the grading result."""
    kind = task["oracle"]["kind"]
    if kind == "patch_static_assert":
        if worktree_dir is None:
            raise ValueError("patch_static_assert requires worktree_dir")
        return grade_patch_static(task, worktree_dir, changed_files, added_text)
    if kind == "answer_checklist":
        return grade_answer_checklist(task, answer_text or "")
    raise ValueError(f"unknown oracle kind: {kind}")
