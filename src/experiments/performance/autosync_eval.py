"""Isolated autosync resource/correctness experiment; no battery inference.

Each native measurement includes foreground startup, the named observation
window and owned shutdown. Initialization and the Python driver are outside
the measured child process. Raw diagnostics stay in the disposable workspace.
"""

import argparse
from contextlib import closing
import hashlib
import json
import math
import os
import platform
from pathlib import Path
import re
import shutil
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time


MAX_BYTES = 1024 * 1024
SCENARIOS = ("idle_60s", "ignored_burst", "same_size_same_mtime", "supported_burst")
SOURCE = b'from fastapi import APIRouter\nrouter = APIRouter()\n\n@router.get("/one")\ndef one():\n    return {"ok": 1}\n\n@router.get("/two")\ndef two():\n    return {"ok": 1}\n\n@router.get("/three")\ndef three():\n    return {"ok": 1}\n'


class HarnessError(Exception):
    """Only stable error codes reach the source-free report."""


def unavailable(reason, state="NOT_MEASURED"):
    return {"state": state, "reason": reason}


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(65536), b""):
            digest.update(block)
    return digest.hexdigest()


def bounded_read(path, ceiling=MAX_BYTES):
    with Path(path).open("rb") as stream:
        value = stream.read(ceiling + 1)
    if len(value) > ceiling:
        raise HarnessError("diagnostic_output_oversized")
    return value


def native_resources(text, platform):
    if platform not in ("darwin", "linux"):
        return unavailable("native_resource_platform_unsupported")
    user = system = wall = rss = None
    if platform == "darwin":
        match = re.search(r"([\d.+eE-]+)\s+real\s+([\d.+eE-]+)\s+user\s+([\d.+eE-]+)\s+sys", text)
        peak = re.search(r"^\s*(\d+)\s+maximum resident set size", text, re.MULTILINE)
        if match:
            wall, user, system = map(float, match.groups())
        if peak:
            rss = int(peak.group(1))
    else:
        def scalar(label):
            match = re.search(r"^\s*" + re.escape(label) + r"\s*([^\n]+)$", text, re.MULTILINE)
            return match.group(1).strip() if match else None
        raw_user = scalar("User time (seconds):")
        raw_system = scalar("System time (seconds):")
        raw_wall = scalar("Elapsed (wall clock) time (h:mm:ss or m:ss):")
        raw_rss = scalar("Maximum resident set size (kbytes):")
        try:
            user, system = float(raw_user), float(raw_system)
            parts = [float(part) for part in raw_wall.split(":")]
            if len(parts) not in (2, 3):
                raise ValueError
            wall = 0.0
            for part in parts:
                wall = wall * 60 + part
            rss = int(raw_rss) * 1024
        except (TypeError, ValueError, AttributeError):
            return unavailable("native_resource_output_invalid")
    if any(value is None or not math.isfinite(value) or value < 0 for value in (wall, user, system)) or rss is None or rss < 0:
        return unavailable("native_resource_output_incomplete")
    return {"state": "MEASURED", "wall_seconds": wall, "user_cpu_seconds": user,
            "system_cpu_seconds": system, "peak_rss_bytes": rss,
            "rss_scope": "native child rusage maximum, not simultaneous process-tree RSS sum"}


def wait_until(predicate, seconds, clock=time.monotonic, sleep=time.sleep):
    deadline = clock() + seconds
    while True:
        value = predicate()
        if value:
            return value
        remaining = deadline - clock()
        if remaining <= 0:
            raise HarnessError("observation_deadline_exceeded")
        sleep(min(0.1, remaining))


def owned_lock_pid(lock, group, parent_alive, getpgid=os.getpgid):
    pid = lock.get("pid")
    if not parent_alive or lock.get("kind") != "autosync_daemon" or lock.get("phase") != "ready" or type(pid) is not int or pid <= 0:
        raise HarnessError("daemon_ownership_unverified")
    try:
        if getpgid(pid) != group:
            raise HarnessError("daemon_ownership_unverified")
    except ProcessLookupError:
        raise HarnessError("daemon_exited_before_observation") from None
    return pid


def state_snapshot(repo):
    database = repo / ".repogrammar/repogrammar.sqlite"
    with closing(sqlite3.connect(database.as_uri() + "?mode=ro", uri=True, timeout=0.25)) as connection:
        connection.execute("BEGIN")
        generation = connection.execute("SELECT generation_id FROM index_generations WHERE status = 'active'").fetchall()
        if len(generation) != 1:
            raise HarnessError("active_generation_unavailable")
        generation = generation[0][0]
        count = connection.execute("SELECT COUNT(*) FROM index_generations WHERE status IN ('active', 'validated')").fetchone()[0]
        hashes = dict(connection.execute("SELECT path, content_hash FROM indexed_files WHERE generation_id = ?", (generation,)))
    return {"generation": generation, "generation_count": count, "hashes": hashes}


def run_json(command, environment, repo, directory, stage):
    with (directory / (stage + ".stdout")).open("wb") as stdout, (directory / (stage + ".stderr")).open("wb") as stderr:
        process = subprocess.Popen(command, env=environment, cwd=repo, stdin=subprocess.DEVNULL,
                                   stdout=stdout, stderr=stderr, start_new_session=True)
        try:
            status = process.wait(timeout=30)
        except subprocess.TimeoutExpired:
            # The unreaped live Popen handle owns this newly created session.
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)
            raise HarnessError("product_step_timeout") from None
    if status:
        raise HarnessError("product_step_failed")
    try:
        return json.loads(bounded_read(directory / (stage + ".stdout")))
    except (ValueError, UnicodeError):
        raise HarnessError("product_step_json_invalid") from None


def isolated_environment(root, python, worker):
    home, tools = root / "home", root / "tools"
    home.mkdir()
    tools.mkdir()
    for name in ("git", "ps", "kill"):
        executable = shutil.which(name)
        if not executable:
            raise HarnessError("required_native_tool_unavailable")
        (tools / name).symlink_to(executable)
    (tools / "python3").symlink_to(python)
    return {"HOME": str(home), "USERPROFILE": str(home), "XDG_CONFIG_HOME": str(home / ".config"),
            "XDG_DATA_HOME": str(home / ".local/share"), "XDG_CACHE_HOME": str(home / ".cache"),
            "CODEX_HOME": str(home / ".codex"), "PATH": str(tools), "LC_ALL": "C",
            "REPOGRAMMAR_PYTHON_EXECUTABLE": str(python), "REPOGRAMMAR_PYTHON_WORKER": str(worker),
            "REPOGRAMMAR_TELEMETRY": "0", "DO_NOT_TRACK": "1",
            "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_SYSTEM": os.devnull}


def log_summary(repo):
    text = bounded_read(repo / ".repogrammar/logs/daemon.log").decode("utf-8")
    records = re.findall(r"autosync: (\w+) sync \+(\d+) ~(\d+) -(\d+) unchanged (\d+) copied (\d+) reparsed (\d+) file\(s\), (\d+) unit\(s\) in (\d+)ms", text)
    return {"syncs": len(records), "reparsed_files": sum(int(row[6]) for row in records),
            "copied_forward_files": sum(int(row[5]) for row in records),
            "sync_wall_ms": sum(int(row[8]) for row in records),
            "native_events": "native events enabled" in text,
            "polling_fallback": "using metadata polling" in text or "falling back to metadata polling" in text,
            "generations": sorted(set(re.findall(r"\(generation (gen-[0-9]{6})\)", text)))}


def observe_scenario(scenario, args):
    temporary = tempfile.TemporaryDirectory(prefix="repogrammar-autosync-performance-")
    root = Path(temporary.name)
    repo = root / "repo"
    repo.mkdir()
    environment = isolated_environment(root, args.python, args.worker)
    if args.project_session:
        environment["REPOGRAMMAR_PYTHON_PROJECT_SESSION"] = "1"
    (repo / "app.py").write_bytes(SOURCE)
    (repo / ".gitignore").write_text("scratch-output/\n", encoding="utf-8")
    git = subprocess.run([str(root / "tools/git"), "-c", "core.hooksPath=" + os.devnull, "init", "-q"],
                         cwd=repo, env=environment, capture_output=True, timeout=10)
    if git.returncode:
        raise HarnessError("isolated_git_init_failed")
    product = [str(args.binary)]
    project_args = ["--project", str(repo), "--json"]
    run_json(product + ["init"] + project_args + ["--yes", "--no-autosync", "--progress", "never"], environment, repo, root, "init")
    config = run_json(product + ["autosync", "enable"] + project_args, environment, repo, root, "enable")
    before = state_snapshot(repo)
    lock_path = repo / ".repogrammar/locks/daemon.lock"
    process = None
    lock_identity = None
    cleanup_verified = False
    row = {"scenario_id": scenario, "state": "FAIL", "configuration": {"poll_ms": config.get("poll_ms"), "debounce_ms": config.get("debounce_ms")},
           "worker_spawns": unavailable("native uninstrumented daemon does not expose subprocess counters"),
           "watcher_registration_count": unavailable("notify backend registration count is not exposed"),
           "battery_effect": unavailable("CPU observations do not measure battery use")}
    try:
        with (root / "daemon.stdout").open("wb") as stdout, (root / "daemon.stderr").open("wb") as stderr:
            process = subprocess.Popen(["/usr/bin/time", "-l" if sys.platform == "darwin" else "-v"] + product + ["autosync", "run", "--project", str(repo), "--quiet"],
                                       cwd=repo, env=environment, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr, start_new_session=True)
            started = time.monotonic()
            def ready():
                if process.poll() is not None:
                    raise HarnessError("daemon_exited_before_ready")
                if not lock_path.exists():
                    return False
                try:
                    lock = json.loads(bounded_read(lock_path, 4096))
                except (ValueError, UnicodeError):
                    return False
                if lock.get("phase") != "ready":
                    return False
                owned_lock_pid(lock, process.pid, process.poll() is None)
                return lock
            lock_identity = wait_until(ready, args.step_timeout)
            row["startup_wall_seconds"] = time.monotonic() - started
            time.sleep(1)
            window_started = time.monotonic()
            if scenario == "idle_60s":
                def idle_complete():
                    if process.poll() is not None:
                        raise HarnessError("daemon_exited_during_idle")
                    return time.monotonic() - window_started >= args.idle_seconds
                wait_until(idle_complete, args.idle_seconds + 1)
            elif scenario == "ignored_burst":
                ignored = repo / "scratch-output"
                ignored.mkdir()
                for index in range(args.burst_files):
                    (ignored / (str(index) + ".py")).write_bytes(SOURCE)
                time.sleep(args.settle_seconds)
            else:
                changed = {}
                if scenario == "same_size_same_mtime":
                    path = repo / "app.py"
                    stat = path.stat()
                    replacement = SOURCE.replace(b'"ok": 1', b'"ok": 2', 1)
                    path.write_bytes(replacement)
                    os.utime(path, ns=(stat.st_atime_ns, stat.st_mtime_ns))
                    row["same_size_preserved"] = path.stat().st_size == stat.st_size
                    row["same_mtime_preserved"] = path.stat().st_mtime_ns == stat.st_mtime_ns
                    changed["app.py"] = "sha256:" + hashlib.sha256(replacement).hexdigest()
                else:
                    for index in range(args.burst_files):
                        relative = "added_" + str(index) + ".py"
                        (repo / relative).write_bytes(SOURCE)
                        changed[relative] = "sha256:" + hashlib.sha256(SOURCE).hexdigest()
                def activated():
                    if process.poll() is not None:
                        raise HarnessError("daemon_exited_during_observation")
                    current = state_snapshot(repo)
                    return current if current["generation"] != before["generation"] and all(current["hashes"].get(path) == digest for path, digest in changed.items()) else False
                wait_until(activated, args.step_timeout)
                time.sleep(args.settle_seconds)
            row["observation_wall_seconds"] = time.monotonic() - window_started
            after = state_snapshot(repo)
            row["retained_generation_count"] = after["generation_count"]
            row["active_generation_changed"] = after["generation"] != before["generation"]
            summary = log_summary(repo)
            row["generation_activations"] = len(set(summary.pop("generations")) - {before["generation"]})
            row["daemon_work"] = summary
            expected_change = scenario in ("same_size_same_mtime", "supported_burst")
            row["correctness_pass"] = row["active_generation_changed"] == expected_change
            if scenario == "same_size_same_mtime":
                row["correctness_pass"] &= row["same_size_preserved"] and row["same_mtime_preserved"] and summary["native_events"] and not summary["polling_fallback"]
            current_lock = json.loads(bounded_read(lock_path, 4096))
            if current_lock != lock_identity:
                raise HarnessError("daemon_lock_identity_changed")
            owned_lock_pid(current_lock, process.pid, process.poll() is None)
            run_json(product + ["autosync", "stop"] + project_args, environment, repo, root, "stop")
            process.wait(timeout=args.step_timeout)
            cleanup_verified = not lock_path.exists()
            row["cleanup_verified"] = cleanup_verified
            row["native_exit_code"] = process.returncode
        row["resources"] = native_resources(bounded_read(root / "daemon.stderr").decode("utf-8"), sys.platform)
        row["resource_scope"] = "foreground daemon startup + 1-second settling + named observation + owned shutdown; initialization and driver CPU excluded"
        row["state"] = "PASS" if row["correctness_pass"] and cleanup_verified and row["resources"]["state"] == "MEASURED" else "FAIL"
    except (HarnessError, subprocess.TimeoutExpired, OSError, sqlite3.Error) as error:
        row["reason"] = str(error) if isinstance(error, HarnessError) else "native_or_state_operation_failed"
    finally:
        if process is not None and process.poll() is None:
            # Never signal a PID read from an unverified lock. The unreaped Popen
            # handle owns the new process group created by this harness.
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)
        if process is None or (process.poll() is not None and not lock_path.exists()):
            temporary.cleanup()
        else:
            # Preserve unexpected ownership/state for review; do not delete it.
            temporary._finalizer.detach()
            row["workspace_retained"] = True
            print("autosync experiment workspace retained: " + str(root), file=sys.stderr)
    return row


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", dest="binary", required=True, type=Path)
    parser.add_argument("--worker", required=True, type=Path)
    parser.add_argument("--python", type=Path, default=Path(sys.executable))
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--condition", default="baseline")
    parser.add_argument("--project-session", action="store_true")
    parser.add_argument("--scenario", choices=SCENARIOS, action="append")
    parser.add_argument("--idle-seconds", type=float, default=60)
    parser.add_argument("--settle-seconds", type=float, default=3)
    parser.add_argument("--step-timeout", type=float, default=20)
    parser.add_argument("--burst-files", type=int, default=20)
    args = parser.parse_args()
    for name in ("binary", "worker", "python"):
        path = getattr(args, name).resolve(strict=True)
        if not path.is_file():
            parser.error("binary/worker/Python must be regular files")
        setattr(args, name, path)
    if not 1 <= args.idle_seconds <= 120 or not 1 <= args.settle_seconds <= 10 or not 1 <= args.step_timeout <= 60 or not 1 <= args.burst_files <= 100:
        parser.error("observation/resource bounds exceeded")
    if not re.fullmatch(r"[a-z0-9_-]{1,40}", args.condition):
        parser.error("condition must be a bounded token")
    return args


def main():
    args = arguments()
    pins = {"binary_sha256": sha256(args.binary), "worker_sha256": sha256(args.worker), "harness_sha256": sha256(__file__)}
    report = {"schema_version": "autosync-performance.v1", "condition": args.condition,
              "platform": {"os": sys.platform, "arch": platform.machine()}, **pins,
              "idle_seconds": args.idle_seconds, "burst_files": args.burst_files,
              "python_project_session_enabled": args.project_session,
              "linux_validation": unavailable("this run is not native Linux", "NOT_RUN") if sys.platform != "linux" else {"state": "RUN"},
              "rows": []}
    if sys.platform not in ("darwin", "linux"):
        report["state"] = "NOT_RUN"
        report["reason"] = "declared_native_platform_unavailable"
    else:
        for scenario in args.scenario or SCENARIOS:
            try:
                row = observe_scenario(scenario, args)
            except (HarnessError, subprocess.TimeoutExpired, OSError, sqlite3.Error) as error:
                row = {"scenario_id": scenario, "state": "FAIL", "reason": str(error) if isinstance(error, HarnessError) else "isolated_preparation_failed"}
            report["rows"].append(row)
        unchanged = pins == {"binary_sha256": sha256(args.binary), "worker_sha256": sha256(args.worker), "harness_sha256": sha256(__file__)}
        report["assets_unchanged"] = unchanged
        report["state"] = "PASS" if unchanged and all(row["state"] == "PASS" for row in report["rows"]) else "FAIL"
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "autosync-performance.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print("autosync-performance: " + report["state"])
    return 0 if report["state"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
