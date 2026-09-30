"""Offline preparation for the conditional-instruction A0/A1/A2 confirmation.

Starts no agent, subprocess, index, network request or credential lookup.
Raw workspaces stay outside the repository; preparation is not adoption evidence.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import random
import shutil
import tempfile

from treehash import tree_sha256

ROOT = Path(__file__).resolve().parents[3]
SEED = 20260930
ARMS = ("A0", "A1", "A2")
TASKS = (
    {"task_id": "route_convention", "eligible": True,
     "prompt": "Before adding another route, identify this repository's route registration convention. Return only JSON with file and decorator keys.",
     "oracle": {"file": "routes.py", "decorator": "router.get"}},
    {"task_id": "fixture_convention", "eligible": True,
     "prompt": "Before adding tests that need an API client, identify the existing fixture's registered name and definition file. Return only JSON with file and name keys.",
     "oracle": {"file": "tests/conftest.py", "name": "api_client"}},
    {"task_id": "documentation_only", "eligible": False,
     "prompt": "Edit only README.md: replace 'This fixture shows repository conventions.' with 'This fixture demonstrates repository conventions.' Do not change code.",
     "oracle": {"file": "README.md", "text": "This fixture demonstrates repository conventions.\n"}},
    {"task_id": "exact_lookup", "eligible": False,
     "prompt": "Read routes.py and return only JSON with a name key: the function decorated with the GET route /users. This is an exact lookup.",
     "oracle": {"name": "list_users"}},
)


def prepare(out: Path, instruction_file: Path | None = None) -> dict:
    out = out.resolve()
    if out == ROOT or ROOT in out.parents:
        raise ValueError("workspaces must be outside the repository")
    if out.exists() and any(out.iterdir()):
        raise ValueError("refusing a nonempty output directory")
    if instruction_file is None:
        raise ValueError("the coordinator's canonical global instruction artifact is required")
    if instruction_file.is_symlink() or not instruction_file.is_file():
        raise ValueError("instruction input must be a regular file")
    with instruction_file.open("rb") as stream:
        guide = stream.read(1_025)
    if not 0 < len(guide) <= 1_024 or b"MANAGED CONTENT VERSION: 4" not in guide:
        raise ValueError("supply the reviewed short global v4 profile")
    out.mkdir(parents=True, exist_ok=True)
    fixture = ROOT / "src/fixtures/python/release/v0_1"
    cells = [(task, arm) for task in TASKS for arm in ARMS]
    random.Random(SEED).shuffle(cells)
    rows = []
    for ordinal, (task, arm) in enumerate(cells):
        cell = out / f"{ordinal:02d}-{task['task_id']}-{arm}"
        worktree = cell / "worktree"
        (worktree / "tests").mkdir(parents=True)
        for source, destination in (
            ("positive-strong-evidence/routes.py", "routes.py"),
            ("pytest-fixture-alias-strong-evidence/conftest.py", "tests/conftest.py"),
            ("pytest-fixture-alias-strong-evidence/test_fixture_names.py", "tests/test_fixture_names.py"),
        ):
            shutil.copyfile(fixture / source, worktree / destination)
        (worktree / "README.md").write_text("This fixture shows repository conventions.\n", encoding="utf-8")
        if arm == "A2" and guide is not None:
            profile = cell / "home/.claude"
            profile.mkdir(parents=True)
            (profile / "CLAUDE.md").write_bytes(guide)
        rows.append({"ordinal": ordinal, "task_id": task["task_id"], "arm": arm,
                     "eligible": task["eligible"], "worktree_sha256": tree_sha256(str(worktree)),
                     "task_sha256": hashlib.sha256(json.dumps(task, sort_keys=True).encode()).hexdigest(),
                     "instruction_sha256": hashlib.sha256(guide).hexdigest() if arm == "A2" and guide else None,
                     "status": "NOT_MEASURED", "mcp_calls": None, "source_bytes_read": None,
                     "task_success": None, "host_tokens": None, "host_cost_usd": None})
    plan = {"schema_version": "agent-adoption-plan.v1", "seed": SEED, "repetitions": 1,
            "tasks": TASKS, "runs": rows, "measurement_status": "NOT_MEASURED",
            "execution_blockers": ["agent_budget_and_auth_isolation_not_authorized", "identical_index_not_prepared"]}
    (out / "adoption-plan.json").write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return plan


def selftest() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        # Shape-only fixture, never treated as a canonical installed profile or
        # an observed agent result. Real preparation requires coordinator review.
        guide = Path(tmp) / "guide.md"
        guide.write_text("<!-- REPOGRAMMAR MANAGED CONTENT VERSION: 4 -->\nselfcheck\n", encoding="utf-8")
        plan = prepare(Path(tmp) / "plan", guide)
        repeated = prepare(Path(tmp) / "repeated", guide)
        assert plan == repeated
        assert len(plan["runs"]) == 12
        assert len({row["worktree_sha256"] for row in plan["runs"]}) == 1
        assert {row["arm"] for row in plan["runs"]} == set(ARMS)
        assert all(row["mcp_calls"] is None for row in plan["runs"])
        try:
            prepare(Path(tmp) / "plan", guide)
        except ValueError:
            pass
        else:
            raise AssertionError("must preserve existing workspaces")
        try:
            prepare(ROOT / "adoption-forbidden", guide)
        except ValueError:
            pass
        else:
            raise AssertionError("must refuse repository-local workspaces")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--instruction-file", type=Path)
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()
    if args.selftest:
        selftest()
        print("PASS: offline preparation; adoption NOT_MEASURED")
    elif args.out:
        if args.instruction_file is None:
            parser.error("--out requires the coordinator-reviewed --instruction-file")
        prepare(args.out, args.instruction_file)
        print("Prepared 12 cells; adoption NOT_MEASURED; no agent was launched")
    else:
        parser.error("choose --selftest or --out")
