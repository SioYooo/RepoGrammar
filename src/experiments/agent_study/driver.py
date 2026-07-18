"""RQ5 agent-study PILOT harness driver.

Orchestrates the minimal harness-proving subset of the Phase 7 agent-study
design: ONE pinned repo, two mechanically-gradable task types (T1 analogue-
guided feature add, T6 comprehension question), two arms (A0 baseline vs A3 full
product), N=2 reps => 8 real runs. Proves harness MECHANICS, not effects.

Two modes:

  --dry-run  Zero-spend end-to-end pipeline exercise. Launches NO agent process
             and needs NO network. Feeds committed scripted fixture transcripts
             through the full pipeline (parse -> detect -> grade -> record ->
             cost accounting), including the four seeded detector transcripts,
             and asserts the expected outcomes. Proves the plumbing before any
             API spend.

  (default)  Real pilot. Clones+pins the repo, builds a shared .repogrammar
             index tarball, and for each cell launches claude-code headless with
             an arm-specific MCP config under a throwaway HOME, capturing the
             stream-json transcript. Cost is read from host-reported usage and
             the run ABORTS if cumulative cost reaches the safety threshold.

Budget: hard cap enforced two ways — per-run via claude `--max-budget-usd`, and
cumulatively in this driver (abort at --abort-at, default $27, under the $30
authorization). Cost is host-reported (`total_cost_usd` in the result event).

Committed artifacts contain only hashes/counts/repo-relative paths (design §6).
Raw transcripts and patches stay in the local untracked work base.
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import random
import shutil
import subprocess
import sys
import tempfile
import uuid
from typing import Any, Dict, List, Optional, Tuple

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import detectors  # noqa: E402
import grader  # noqa: E402
import parsers  # noqa: E402
import record_schema  # noqa: E402
from treehash import tree_sha256  # noqa: E402

# ----- pilot constants (design + brief) -----
PILOT_MODEL = "claude-haiku-4-5-20251001"  # exact dated snapshot; cheapest tier
DEFAULT_ARMS = ["A0", "A3"]
DEFAULT_REPS = 2
DEFAULT_ABORT_AT = 27.0  # USD cumulative safety margin under the $30 cap
DEFAULT_PER_RUN_CAP = 2.0  # USD, also bounded by remaining budget
DEFAULT_TIMEOUT_S = 300  # pilot wall-clock per run (full grid uses 20 min)
DEFAULT_REPO_URL = "https://github.com/fastapi/full-stack-fastapi-template"
SHUFFLE_SEED = 20260718


# ----- small hashing helpers -----
def sha256_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def sha256_text(s: str) -> str:
    return sha256_bytes(s.encode("utf-8"))


def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def now_rfc3339() -> str:
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def _repo_root() -> Optional[str]:
    try:
        top = subprocess.run(["git", "-C", HERE, "rev-parse", "--show-toplevel"],
                             check=True, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                             text=True).stdout.strip()
        return os.path.realpath(top) if top else None
    except (subprocess.SubprocessError, OSError):
        return None


def assert_work_base_outside_repo(work_base: str) -> None:
    """Refuse a work base inside the repo tree (S6).

    Raw transcripts + per-run worktrees live under the work base and must never
    enter the repo (transcripts carry source excerpts; an in-repo worktree would
    also let claude's CWD-upward CLAUDE.md discovery reach the RepoGrammar
    CLAUDE.md, breaking leak control). We refuse rather than rely on .gitignore.
    """
    root = _repo_root()
    wb = os.path.realpath(os.path.abspath(work_base))
    if root is not None:
        common = os.path.commonpath([root, wb])
        if common == root:
            raise SystemExit(
                f"refusing --work-base inside the repo tree ({wb} is under {root}). "
                f"Point it at a path outside the repository (default is a temp dir).")
    # Belt-and-suspenders: ignore everything under the work base if it is ever a
    # git tree of its own.
    os.makedirs(wb, exist_ok=True)
    gi = os.path.join(wb, ".gitignore")
    if not os.path.exists(gi):
        with open(gi, "w", encoding="utf-8") as fh:
            fh.write("*\n")


# ----- task loading -----
def load_task(path: str) -> Dict[str, Any]:
    with open(path, "r", encoding="utf-8") as fh:
        return json.load(fh)


# ----- run plan -----
class RunSpec:
    def __init__(self, task: Dict[str, Any], arm: str, rep: int):
        self.task = task
        self.arm = arm
        self.rep = rep
        self.run_id = uuid.uuid4().hex

    @property
    def task_id(self) -> str:
        return self.task["task_id"]

    @property
    def task_type(self) -> str:
        return self.task["task_type"]


def build_plan(tasks: List[Dict[str, Any]], arms: List[str], reps: int, seed: int) -> List[RunSpec]:
    plan: List[RunSpec] = []
    for task in tasks:
        for arm in arms:
            for rep in range(1, reps + 1):
                plan.append(RunSpec(task, arm, rep))
    random.Random(seed).shuffle(plan)  # interleaved randomized order (design §4)
    return plan


# ----- MCP config rendering -----
def render_mcp_config(arm: str, repogrammar_bin: str, project_dir: str, out_dir: str) -> Tuple[str, str]:
    """Write the concrete per-run MCP config for `arm`; return (path, sha256)."""
    if arm == "A0":
        src = os.path.join(HERE, "configs", "mcp-A0.json")
        with open(src, "r", encoding="utf-8") as fh:
            content = fh.read()
    elif arm == "A3":
        src = os.path.join(HERE, "configs", "mcp-A3.template.json")
        with open(src, "r", encoding="utf-8") as fh:
            content = fh.read()
        content = content.replace("{{REPOGRAMMAR_BIN}}", repogrammar_bin).replace(
            "{{PROJECT_DIR}}", project_dir
        )
    else:
        raise ValueError(f"unknown arm {arm}")
    path = os.path.join(out_dir, f"mcp-{arm}.json")
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(content)
    return path, sha256_text(content)


# ----- git snapshot / diff -----
def git_init_snapshot(worktree: str) -> None:
    env = os.environ.copy()
    env.setdefault("GIT_AUTHOR_NAME", "study")
    env.setdefault("GIT_AUTHOR_EMAIL", "study@example.invalid")
    env.setdefault("GIT_COMMITTER_NAME", "study")
    env.setdefault("GIT_COMMITTER_EMAIL", "study@example.invalid")
    for args in (["init", "-q"], ["add", "-A"], ["commit", "-q", "-m", "snapshot", "--no-gpg-sign"]):
        subprocess.run(["git", "-C", worktree] + args, env=env, check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def git_diff_info(worktree: str) -> Dict[str, Any]:
    def run(args: List[str]) -> str:
        return subprocess.run(["git", "-C", worktree] + args, check=True,
                              stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True).stdout

    names = [n for n in run(["diff", "--name-only"]).splitlines() if n.strip()]
    patch = run(["diff"])
    # added lines only (drop the "+++" file headers) — the agent's new code,
    # used for scope="added" assertions (adversarial review S1).
    added_text = "\n".join(
        ln[1:] for ln in patch.splitlines() if ln.startswith("+") and not ln.startswith("+++")
    )
    insertions = deletions = 0
    for line in run(["diff", "--numstat"]).splitlines():
        parts = line.split("\t")
        if len(parts) >= 2:
            try:
                insertions += int(parts[0])
                deletions += int(parts[1])
            except ValueError:
                pass
    return {
        "changed_files": names,
        "sha256": sha256_text(patch) if patch else None,
        "insertions": insertions,
        "deletions": deletions,
        "added_text": added_text,
    }


# ----- claude invocation -----
def run_claude(
    prompt: str,
    worktree: str,
    home_dir: str,
    mcp_config: str,
    settings: str,
    model: str,
    per_run_cap: float,
    timeout_s: int,
    claude_bin: str,
    transcript_path: str,
    stderr_path: str,
) -> str:
    """Launch claude-code headless; write the stream-json transcript.

    Returns a run-status token: "ok" | "timeout" | "budget" | "infra".
    The transcript file is written even on failure (may be empty on infra).
    """
    os.makedirs(home_dir, exist_ok=True)
    env = os.environ.copy()
    # Auth vs leak-control tension (see agent-study-pilot.md):
    #   "isolated" — point CLAUDE_CONFIG_DIR at an empty dir so the global
    #      ~/.claude/CLAUDE.md (RepoGrammar gate), user memory, plugins and
    #      skills are not discovered. This is the correct target, but on this
    #      host the keychain OAuth credential is coupled to the default config
    #      path, so an alternate/empty dir yields "Not logged in".
    #   "real" — use the host's real config (keychain auth works); the global
    #      CLAUDE.md is present in every arm (symmetric). Pilot-only fallback.
    #   The full grid MUST use ANTHROPIC_API_KEY + throwaway HOME (design §4) so
    #      the correct isolated mode authenticates without the leak.
    auth_mode = os.environ.get("AGENT_STUDY_AUTH_MODE", "isolated")
    if auth_mode == "isolated":
        empty_config = os.path.join(home_dir, ".claude-empty")
        os.makedirs(empty_config, exist_ok=True)
        env["CLAUDE_CONFIG_DIR"] = empty_config
    # "real": leave HOME and CLAUDE_CONFIG_DIR untouched.
    # keep provider CLI off PATH: do not add the repo target dir. (Nothing to strip
    # here since it was never added; MCP launches the server by absolute path.)

    cmd = [
        claude_bin, "-p",
        "--model", model,
        "--output-format", "stream-json", "--verbose",
        "--mcp-config", mcp_config, "--strict-mcp-config",
        "--settings", settings,
        "--permission-mode", "acceptEdits",
        "--max-budget-usd", str(per_run_cap),
        "--add-dir", worktree,
    ]
    try:
        with open(transcript_path, "w", encoding="utf-8") as out, open(stderr_path, "w", encoding="utf-8") as err:
            proc = subprocess.run(
                cmd, cwd=worktree, env=env, input=prompt, text=True,
                stdout=out, stderr=err, timeout=timeout_s,
            )
    except subprocess.TimeoutExpired:
        return "timeout"
    except (OSError, ValueError):
        return "infra"
    # Inspect transcript for a result event; classify budget/infra.
    parsed = parsers.parse_transcript(transcript_path)
    if not parsed["result_present"]:
        return "infra"
    subtype = str(parsed.get("result_subtype") or "")
    if "budget" in subtype.lower() or "max_budget" in subtype.lower():
        return "budget"
    # An error result (e.g. "Not logged in", API/transport error) with no token
    # usage is an infrastructure failure, not a legitimate no-edit outcome.
    toks = parsed.get("tokens", {})
    no_usage = (toks.get("input") in (0, None)) and (toks.get("output") in (0, None))
    if parsed.get("result_is_error") and no_usage:
        return "infra"
    return "ok"


# ----- record assembly -----
def assemble_record(
    spec: RunSpec, parsed: Dict[str, Any], safety: Dict[str, int], grading: Dict[str, Any],
    *, repo_sha: str, worktree_sha256: str, mcp_config_sha256: str, prompt_sha256: str,
    cli_version: str, model: str, outcome: str, started_at: str, finished_at: str,
    diff_info: Optional[Dict[str, Any]], transcript_basename: str, notes: Optional[str] = None,
) -> Dict[str, Any]:
    rec = record_schema.new_record()
    rec["run_id"] = spec.run_id
    rec["rep_index"] = spec.rep
    rec["task_id"] = spec.task_id
    rec["task_type"] = spec.task_type
    rec["repo_id"] = spec.task.get("repo_id")
    rec["repo_sha"] = repo_sha
    rec["worktree_sha256"] = worktree_sha256
    rec["arm"] = spec.arm
    rec["mcp_config_sha256"] = mcp_config_sha256
    rec["agent"] = {"cli": "claude-code", "cli_version": cli_version, "model": model}
    rec["prompt_template_id"] = spec.task.get("prompt_template_id")
    rec["prompt_sha256"] = prompt_sha256
    rec["started_at"] = started_at
    rec["finished_at"] = finished_at
    rec["duration_ms"] = parsed.get("duration_ms", 0)
    rec["outcome"] = outcome
    rec["oracle"] = {
        "oracle_id": spec.task["oracle"].get("oracle_id"),
        "commands_sha256": sha256_text(json.dumps(spec.task["oracle"], sort_keys=True)),
        "tests_passed": grading.get("tests_passed", 0),
        "tests_failed": grading.get("tests_failed", 0),
    }
    if diff_info is not None:
        rec["diff"] = {
            "sha256": diff_info.get("sha256"),
            "files_touched": len(diff_info.get("changed_files", [])),
            "insertions": diff_info.get("insertions", 0),
            "deletions": diff_info.get("deletions", 0),
        }
    rec["tokens"] = parsed["tokens"]
    rec["context"] = parsed["context"]
    rec["mcp"] = parsed["mcp"]
    rec["safety"] = safety
    rec["compaction_count"] = parsed.get("compaction_count", 0)
    rec["num_turns"] = parsed.get("num_turns", 0)
    rec["transcript_path"] = transcript_basename  # basename only in committed record
    rec["notes"] = notes
    return rec


def grade_and_detect(spec: RunSpec, parsed: Dict[str, Any], worktree: Optional[str],
                     changed_files: Optional[List[str]],
                     added_text: Optional[str] = None) -> Tuple[Dict[str, int], Dict[str, Any]]:
    safety = detectors.run_all(parsed["tool_events"], parsed["mcp_results"])
    if spec.task["oracle"]["kind"] == "patch_static_assert":
        grading = grader.grade(spec.task, worktree_dir=worktree, changed_files=changed_files,
                               added_text=added_text)
    else:
        grading = grader.grade(spec.task, answer_text=parsed.get("final_text", ""))
    return safety, grading


def classify_outcome(run_status: str, grading: Dict[str, Any]) -> str:
    if run_status == "timeout":
        return "timeout"
    if run_status == "infra":
        return "infra_failure"
    if run_status == "budget":
        return "budget_exceeded"
    return grading["verdict"]


# ============================ DRY RUN ============================
def _dry_mini_t1_task() -> Dict[str, Any]:
    """A mini T1 task graded against the mini_repo (app/*.py), for the dry run."""
    return {
        "task_id": "t1-mini-dry",
        "task_type": "T1",
        "repo_id": "py-app-template",
        "prompt_template_id": "t1a-v1",
        "prompt": "dry-run mini task",
        "oracle": {
            "oracle_id": "t1-mini-static-v1",
            "kind": "patch_static_assert",
            "require_changed": True,
            "assertions": [
                {"type": "any_file_matches", "scope": "added", "path_glob": "app/*.py",
                 "regex": "@router\\.get\\(\\s*\"[^\"]*summary\"", "desc": "summary route added"},
                {"type": "any_file_matches", "scope": "added", "path_glob": "app/*.py",
                 "regex": "SessionDep", "desc": "added code uses SessionDep"},
                {"type": "any_file_matches", "scope": "added", "path_glob": "app/*.py",
                 "regex": "CurrentUser", "desc": "added code uses CurrentUser"},
                {"type": "any_file_matches", "scope": "added", "path_glob": "app/*.py",
                 "regex": "response_model\\s*=", "desc": "added route declares response_model"},
            ],
        },
    }


def _apply_mini_solution(worktree: str) -> List[str]:
    """Simulate the agent's T1 edit on a mini_repo copy; return changed files."""
    routes = os.path.join(worktree, "app", "routes.py")
    solved = (
        '\n\n@router.get("/{id}/summary", response_model=dict)\n'
        'def item_summary(id: int, session: SessionDep, current_user: CurrentUser):\n'
        '    return {"id": id, "summary": True}\n'
    )
    with open(routes, "a", encoding="utf-8") as fh:
        fh.write(solved)
    return ["app/routes.py"]


def run_dry(args: argparse.Namespace) -> int:
    work = os.path.join(args.work_base, "dry-run")
    if os.path.isdir(work):
        shutil.rmtree(work)
    os.makedirs(work, exist_ok=True)
    ledger = os.path.join(work, "agent-study-run.v1.dryrun.jsonl")

    t1_task = load_task(os.path.join(HERE, "tasks", "t1_item_summary.json"))
    t6_task = load_task(os.path.join(HERE, "tasks", "t6_db_session.json"))
    mini_repo = os.path.join(HERE, "fixtures", "mini_repo")
    fx = os.path.join(HERE, "fixtures", "transcripts")

    fixture_map = {
        ("T1", "A0"): "transcript_t1_a0.jsonl",
        ("T1", "A3"): "transcript_a3_success.jsonl",
        ("T6", "A0"): "transcript_t6_answer.jsonl",
        ("T6", "A3"): "transcript_t6_a3.jsonl",
    }

    tasks = [t1_task, t6_task]
    plan = build_plan(tasks, DEFAULT_ARMS, DEFAULT_REPS, SHUFFLE_SEED)

    rows: List[Dict[str, Any]] = []
    cumulative_cost = 0.0
    worktree_hashes: Dict[str, List[str]] = {}  # task_id -> pre-edit worktree hashes

    for spec in plan:
        transcript = os.path.join(fx, fixture_map[(spec.task_type, spec.arm)])
        parsed = parsers.parse_transcript(transcript)

        worktree = None
        changed_files = None
        diff_info = None
        wt_hash = None
        if spec.task_type == "T1":
            # fresh mini_repo copy per run; hash BEFORE simulated edit (isolation check)
            worktree = os.path.join(work, f"wt-{spec.run_id}")
            shutil.copytree(mini_repo, worktree)
            wt_hash = tree_sha256(worktree)
            git_init_snapshot(worktree)
            changed_files = _apply_mini_solution(worktree)
            diff_info = git_diff_info(worktree)
            grade_task = _dry_mini_t1_task()
            spec_task_backup = spec.task
            spec.task = grade_task  # grade mini task against mini_repo
        else:
            wt_hash = "T6-no-worktree"

        worktree_hashes.setdefault(spec.task_id, []).append(wt_hash)
        added_text = diff_info["added_text"] if diff_info else None
        safety, grading = grade_and_detect(spec, parsed, worktree, changed_files, added_text)
        if spec.task_type == "T1":
            spec.task = spec_task_backup  # restore real task metadata for the record

        run_status = "ok" if parsed["result_present"] else "infra"
        outcome = classify_outcome(run_status, grading)

        cost = parsed["tokens"].get("total_cost_usd") or 0.0
        cumulative_cost += cost  # simulated accounting; no real spend

        rec = assemble_record(
            spec, parsed, safety, grading,
            repo_sha="DRYRUN0000000000000000000000000000000000",
            worktree_sha256=wt_hash, mcp_config_sha256="dryrun-mcp-" + spec.arm,
            prompt_sha256=sha256_text(spec.task.get("prompt", "")),
            cli_version="dry-run", model=PILOT_MODEL, outcome=outcome,
            started_at=now_rfc3339(), finished_at=now_rfc3339(),
            diff_info=diff_info, transcript_basename=os.path.basename(transcript),
            notes="dry-run: scripted fixture transcript; zero API spend",
        )
        record_schema.append_record(ledger, rec)
        rows.append({"task": spec.task_type, "arm": spec.arm, "rep": spec.rep,
                     "outcome": outcome, "cost": cost, "safety": safety,
                     "mcp_calls": parsed["mcp"]["calls"], "files_read": parsed["context"]["files_read"]})

    # ---- seeded detector runs (design §9 criterion 4) ----
    detector_expect = {
        "transcript_clean.jsonl": {"unknown_override": 0, "stale_evidence_use": 0, "index_peek": 0},
        "transcript_stale.jsonl": {"stale_evidence_use": 1},
        "transcript_unknown.jsonl": {"unknown_override": 1, "stale_evidence_use": 0},
        "transcript_indexpeek.jsonl": {"index_peek": 2},
    }
    detector_rows = []
    detector_ok = True
    for fname, expect in detector_expect.items():
        parsed = parsers.parse_transcript(os.path.join(fx, fname))
        safety = detectors.run_all(parsed["tool_events"], parsed["mcp_results"])
        ok = all(safety.get(k) == v for k, v in expect.items())
        detector_ok = detector_ok and ok
        detector_rows.append({"fixture": fname, "expect": expect, "got": safety, "ok": ok})

    # ---- report ----
    print("=== DRY RUN (zero spend) ===")
    print(f"ledger: {ledger}")
    print("\nGrid runs (8 = 2 tasks x 2 arms x 2 reps):")
    print(f"  {'task':4} {'arm':3} {'rep':3} {'outcome':10} {'cost':7} {'mcp':3} {'reads':5} safety")
    for r in rows:
        print(f"  {r['task']:4} {r['arm']:3} {r['rep']:<3} {r['outcome']:10} "
              f"{r['cost']:<7.4f} {r['mcp_calls']:<3} {r['files_read']:<5} {r['safety']}")
    print("\nSeeded detector runs (design §9.4):")
    for d in detector_rows:
        print(f"  {d['fixture']:32} expect={d['expect']} got={d['got']} {'OK' if d['ok'] else 'MISMATCH'}")

    # ---- assertions ----
    errors: List[str] = []
    if len(rows) != 8:
        errors.append(f"expected 8 grid runs, got {len(rows)}")
    for r in rows:
        if r["outcome"] != "gate_pass":
            errors.append(f"grid cell {r['task']}/{r['arm']}/{r['rep']} outcome {r['outcome']} != gate_pass")
    # arm isolation (S3): per task, the pre-edit worktree hash must be IDENTICAL
    # across all arms/reps (real assertion, not a no-op) — and the committed
    # records must agree, with A0's and A3's mcp_config_sha256 differing.
    with open(ledger) as fh:
        recs = [json.loads(ln) for ln in fh]
    for task_id, hashes in worktree_hashes.items():
        if len(set(hashes)) != 1:
            errors.append(f"arm isolation FAILED for {task_id}: worktree hashes differ {set(hashes)}")
    by_task: Dict[str, set] = {}
    mcp_by_arm: Dict[str, set] = {"A0": set(), "A3": set()}
    for rec in recs:
        by_task.setdefault(rec["task_id"], set()).add(rec["worktree_sha256"])
        mcp_by_arm.setdefault(rec["arm"], set()).add(rec["mcp_config_sha256"])
    for task_id, wts in by_task.items():
        if len(wts) != 1:
            errors.append(f"arm isolation FAILED in records for {task_id}: {wts}")
    if mcp_by_arm["A0"] and mcp_by_arm["A0"] == mcp_by_arm["A3"]:
        errors.append("arm isolation FAILED: A0 and A3 share an mcp_config_sha256")
    if len(recs) != 8:
        errors.append(f"expected 8 records in ledger, got {len(recs)}")
    if not detector_ok:
        errors.append("one or more seeded detector expectations did not match")
    # MCP adoption sanity: A3 grid runs should show mcp calls > 0
    a3_mcp = [r for r in rows if r["arm"] == "A3"]
    if not all(r["mcp_calls"] > 0 for r in a3_mcp):
        errors.append("A3 dry-run cells did not all record mcp calls")

    print(f"\ncumulative (simulated) cost: ${cumulative_cost:.4f}")
    print(f"real API spend this dry run: $0.0000 (no agent process launched)")
    if errors:
        print("\nDRY RUN FAILED:")
        for e in errors:
            print("  - " + e)
        return 1
    print("\nDRY RUN PASSED: pipeline (parse/detect/grade/record/cost) verified end-to-end.")
    return 0


# ============================ REAL PILOT ============================
def preflight(args: argparse.Namespace) -> Dict[str, str]:
    """Verify CLI + binary presence; refuse if version detection fails."""
    claude_bin = shutil.which("claude") or args.claude_bin
    if not claude_bin or not os.path.exists(claude_bin):
        raise SystemExit("preflight: claude CLI not found; refusing to run")
    try:
        ver = subprocess.run([claude_bin, "--version"], check=True, stdout=subprocess.PIPE,
                             stderr=subprocess.DEVNULL, text=True, timeout=30).stdout.strip()
    except (subprocess.SubprocessError, OSError):
        raise SystemExit("preflight: `claude --version` failed; refusing to run (design §4)")
    if not ver:
        raise SystemExit("preflight: empty claude version; refusing to run")
    repogrammar_bin = args.repogrammar_bin
    if not os.path.exists(repogrammar_bin):
        raise SystemExit(f"preflight: repogrammar binary missing at {repogrammar_bin}; "
                         f"run `cargo build --bin repogrammar` first")
    return {"claude_bin": claude_bin, "cli_version": ver.split()[0], "repogrammar_bin": repogrammar_bin}


def clone_and_pin(args: argparse.Namespace, work: str) -> Dict[str, Any]:
    repo_dir = os.path.join(work, "pinned-repo")
    if os.path.isdir(repo_dir):
        shutil.rmtree(repo_dir)
    subprocess.run(["git", "clone", "--quiet", args.repo_url, repo_dir], check=True)
    if args.repo_sha:
        subprocess.run(["git", "-C", repo_dir, "checkout", "--quiet", args.repo_sha], check=True)
    sha = subprocess.run(["git", "-C", repo_dir, "rev-parse", "HEAD"], check=True,
                         stdout=subprocess.PIPE, text=True).stdout.strip()
    # strip the upstream .git so the tree hash is over source only, and so per-run
    # worktrees get a clean synthetic history.
    shutil.rmtree(os.path.join(repo_dir, ".git"))
    tree = tree_sha256(repo_dir)
    return {"repo_dir": repo_dir, "repo_sha": sha, "tree_sha256": tree}


def build_index_tarball(args: argparse.Namespace, repogrammar_bin: str, repo_dir: str, work: str) -> Dict[str, Any]:
    """Build a pinned .repogrammar index once, tar it (identical for all arms)."""
    idx_src = os.path.join(work, "index-src")
    if os.path.isdir(idx_src):
        shutil.rmtree(idx_src)
    # symlinks=True preserves the template's (possibly dangling) symlinks instead
    # of following them; the tree hash skips symlinks the same way (treehash.py).
    shutil.copytree(repo_dir, idx_src, symlinks=True)
    subprocess.run([repogrammar_bin, "init", "--project", idx_src, "--yes"], check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    rp = os.path.join(idx_src, ".repogrammar")
    if not os.path.isdir(rp):
        raise SystemExit("index build produced no .repogrammar directory")
    tar = os.path.join(work, "repogrammar-index.tar")
    subprocess.run(["tar", "-cf", tar, "-C", idx_src, ".repogrammar"], check=True)
    return {"tar": tar, "sha256": sha256_file(tar)}


def make_run_worktree(repo_dir: str, index_tar: str, run_dir: str) -> str:
    worktree = os.path.join(run_dir, "worktree")
    shutil.copytree(repo_dir, worktree, symlinks=True)
    subprocess.run(["tar", "-xf", index_tar, "-C", worktree], check=True)
    return worktree


def run_real(args: argparse.Namespace) -> int:
    pf = preflight(args)
    work = os.path.join(args.work_base, "real")
    os.makedirs(work, exist_ok=True)
    ledger = args.ledger or os.path.join(
        HERE, "..", "..", "..", "docs", "experiments", "data", "agent-study-run.v1.jsonl")
    ledger = os.path.abspath(ledger)

    print("=== REAL PILOT ===")
    print(f"claude {pf['cli_version']} | model {PILOT_MODEL} | abort-at ${args.abort_at:.2f}")
    pinned = clone_and_pin(args, work)
    print(f"pinned repo {args.repo_url} @ {pinned['repo_sha']}  tree={pinned['tree_sha256'][:12]}")
    idx = build_index_tarball(args, pf["repogrammar_bin"], pinned["repo_dir"], work)
    print(f"index tarball sha256={idx['sha256'][:12]}")

    # repos.lock.json (committed metadata; never repo source)
    lock = {
        "schema_version": "product-eval-pinned-repos.v1",
        "generated_at": now_rfc3339(),
        "tree_hash_algorithm": "fixture_version sha256 (see treehash.py)",
        "repos": [{
            "repo_id": "py-app-template", "lane": "agent_study", "split": "pilot_burned",
            "url": args.repo_url, "pinned_commit": pinned["repo_sha"],
            "tree_sha256": pinned["tree_sha256"],
            "repogrammar_index_tar_sha256": idx["sha256"],
        }],
    }
    lock_path = os.path.join(HERE, "repos.lock.json")
    with open(lock_path, "w", encoding="utf-8") as fh:
        json.dump(lock, fh, indent=2, sort_keys=True)
        fh.write("\n")
    print(f"wrote {lock_path}")

    t1 = load_task(os.path.join(HERE, "tasks", "t1_item_summary.json"))
    t6 = load_task(os.path.join(HERE, "tasks", "t6_db_session.json"))
    plan = build_plan([t1, t6], DEFAULT_ARMS, DEFAULT_REPS, SHUFFLE_SEED)

    if args.limit is not None:
        plan = plan[: args.limit]
        print(f"(limit) running only the first {len(plan)} planned run(s)")

    cumulative = 0.0
    infra_fail_classes: Dict[str, int] = {}
    rows: List[Dict[str, Any]] = []

    for i, spec in enumerate(plan):
        remaining = args.abort_at - cumulative
        if remaining <= 0.05:
            print(f"ABORT: cumulative cost ${cumulative:.4f} reached safety threshold ${args.abort_at:.2f}")
            break
        per_run_cap = round(min(DEFAULT_PER_RUN_CAP, max(0.05, remaining)), 4)

        run_dir = os.path.join(work, f"run-{i:02d}-{spec.run_id[:8]}")
        os.makedirs(run_dir, exist_ok=True)
        worktree = make_run_worktree(pinned["repo_dir"], idx["tar"], run_dir)
        wt_hash = tree_sha256(worktree)
        home_dir = os.path.join(run_dir, "home")
        mcp_path, mcp_sha = render_mcp_config(spec.arm, pf["repogrammar_bin"], worktree, run_dir)
        settings = os.path.join(HERE, "configs", "settings-study.json")
        prompt = spec.task["prompt"]
        transcript = os.path.join(run_dir, "transcript.jsonl")
        stderr_path = os.path.join(run_dir, "stderr.log")

        git_init_snapshot(worktree)
        started = now_rfc3339()
        status = run_claude(prompt, worktree, home_dir, mcp_path, settings, PILOT_MODEL,
                            per_run_cap, args.timeout_s, pf["claude_bin"], transcript, stderr_path)
        finished = now_rfc3339()

        parsed = parsers.parse_transcript(transcript)
        diff_info = git_diff_info(worktree) if spec.task_type == "T1" else None
        changed = diff_info["changed_files"] if diff_info else None
        added_text = diff_info["added_text"] if diff_info else None
        safety, grading = grade_and_detect(
            spec, parsed, worktree if spec.task_type == "T1" else None, changed, added_text)
        outcome = classify_outcome(status, grading)

        rec = assemble_record(
            spec, parsed, safety, grading,
            repo_sha=pinned["repo_sha"], worktree_sha256=wt_hash, mcp_config_sha256=mcp_sha,
            prompt_sha256=sha256_text(prompt), cli_version=pf["cli_version"], model=PILOT_MODEL,
            outcome=outcome, started_at=started, finished_at=finished, diff_info=diff_info,
            transcript_basename=os.path.basename(transcript),
            notes=None if status == "ok" else f"run_status={status}",
        )
        record_schema.append_record(ledger, rec)

        cost = parsed["tokens"].get("total_cost_usd") or 0.0
        cumulative += cost
        rows.append({"i": i, "task": spec.task_type, "arm": spec.arm, "rep": spec.rep,
                     "outcome": outcome, "cost": cost, "cum": cumulative,
                     "mcp": parsed["mcp"]["calls"], "reads": parsed["context"]["files_read"],
                     "status": status, "safety": safety})
        print(f"  run {i:02d} {spec.task_type}/{spec.arm}/r{spec.rep}: {outcome} "
              f"status={status} cost=${cost:.4f} cum=${cumulative:.4f} mcp={parsed['mcp']['calls']}")

        if status in ("infra", "timeout"):
            infra_fail_classes[status] = infra_fail_classes.get(status, 0) + 1
            if infra_fail_classes[status] >= 2:
                print(f"STOP: infrastructure failure class '{status}' repeated twice; "
                      f"halting to protect budget (per brief).")
                break

    # ---- summary ----
    print("\n=== PILOT SUMMARY ===")
    print(f"{'#':2} {'task':4} {'arm':3} {'rep':3} {'outcome':14} {'cost':8} {'cum':8} {'mcp':3} safety")
    for r in rows:
        print(f"{r['i']:<2} {r['task']:4} {r['arm']:3} {r['rep']:<3} {r['outcome']:14} "
              f"${r['cost']:<7.4f} ${r['cum']:<7.4f} {r['mcp']:<3} {r['safety']}")
    print(f"\ncumulative host-reported cost: ${cumulative:.4f} (cap ${args.abort_at:.2f} / authorized $30)")
    # arm-isolation report (verified from the committed records, not asserted by
    # construction): per task, one distinct worktree_sha256; A0 vs A3 differ only
    # in mcp_config_sha256.
    print("\nArm isolation (worktree_sha256 identical per task across arms/reps):")
    with open(ledger) as fh:
        wrote = [json.loads(ln) for ln in fh]
    per_task: Dict[str, set] = {}
    mcp_arms: Dict[str, set] = {}
    for rec in wrote:
        per_task.setdefault(rec["task_id"], set()).add(rec["worktree_sha256"])
        mcp_arms.setdefault(rec["arm"], set()).add(rec["mcp_config_sha256"])
    for tid, wts in per_task.items():
        ok = "OK" if len(wts) == 1 else "MISMATCH"
        print(f"  {tid}: {sorted(wts)[0][:16]} ({ok}, {len(wts)} distinct)")
    a0, a3 = mcp_arms.get("A0", set()), mcp_arms.get("A3", set())
    print(f"  mcp_config A0 vs A3 differ: {bool(a0) and a0 != a3}")
    print(f"\nledger: {ledger}")
    return 0


def main(argv: Optional[List[str]] = None) -> int:
    ap = argparse.ArgumentParser(description="RQ5 agent-study pilot harness")
    ap.add_argument("--dry-run", action="store_true", help="zero-spend pipeline exercise")
    ap.add_argument("--work-base", default=os.path.join(tempfile.gettempdir(), "repogrammar-agent-study"),
                    help="base dir for clones/worktrees/transcripts (MUST be outside the repo tree)")
    ap.add_argument("--repo-url", default=DEFAULT_REPO_URL)
    ap.add_argument("--repo-sha", default=None, help="commit SHA to pin (default: clone HEAD)")
    ap.add_argument("--reps", type=int, default=DEFAULT_REPS)
    ap.add_argument("--abort-at", type=float, default=DEFAULT_ABORT_AT)
    ap.add_argument("--timeout-s", type=int, default=DEFAULT_TIMEOUT_S)
    ap.add_argument("--claude-bin", default="claude")
    ap.add_argument("--repogrammar-bin",
                    default=os.path.abspath(os.path.join(HERE, "..", "..", "..", "target", "debug", "repogrammar")))
    ap.add_argument("--ledger", default=None, help="records ledger path (default: docs/experiments/data/)")
    ap.add_argument("--limit", type=int, default=None, help="run only the first N planned runs (de-risk probe)")
    args = ap.parse_args(argv)

    assert_work_base_outside_repo(args.work_base)  # S6: never place transcripts/worktrees in-repo
    if args.dry_run:
        return run_dry(args)
    return run_real(args)


if __name__ == "__main__":
    raise SystemExit(main())
