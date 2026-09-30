"""Offline B0-B3 freeze and OS-tool-boundary qualification. No agent launcher.

Native CLI discovery, credential isolation, hidden host tools and MCP IPC still
require a separately authorized pinned-host qualification. This is not a live
agent-adoption-run.v2 validator or a sandbox for arbitrary installed agents.
"""
import argparse
from contextlib import closing
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import random
import re
import sqlite3
import stat
import subprocess
import sys

from oracles_v2 import frozen_tasks, qualify as qualify_oracles
from treehash import tree_sha256

ROOT = Path(__file__).resolve().parents[3]
ARMS = ("B0", "B1", "B2", "B3")
SEED = 20260930
PREFIX = "Use the available RepoGrammar MCP for this task before broad source acquisition.\n"
PROBE = Path(__file__).with_name("isolation_probe_v2.py")
PYTHON = Path(sys.executable).resolve()
FRAMEWORK_PYTHON = Path(sys.base_prefix) / "Resources/Python.app/Contents/MacOS/Python"
if FRAMEWORK_PYTHON.is_file():
    PYTHON = FRAMEWORK_PYTHON.resolve()
CHECKS = {"source_read", "source_glob", "source_write", "source_shell", "source_sqlite", "read", "grep", "glob",
          "recursive_glob", "shell", "sqlite", "symlink", "hardlink", "rename", "product_read",
          "product_exec", "oracle", "other_profile", "global_path",
          "environment", "network_bind", "network_connect"}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def regular_bytes(path, limit):
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), "rb") as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_size > limit:
            raise ValueError("input must be a bounded single-link regular file")
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise ValueError("input exceeds bound")
    return data


def new_output(out):
    if out.is_symlink():
        raise ValueError("output symlink is forbidden")
    out = out.resolve()
    if out == ROOT or ROOT in out.parents or out in ROOT.parents:
        raise ValueError("output must be outside the repository and its ancestors")
    out.mkdir(mode=0o700, parents=True, exist_ok=False)
    return out


def materialize(worktree, files):
    worktree.mkdir(parents=True)
    for name, text in files.items():
        path = PurePosixPath(name)
        if (path.is_absolute() or not path.parts or any(p in (".", "..") or p.startswith(".")
                for p in path.parts) or "\\" in name or ":" in name):
            raise ValueError("unsafe seed path")
        target = worktree / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding="utf-8")


def admit_tree(worktree):
    for path in (worktree, *worktree.rglob("*")):
        info = path.lstat()
        if (stat.S_ISLNK(info.st_mode) or not (stat.S_ISREG(info.st_mode) or stat.S_ISDIR(info.st_mode))
                or (stat.S_ISREG(info.st_mode) and info.st_nlink != 1)):
            raise ValueError("source tree contains an alias or special file")


def environment(cell):
    home = cell / "home"
    env = {"HOME": str(home), "CODEX_HOME": str(home / ".codex"),
           "CLAUDE_CONFIG_DIR": str(home / ".claude"),
           "XDG_CONFIG_HOME": str(home / ".config"), "XDG_CACHE_HOME": str(home / ".cache"),
           "XDG_DATA_HOME": str(home / ".local/share"), "XDG_STATE_HOME": str(home / ".local/state"),
           "TMPDIR": str(cell / "tmp"), "PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"}
    for name, value in env.items():
        if name not in ("PATH", "LANG", "LC_ALL"):
            Path(value).mkdir(parents=True, exist_ok=True)
    return env


def sandbox_profile(worktree, env, probe):
    # ponytail: macOS tool-process sandbox only; qualify another backend/host before live use.
    quote = json.dumps
    runtime = ["/System", "/usr/lib", "/usr/share", "/usr/bin", "/bin", sys.base_prefix]
    ancestors = sorted({str(p) for root in (PYTHON, Path(sys.base_prefix))
                        for p in root.parents})
    metadata = " ".join(f"(literal {quote(p)})" for p in ancestors)
    execution = " ".join(f"(literal {quote(p)})" for p in (str(PYTHON), "/bin/cat"))
    reads = " ".join(f"(subpath {quote(p)})" for p in runtime + [str(worktree), env["HOME"]])
    writes = " ".join(f"(subpath {quote(p)})" for p in [str(worktree), env["TMPDIR"]])
    return ("(version 1)\n(deny default)\n(allow process-fork)\n"
            "(allow sysctl-read)\n"
            f"(allow file-read-metadata {metadata})\n"
            f"(allow process-exec {execution})\n"
            f"(allow file-read* {reads} (literal \"/\") (literal {quote(str(PYTHON))}) (literal {quote(str(PYTHON.parent))}) (literal {quote(str(probe))}) (subpath {quote(env['TMPDIR'])}))\n"
            f"(allow file-write* {writes})\n"
            "(allow file-read* file-write* (literal \"/dev/null\") (literal \"/dev/urandom\"))\n"
            f"(deny file-read* file-write* (subpath {quote(str(worktree / '.repogrammar'))}))\n")


def native_control(cell, arm, guide, product):
    worktree = cell / "worktree"
    materialize(worktree, {"allowed.txt": "allowed\n"})
    env = environment(cell)
    if arm == "B2":
        (Path(env["CLAUDE_CONFIG_DIR"]) / "CLAUDE.md").write_bytes(guide)
    hidden = cell / "hidden"
    hidden.mkdir()
    oracle = hidden / "oracle.json"
    oracle.write_text("hidden oracle\n", encoding="utf-8")
    other = hidden / "other-profile.md"
    other.write_text("other arm\n", encoding="utf-8")
    index = worktree / ".repogrammar/index.sqlite"
    index.parent.mkdir()
    with closing(sqlite3.connect(index)) as db, db:
        db.execute("create table sentinel(value text)")
        db.execute("insert into sentinel values ('secret')")
        if db.execute("select value from sentinel").fetchone() != ("secret",):
            raise ValueError("harness-owned index access failed")
    admit_tree(worktree)
    index_hash = sha(index.read_bytes())
    (worktree / "index-alias").symlink_to(index)
    probe = cell / "probe.py"
    probe.write_bytes(PROBE.read_bytes())
    profile = cell / "sandbox.sb"
    profile.write_text(sandbox_profile(worktree, env, probe), encoding="utf-8")
    row = {"arm": arm, "status": "SANDBOX_UNAVAILABLE", "passed": 0,
           "required": len(CHECKS), "index_sha256": index_hash,
           "sandbox_sha256": sha(profile.read_bytes()), "harness_index_access": "PASS"}
    if sys.platform != "darwin" or not Path("/usr/bin/sandbox-exec").is_file():
        return row
    command = ["/usr/bin/sandbox-exec", "-f", str(profile), str(PYTHON), "-I", "-S", str(probe),
               str(worktree), str(index), str(product), str(oracle), str(other), sha(guide) if arm == "B2" else ""]
    try:
        proc = subprocess.run(command, cwd=worktree, env=env, capture_output=True, timeout=20)
    except (OSError, subprocess.TimeoutExpired):
        row["status"] = "SANDBOX_LAUNCH_FAILED"
        return row
    (cell / "probe.stdout").write_bytes(proc.stdout)
    (cell / "probe.stderr").write_bytes(proc.stderr)
    row["stdout_sha256"], row["stderr_sha256"] = sha(proc.stdout), sha(proc.stderr)
    if proc.returncode != 0:
        row["status"] = "SANDBOX_LAUNCH_FAILED"
        return row
    try:
        checks = json.loads(proc.stdout)
    except (ValueError, UnicodeError):
        row["status"] = "CONTROL_LEAK"
        return row
    row["passed"] = sum(value is True for value in checks.values()) if isinstance(checks, dict) else 0
    row["status"] = ("PASS" if isinstance(checks, dict) and set(checks) == CHECKS
                     and all(value is True for value in checks.values()) else "CONTROL_LEAK")
    try:
        unchanged = sha(index.read_bytes()) == index_hash
    except OSError:
        unchanged = False
    if not unchanged:
        row["status"] = "CONTROL_LEAK"
    return row


def qualify(out, instruction_file, expected_guide_sha256, product):
    if not re.fullmatch(r"[0-9a-f]{64}", expected_guide_sha256):
        raise ValueError("expected guide hash must be SHA-256")
    guide = regular_bytes(instruction_file, 1024)
    if (sha(guide) != expected_guide_sha256 or not guide.startswith(b"<!-- BEGIN REPOGRAMMAR MANAGED SECTION -->\n")
            or b"MANAGED CONTENT VERSION: 4" not in guide
            or not guide.rstrip(b"\n").endswith(b"<!-- END REPOGRAMMAR MANAGED SECTION -->")):
        raise ValueError("guide does not match the coordinator-pinned v4 artifact")
    product = product.resolve(strict=True)
    product_hash = sha(regular_bytes(product, 256 * 1024 * 1024))
    out = new_output(out)
    oracles = qualify_oracles(out / "private-oracles")
    tasks = frozen_tasks()
    generator = random.Random(SEED)
    blocks = [(task, repetition) for task in tasks for repetition in range(1, 6)]
    generator.shuffle(blocks)
    rows = []
    for task, repetition in blocks:
        arms = list(ARMS)
        generator.shuffle(arms)
        for arm in arms:
            ordinal = len(rows)
            cell = out / "cells" / str(ordinal)
            worktree = cell / "worktree"
            materialize(worktree, task["seed"])
            (worktree / ".repogrammar").mkdir()
            admit_tree(worktree)
            env = environment(cell)
            if arm == "B2":
                (Path(env["CLAUDE_CONFIG_DIR"]) / "CLAUDE.md").write_bytes(guide)
            prompt = task["prompt"]
            delivered = PREFIX + prompt if arm == "B3" else prompt
            (cell / "prompt.txt").write_text(delivered, encoding="utf-8")
            rows.append({"ordinal": ordinal, "task_id": task["task_id"], "arm": arm,
                         "repetition": repetition, "eligible": task["eligible"],
                         "source_sha256": tree_sha256(str(worktree)), "base_prompt_sha256": sha(prompt.encode()),
                         "delivered_prompt_sha256": sha(delivered.encode()),
                         "guide_sha256": sha(guide) if arm == "B2" else None,
                         "mcp": "ABSENT" if arm == "B0" else "PLANNED_PINNED",
                         "index": "NOT_PREPARED",
                         "status": "NOT_MEASURED", "task_success": None, "cost_usd": None})
    # Freeze order/prompts/oracles before any adversarial control execution.
    plan = {"schema_version": "agent-adoption-offline-plan.v2", "seed": SEED,
            "split": "DEVELOPMENT_BURNED", "repetitions": 5, "runs": rows,
            "product_sha256": product_hash, "guide_sha256": sha(guide),
            "oracles": oracles, "live_execution": "CONTROL_ISOLATION_BLOCKED"}
    (out / "plan.json").write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    controls = [native_control(out / "controls" / arm, arm, guide, product) for arm in ARMS]
    summary = {"schema_version": "agent-adoption-offline-qualification.v2",
               "status": "PASS" if all(c["status"] == "PASS" for c in controls) else "CONTROL_ISOLATION_BLOCKED",
               "scope": "MACOS_TOOL_PROCESS", "split": "DEVELOPMENT_BURNED",
               "plan_sha256": sha((out / "plan.json").read_bytes()), "controls": controls,
               "guide_sha256": sha(guide), "product_sha256": product_hash,
               "runner_sha256": sha(Path(__file__).read_bytes()), "probe_sha256": sha(PROBE.read_bytes()),
               "python_sha256": sha(PYTHON.read_bytes()), "cells": len(rows),
               "sandbox_executable_sha256": sha(Path('/usr/bin/sandbox-exec').read_bytes()) if sys.platform == 'darwin' else None,
               "oracles": oracles, "live_execution": "CONTROL_ISOLATION_BLOCKED",
               "global_file_discovery": "NOT_MEASURED", "adoption": "NOT_MEASURED",
               "blockers": ["PINNED_AGENT_HOST", "MCP_IPC", "ACTUAL_GLOBAL_DISCOVERY", "HELD_OUT_FREEZE", "BUDGET_AUTH"]}
    (out / "qualification.summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return summary


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--instruction-file", type=Path, required=True)
    parser.add_argument("--expected-guide-sha256", required=True)
    parser.add_argument("--product-bin", type=Path, required=True)
    args = parser.parse_args()
    result = qualify(args.out, args.instruction_file, args.expected_guide_sha256, args.product_bin)
    print(result["status"] + "; live execution CONTROL_ISOLATION_BLOCKED; adoption NOT_MEASURED")
    raise SystemExit(0 if result["status"] == "PASS" else 1)
