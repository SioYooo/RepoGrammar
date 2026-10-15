#!/usr/bin/env python3
"""Offline quickstart contracts against a real candidate binary and worker."""

import argparse
import fcntl
import hashlib
import io
import json
import os
from pathlib import Path
import pty
import select
import shutil
import subprocess
import sys
import tarfile
import tempfile
import termios
import time
import unittest


ROOT = Path(__file__).resolve().parents[2]
INSTALLER = ROOT / "src/install/repogrammar-install.sh"
WORKER = ROOT / "src/workers/python/worker.py"
FIXTURE = ROOT / "src/fixtures/python/release/v0_1/pydantic-basic/schemas.py"
TOOLS = ("bash sh env cat cp chmod mkdir rm dirname basename uname tar gzip awk grep sed tr "
         "readlink getconf ldd git id date mv cmp ln which head stat sleep ps kill mktemp "
         "sha256sum shasum wc sort touch").split()
ARGS = None
RESULTS = []


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def snapshot(root):
    result = {}
    for path in sorted(root.rglob("*")):
        name = str(path.relative_to(root))
        if path.is_symlink():
            result[name] = ("link", os.readlink(path))
        elif path.is_file():
            result[name] = ("file", digest(path))
        else:
            result[name] = ("directory",)
    return result


def release(directory, binary, target, *, mutation=None):
    directory.mkdir(parents=True)
    archive = directory / f"repogrammar-{target}.tar.gz"
    with tarfile.open(archive, "w:gz", format=tarfile.USTAR_FORMAT) as bundle:
        for name, source in [("repogrammar", binary), ("workers/python/worker.py", WORKER)]:
            if mutation == "missing-worker" and name.endswith("worker.py"):
                continue
            info = tarfile.TarInfo(name)
            info.mode = 0o755 if name == "repogrammar" else 0o644
            data = source.read_bytes()
            if mutation == "broken-worker" and name.endswith("worker.py"):
                data = b"raise RuntimeError('injected worker failure')\n"
            if mutation == "symlink" and name == "repogrammar":
                info.type, info.linkname = tarfile.SYMTYPE, "../outside"
                bundle.addfile(info)
            elif mutation == "hardlink" and name == "repogrammar":
                info.type, info.linkname = tarfile.LNKTYPE, "../outside"
                bundle.addfile(info)
            else:
                info.size = len(data)
                bundle.addfile(info, io.BytesIO(data))
                if mutation == "duplicate" and name == "repogrammar":
                    bundle.addfile(info, io.BytesIO(data))
        if mutation in ("unexpected", "traversal"):
            info = tarfile.TarInfo("unexpected.txt" if mutation == "unexpected" else "../outside")
            info.size = 6
            bundle.addfile(info, io.BytesIO(b"unsafe"))
    if mutation == "truncated":
        archive.write_bytes(archive.read_bytes()[:100])
    checksum = archive.with_name(archive.name + ".sha256")
    checksum.write_text(f"{digest(archive)}  {archive.name}\n")
    if mutation == "checksum":
        checksum.write_text(f"{'0' * 64}  {archive.name}\n")
    elif mutation == "malformed-checksum":
        checksum.write_text(f"{digest(archive)}  wrong-archive.tar.gz\n")
    elif mutation == "missing-checksum":
        checksum.unlink()
    return directory


FAKE_AGENT = r'''
import json, os, pathlib, sys
agent = pathlib.Path(sys.argv[0]).name
root = pathlib.Path(os.environ["HOME"]) / ".fake-agents"
root.mkdir(exist_ok=True)
state = root / (agent + ".json")
args = sys.argv[1:]
if args == ["--version"]:
    print(agent + " test-fixture")
    raise SystemExit(0)
with (root / "calls.jsonl").open("a") as log:
    log.write(json.dumps([agent] + args) + "\n")
if args[:2] == ["mcp", "get"]:
    if not state.exists():
        print("Error: No MCP server named 'repogrammar' found." if agent == "codex" else 'No MCP server named "repogrammar".')
        raise SystemExit(1)
    value = json.loads(state.read_text())
    if agent == "codex":
        print(json.dumps(value))
    else:
        print("Scope: User config\nCommand: " + value["transport"]["command"] + "\nArgs: " + " ".join(value["transport"]["args"]))
elif args[:2] == ["mcp", "add"]:
    if os.environ.get("QUICKSTART_FAIL_AGENT") == agent:
        raise SystemExit(9)
    command = args[args.index("--") + 1:]
    state.write_text(json.dumps({"name":"repogrammar", "enabled":True, "transport":{"type":"stdio", "command":command[0], "args":command[1:]}}))
elif args[:2] == ["mcp", "remove"]:
    state.unlink(missing_ok=True)
else:
    raise SystemExit(2)
'''


class Quickstart(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.target = subprocess.check_output(["/bin/bash", str(INSTALLER), "--print-target"], text=True).strip()
        cls.assets = release(ARGS.out / "release", ARGS.binary, cls.target)

    def setUp(self):
        self.root = ARGS.out / self._testMethodName
        self.root.mkdir()
        self.home, self.repo = self.root / "home", self.root / "project"
        self.tools, self.scratch = self.root / "tools", self.root / "scratch"
        for directory in (self.home, self.repo, self.tools, self.scratch):
            directory.mkdir()
        for tool in TOOLS:
            source = shutil.which(tool)
            if source:
                (self.tools / tool).symlink_to(source)
        (self.tools / "python3").symlink_to(sys.executable)
        self.command = self.home / ".local/bin/repogrammar"
        self.data = self.home / ".local/share/repogrammar"
        self.env = {
            "HOME": str(self.home), "PATH": str(self.tools), "TMPDIR": str(self.scratch),
            "XDG_CONFIG_HOME": str(self.home / ".config"), "XDG_DATA_HOME": str(self.home / ".local/share"),
            "XDG_CACHE_HOME": str(self.home / ".cache"), "CODEX_HOME": str(self.home / ".codex"),
            "CLAUDE_CONFIG_DIR": str(self.home / ".claude"), "GIT_CONFIG_NOSYSTEM": "1",
            "REPOGRAMMAR_RELEASE_DIR": str(self.assets), "REPOGRAMMAR_COMMAND_DIR": str(self.command.parent),
            "REPOGRAMMAR_INSTALL_DIR": str(self.data), "REPOGRAMMAR_PYTHON_EXECUTABLE": str(self.tools / "python3"),
            "NO_COLOR": "1", "TERM": "dumb", "LC_ALL": "C", "LANG": "C",
        }
        self.sequence = 0
        self.started = time.monotonic()
        self.run_process(["git", "init", "--quiet", str(self.repo)])
        shutil.copyfile(FIXTURE, self.repo / "schemas.py")

    def tearDown(self):
        if self.command.exists() and (self.repo / ".repogrammar").exists():
            self.run_process([str(self.command), "autosync", "stop", "--project", str(self.repo), "--json"], success=None)

    def run_process(self, command, *, cwd=None, data=None, success=True, env=None):
        completed = subprocess.run(command, cwd=cwd or self.repo, env=env or self.env, input=data,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True, timeout=90)
        self.sequence += 1
        prefix = self.root / f"{self.sequence:02d}"
        prefix.with_suffix(".stdout").write_bytes(completed.stdout)
        prefix.with_suffix(".stderr").write_bytes(completed.stderr)
        prefix.with_suffix(".json").write_text(json.dumps({"command":command, "returncode":completed.returncode}))
        message = completed.stdout.decode(errors="replace") + completed.stderr.decode(errors="replace")
        if success is True:
            self.assertEqual(completed.returncode, 0, message)
        elif success is False:
            self.assertNotEqual(completed.returncode, 0, message)
        return completed

    def quickstart(self, *options, piped=False, success=True, cwd=None):
        command = ["/bin/bash", "-s", "--"] if piped else ["/bin/bash", str(INSTALLER)]
        return self.run_process(command + ["--quickstart", *options], cwd=cwd,
                                data=INSTALLER.read_bytes() if piped else b"", success=success)

    def cli(self, *arguments, success=True):
        output = self.run_process([str(self.command), *arguments, "--json"], success=success)
        return json.loads(output.stdout)

    def agents(self, *names):
        for name in names:
            script = self.tools / name
            script.write_text(f"#!{sys.executable}\n" + FAKE_AGENT)
            script.chmod(0o755)

    def assert_installed(self):
        self.assertEqual(digest(self.command), digest(ARGS.binary))
        self.assertEqual(digest(self.data / "workers/python/worker.py"), digest(WORKER))
        self.assertEqual(digest(self.command.parent / "repogrammar-workers/python/worker.py"), digest(WORKER))
        self.assertTrue((self.data / "receipts/product-install.json").is_file())

    def assert_indexed(self):
        self.assert_installed()
        value = self.cli("status", "--project", str(self.repo))
        self.assertTrue(value["initialized"])
        self.assertTrue(value["active_generation"])
        self.assertEqual(value["integrity_check"], "ok")
        return value

    def test_isolated_path_can_run_archive_decompressor(self):
        archive = self.assets / f"repogrammar-{self.target}.tar.gz"
        self.run_process(["gzip", "--test", str(archive)])

    def test_direct_and_piped_authorization_refusal(self):
        before = snapshot(self.home), snapshot(self.repo)
        for piped in (False, True):
            result = self.quickstart("--no-autosync", piped=piped, success=False)
            self.assertIn(b"--yes", result.stdout + result.stderr)
            self.assertEqual((snapshot(self.home), snapshot(self.repo)), before)

    def test_direct_and_piped_dry_run_are_read_only(self):
        before = snapshot(self.home), snapshot(self.repo)
        for piped in (False, True):
            self.quickstart("--dry-run", "--no-autosync", piped=piped)
            self.assertEqual((snapshot(self.home), snapshot(self.repo)), before)

    def test_piped_yes_no_agents_indexes_and_selftests(self):
        result = self.quickstart("--yes", "--no-autosync", piped=True)
        self.assert_indexed()
        self.assertIn(b"repogrammar_context self-test passed", result.stdout.lower())
        self.assertNotIn(b"next: ask your coding agent", result.stdout.lower())
        self.assertFalse(self.cli("status")["readiness"]["autosync"]["running"])
        setup = self.cli("setup", "--target", "none", "--yes", "--no-autosync")
        self.assertIsNone(setup["suggested_question"])

    def test_direct_one_agent_and_idempotent_repeat(self):
        self.agents("codex")
        self.quickstart("--yes", "--version", "v0.5.0", "--no-autosync")
        first = self.assert_indexed()
        instructions = self.home / ".codex/AGENTS.md"
        self.assertTrue(instructions.is_file())
        original = instructions.read_bytes()
        self.quickstart("--yes", "--no-autosync", piped=True)
        second = self.assert_indexed()
        self.assertEqual(first["active_generation"], second["active_generation"])
        self.assertEqual(original, instructions.read_bytes())
        calls = [json.loads(line) for line in (self.home / ".fake-agents/calls.jsonl").read_text().splitlines()]
        self.assertEqual(sum(call[1:3] == ["mcp", "add"] for call in calls), 1)

    def test_auto_detects_all_three_live_agents(self):
        self.agents("codex", "claude", "opencode")
        self.quickstart("--yes", "--no-autosync")
        self.assert_indexed()
        for agent in ("codex", "claude"):
            self.assertTrue((self.home / f".fake-agents/{agent}.json").is_file())
        opencode = json.loads((self.home / ".config/opencode/opencode.json").read_text())
        self.assertEqual(opencode["mcp"]["repogrammar"]["command"][1:], ["serve"])

    def test_target_none_skips_detected_agents_and_instructions(self):
        self.agents("codex", "claude", "opencode")
        self.quickstart("--yes", "--target", "none", "--no-instructions", "--no-autosync")
        self.assert_indexed()
        self.assertFalse((self.home / ".codex/AGENTS.md").exists())
        self.assertFalse((self.home / ".config/opencode/opencode.json").exists())
        self.assertFalse((self.home / ".fake-agents/codex.json").exists())

    def test_explicit_target_and_no_instructions(self):
        self.agents("codex", "claude")
        self.quickstart("--yes", "--target", "codex", "--no-instructions", "--no-autosync")
        self.assert_indexed()
        self.assertTrue((self.home / ".fake-agents/codex.json").exists())
        self.assertFalse((self.home / ".fake-agents/claude.json").exists())
        self.assertFalse((self.home / ".codex/AGENTS.md").exists())

    def test_outside_git_installs_machine_only(self):
        outside = self.root / "outside"
        outside.mkdir()
        self.quickstart("--yes", "--no-autosync", piped=True, cwd=outside)
        self.assert_installed()
        self.assertFalse((outside / ".repogrammar").exists())
        self.assertFalse((self.root / ".repogrammar").exists())

    def test_explicit_project_with_shell_characters(self):
        self.repo.rename(self.root / "project space ' $ ; [x]")
        self.repo = self.root / "project space ' $ ; [x]"
        old_home = str(self.home)
        self.home.rename(self.root / "home space ' $ ; [x]")
        self.home = self.root / "home space ' $ ; [x]"
        self.env = {key:value.replace(old_home, str(self.home)) for key, value in self.env.items()}
        self.command = self.home / ".local/bin/repogrammar"
        self.data = self.home / ".local/share/repogrammar"
        self.quickstart("--yes", "--project", str(self.repo), "--no-autosync", piped=True, cwd=self.root)
        self.assert_indexed()

    def test_explicit_invalid_project_fails_before_install(self):
        outside = self.root / "existing-non-git"
        outside.mkdir()
        for project in (self.root / "not-a-repo", outside):
            self.quickstart("--yes", "--project", str(project), success=False)
            self.assertFalse(self.command.exists())
            self.assertFalse(self.data.exists())

    def test_deferred_local_scope_does_not_write(self):
        before = snapshot(self.home), snapshot(self.repo)
        result = self.quickstart("--yes", "--scope", "local", success=False)
        self.assertIn(b"scope", result.stdout + result.stderr)
        self.assertEqual((snapshot(self.home), snapshot(self.repo)), before)

    def test_refreshes_changed_repository(self):
        self.quickstart("--yes", "--no-autosync")
        before = self.assert_indexed()["active_generation"]
        schema = self.repo / "schemas.py"
        schema.write_text(schema.read_text().replace("id: int", "id: str", 1))
        self.assertNotEqual(self.cli("status")["product_readiness"]["family_evidence"]["state"], "fresh")
        self.quickstart("--yes", "--no-autosync", piped=True)
        self.assertNotEqual(self.assert_indexed()["active_generation"], before)

    def test_foreign_agent_preserved_while_repository_ready(self):
        self.agents("opencode")
        config = self.home / ".config/opencode/opencode.json"
        config.parent.mkdir(parents=True)
        original = '{"mcp":{"repogrammar":{"type":"local","command":["/foreign/program","serve"],"enabled":true}},"theme":"dark"}\n'
        config.write_text(original)
        self.quickstart("--yes", "--no-autosync")
        self.assert_indexed()
        self.assertEqual(config.read_text(), original)

    def test_malformed_and_drifted_agent_config_preserved(self):
        self.agents("codex")
        self.quickstart("--yes", "--no-autosync")
        state = self.home / ".fake-agents/codex.json"
        value = json.loads(state.read_text())
        value["transport"]["command"] = "/foreign/drifted"
        state.write_text(json.dumps(value))
        original = state.read_bytes()
        self.quickstart("--yes", "--no-autosync")
        self.assertEqual(state.read_bytes(), original)
        self.assert_indexed()
        state.write_text('{}')
        self.quickstart("--yes", "--no-autosync")
        self.assertEqual(state.read_text(), '{}')
        self.assert_indexed()

    def test_native_failure_retains_installed_cli(self):
        self.agents("codex")
        self.env["QUICKSTART_FAIL_AGENT"] = "codex"
        self.quickstart("--yes", "--no-autosync", success=False)
        self.assert_installed()
        self.assertFalse((self.home / ".fake-agents/codex.json").exists())
        self.assertFalse((self.home / ".codex/AGENTS.md").exists())

    def test_index_failure_rolls_back_new_agent_only(self):
        self.agents("codex")
        self.env["REPOGRAMMAR_RELEASE_DIR"] = str(release(self.root / "broken-worker", ARGS.binary, self.target, mutation="broken-worker"))
        # Debug builds prefer the checkout worker; bind this fault to the installed bundle.
        self.env["REPOGRAMMAR_PYTHON_WORKER"] = str(self.data / "workers/python/worker.py")
        result = self.quickstart("--yes", "--no-autosync", success=False)
        self.assertIn(b"index_failed", result.stderr)
        self.assertTrue(self.command.exists())
        self.assertFalse((self.home / ".fake-agents/codex.json").exists())
        self.assertFalse((self.home / ".codex/AGENTS.md").exists())

    def test_installed_dry_run_preserves_receipts_and_index(self):
        self.quickstart("--yes", "--no-autosync")
        before = snapshot(self.home), snapshot(self.repo)
        self.quickstart("--dry-run", "--no-autosync", piped=True)
        self.assertEqual((snapshot(self.home), snapshot(self.repo)), before)

    def test_unmanaged_collision_preserves_foreign_command(self):
        self.command.parent.mkdir(parents=True)
        self.command.write_text("foreign command\n")
        self.command.chmod(0o755)
        before = snapshot(self.home)
        self.quickstart("--yes", "--no-autosync", success=False)
        self.assertEqual(snapshot(self.home), before)
        self.quickstart("--yes", "--no-autosync", "--replace-unmanaged-command")
        self.assert_indexed()
        self.assertEqual(self.command.with_name("repogrammar.unmanaged-backup").read_text(), "foreign command\n")

    def test_foreign_path_command_is_not_executed_or_deleted(self):
        foreign = self.tools / "repogrammar"
        foreign.write_text("#!/bin/sh\nexit 91\n")
        foreign.chmod(0o755)
        self.quickstart("--yes", "--no-autosync")
        self.assert_indexed()
        self.assertEqual(foreign.read_text(), "#!/bin/sh\nexit 91\n")

    def test_missing_and_unsupported_python_fail_before_writes(self):
        python = self.tools / "python3"
        python.unlink()
        before = snapshot(self.home), snapshot(self.repo)
        for version in (None, "Python 3.9.9", "Python 4.0.0"):
            if version:
                python.write_text(f"#!/bin/sh\necho '{version}'\nexit 1\n")
                python.chmod(0o755)
            result = self.quickstart("--yes", "--no-autosync", success=False)
            self.assertIn(b"Python", result.stdout + result.stderr)
            self.assertEqual((snapshot(self.home), snapshot(self.repo)), before)

    def test_bad_release_assets_fail_without_installation(self):
        errors = {
            "checksum":b"checksum", "missing-checksum":b"not found",
            "malformed-checksum":b"checksum", "missing-worker":b"bundled Python worker",
            "unexpected":b"unsafe or unexpected", "symlink":b"non-regular-file",
            "hardlink":b"non-regular-file", "duplicate":b"duplicate",
            "traversal":b"unsafe or unexpected", "truncated":b"archive",
        }
        for mutation, error in errors.items():
            with self.subTest(mutation=mutation):
                assets = release(self.root / mutation, ARGS.binary, self.target, mutation=mutation)
                self.env["REPOGRAMMAR_RELEASE_DIR"] = str(assets)
                result = self.quickstart("--yes", "--no-autosync", piped=True, success=False)
                self.assertIn(error, result.stdout + result.stderr)
                self.assertFalse(self.command.exists())
                self.assertFalse(self.data.exists())
                self.assertFalse((self.root / "outside").exists())
                self.assertFalse((self.scratch / "outside").exists())

    def test_valid_checksum_without_trailing_newline(self):
        assets = release(self.root / "no-newline", ARGS.binary, self.target)
        checksum = assets / f"repogrammar-{self.target}.tar.gz.sha256"
        checksum.write_bytes(checksum.read_bytes().rstrip(b"\n"))
        self.env["REPOGRAMMAR_RELEASE_DIR"] = str(assets)
        self.quickstart("--yes", "--no-autosync", piped=True)
        self.assert_indexed()

    def test_download_interruption_is_bounded_and_nonmutating(self):
        self.env.pop("REPOGRAMMAR_RELEASE_DIR")
        curl = self.tools / "curl"
        curl.write_text("#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$HOME/curl-args\"\nexit 28\n")
        curl.chmod(0o755)
        self.quickstart("--yes", "--no-autosync", success=False)
        self.assertFalse(self.command.exists())
        self.assertFalse(self.data.exists())
        self.assertIn("--max-time", (self.home / "curl-args").read_text())

    def test_symlink_install_root_preserves_outside_sentinel(self):
        outside = self.root / "outside"
        outside.mkdir()
        (outside / "sentinel").write_text("preserve")
        self.data.parent.mkdir(parents=True)
        self.data.symlink_to(outside, target_is_directory=True)
        before = snapshot(outside)
        self.quickstart("--yes", "--no-autosync", success=False)
        self.assertEqual(snapshot(outside), before)
        self.assertFalse(self.command.exists())

    def test_symlinked_temporary_directory_is_canonicalized(self):
        temporary_link = self.root / "scratch-link"
        temporary_link.symlink_to(self.scratch, target_is_directory=True)
        self.env["TMPDIR"] = str(temporary_link)
        self.quickstart("--yes", "--no-autosync", piped=True)
        self.assert_indexed()

    def test_permission_failure_preserves_existing_install_directory(self):
        if os.geteuid() == 0:
            self.skipTest("root bypasses ordinary directory write permissions")
        self.data.mkdir(parents=True)
        (self.data / "sentinel").write_text("preserve")
        before = snapshot(self.data)
        self.data.chmod(0o555)
        try:
            self.quickstart("--yes", "--no-autosync", success=False)
        finally:
            self.data.chmod(0o755)
        self.assertEqual(snapshot(self.data), before)
        self.assertFalse(self.command.exists())

    def test_malformed_product_receipt_refuses_update(self):
        self.quickstart("--yes", "--no-autosync")
        receipt = self.data / "receipts/product-install.json"
        receipt.write_text("malformed receipt\n")
        before = snapshot(self.home), snapshot(self.repo)
        self.quickstart("--yes", "--no-autosync", success=False)
        self.assertEqual((snapshot(self.home), snapshot(self.repo)), before)

    def terminal(self, answer, before_answer=None):
        master, slave = pty.openpty()

        def controlling_terminal():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

        process = subprocess.Popen(["/bin/bash", "-s", "--", "--quickstart", "--no-autosync"],
                                   cwd=self.repo, env=self.env, stdin=subprocess.PIPE,
                                   stdout=slave, stderr=slave, preexec_fn=controlling_terminal)
        os.close(slave)
        process.stdin.write(INSTALLER.read_bytes())
        process.stdin.close()
        output, sent, deadline = b"", False, time.monotonic() + 90
        try:
            while time.monotonic() < deadline:
                if select.select([master], [], [], 0.1)[0]:
                    try:
                        chunk = os.read(master, 65536)
                    except OSError:
                        break
                    if not chunk:
                        break
                    output += chunk
                    if b"[y/N]" in output and not sent:
                        if before_answer:
                            before_answer()
                        os.write(master, answer)
                        sent = True
                if process.poll() is not None and not select.select([master], [], [], 0)[0]:
                    break
            status = process.wait(timeout=2)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            os.close(master)
            (self.root / "terminal.log").write_bytes(output)
        self.assertTrue(sent, output.decode(errors="replace"))
        self.assertEqual(output.count(b"[y/N]"), 1, output.decode(errors="replace"))
        return status, output

    def test_piped_controlling_terminal_confirmation(self):
        status, output = self.terminal(b"yes\n")
        self.assertEqual(status, 0, output.decode(errors="replace"))
        self.assert_indexed()

    def test_piped_controlling_terminal_default_no(self):
        before = snapshot(self.home), snapshot(self.repo)
        self.terminal(b"\n")
        self.assertEqual((snapshot(self.home), snapshot(self.repo)), before)

    def test_new_agent_after_confirmation_plan_requires_new_authorization(self):
        status, output = self.terminal(b"yes\n", lambda: self.agents("codex"))
        self.assertNotEqual(status, 0, output.decode(errors="replace"))
        self.assert_installed()
        self.assertFalse((self.home / ".fake-agents/codex.json").exists())
        self.assertFalse((self.repo / ".repogrammar").exists())

    def test_default_autosync_has_live_readiness_and_stops(self):
        self.quickstart("--yes", "--target", "none")
        self.assertTrue(self.assert_indexed()["readiness"]["autosync"]["running"])
        self.cli("autosync", "stop", "--project", str(self.repo))
        self.assertFalse(self.cli("status")["readiness"]["autosync"]["running"])

    def test_managed_uninstall_preserves_repository_and_foreign_data(self):
        self.quickstart("--yes", "--no-autosync")
        sentinel = self.data / "foreign-user-file"
        sentinel.write_text("preserve")
        before = snapshot(self.home), snapshot(self.repo)
        self.cli("uninstall", "--dry-run", "--yes")
        self.assertEqual((snapshot(self.home), snapshot(self.repo)), before)
        result = self.cli("uninstall", "--yes")
        self.assertEqual(result["status"], "finalizer_pending")
        report = Path(result["report_path"])
        deadline = time.monotonic() + 15
        final = None
        while time.monotonic() < deadline:
            if report.exists():
                final = json.loads(report.read_text())
                if final["status"] != "in_progress":
                    break
            time.sleep(0.05)
        self.assertIsNotNone(final)
        self.assertEqual(final["status"], "complete")
        self.assertFalse(self.command.exists())
        self.assertFalse(self.command.is_symlink())
        for relative in ("bin/repogrammar", "workers/python/worker.py", "receipts/product-install.json"):
            self.assertFalse((self.data / relative).exists())
        self.assertEqual(sentinel.read_text(), "preserve")
        self.assertTrue((self.repo / ".repogrammar/repogrammar.sqlite").exists())

    def test_previous_candidate_upgrade(self):
        if not ARGS.previous_binary:
            self.skipTest("--previous-binary not supplied; cross-revision upgrade not measured")
        previous = release(self.root / "previous-release", ARGS.previous_binary, self.target)
        self.env["REPOGRAMMAR_RELEASE_DIR"] = str(previous)
        result = self.quickstart("--yes", "--no-autosync", success=False)
        self.assertIn(b"QUICKSTART_RELEASE_REQUIRED", result.stderr)
        self.assertFalse(self.command.exists())
        self.run_process(["/bin/bash", str(INSTALLER), "--install-cli-only", "--yes"])
        self.assertEqual(digest(self.command), digest(ARGS.previous_binary))
        self.env["REPOGRAMMAR_RELEASE_DIR"] = str(self.assets)
        self.quickstart("--yes", "--no-autosync", piped=True)
        self.assert_indexed()


class EvidenceResult(unittest.TextTestResult):
    def addSuccess(self, test):
        super().addSuccess(test)
        RESULTS.append({"case":test._testMethodName, "status":"PASS", "seconds":round(time.monotonic() - test.started, 3)})

    def addFailure(self, test, error):
        super().addFailure(test, error)
        RESULTS.append({"case":test._testMethodName, "status":"FAIL"})

    def addError(self, test, error):
        super().addError(test, error)
        RESULTS.append({"case":str(test), "status":"ERROR"})

    def addSkip(self, test, reason):
        super().addSkip(test, reason)
        RESULTS.append({"case":test._testMethodName, "status":"NOT_MEASURED", "reason":reason})


def main():
    global ARGS
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--previous-binary", type=Path, help="Pre-quickstart binary for the cross-revision upgrade control")
    parser.add_argument("--out", type=Path, help="New directory retaining fixtures, command logs and summary.json")
    parser.add_argument("--case", help="Run one named test while diagnosing a failed contract")
    ARGS = parser.parse_args()
    if sys.version_info.major != 3 or sys.version_info.minor < 10:
        parser.error("Python 3.10+ within 3.x is required")
    ARGS.binary = ARGS.binary.resolve(strict=True)
    if ARGS.previous_binary:
        ARGS.previous_binary = ARGS.previous_binary.resolve(strict=True)
    temporary = None
    if ARGS.out:
        ARGS.out = ARGS.out.resolve()
        ARGS.out.mkdir(parents=True, exist_ok=False)
    else:
        temporary = tempfile.TemporaryDirectory(prefix="repogrammar-quickstart-tests-")
        ARGS.out = Path(temporary.name).resolve()
    suite = unittest.defaultTestLoader.loadTestsFromName(ARGS.case, Quickstart) if ARGS.case else unittest.defaultTestLoader.loadTestsFromTestCase(Quickstart)
    result = unittest.TextTestRunner(verbosity=2, resultclass=EvidenceResult).run(suite)
    (ARGS.out / "summary.json").write_text(json.dumps({
        "schema":"quickstart-tests.v1", "binary_sha256":digest(ARGS.binary),
        "worker_sha256":digest(WORKER), "installer_sha256":digest(INSTALLER),
        "platform":sys.platform, "target":getattr(Quickstart, "target", None),
        "python":sys.version.split()[0], "tests_run":result.testsRun,
        "success":result.wasSuccessful(), "cases":RESULTS,
        "failures":[str(test) for test, _ in result.failures],
        "errors":[str(test) for test, _ in result.errors],
        "skipped":[{"case":str(test), "reason":reason} for test, reason in result.skipped],
        "limitations":["Native coding agents are isolated protocol fakes; product binary, worker, setup and MCP self-test are real.", "Only the current host executes natively."],
    }, indent=2) + "\n")
    if temporary:
        temporary.cleanup()
        print("Quickstart fixtures removed; use --out to retain evidence.")
    else:
        print(f"Quickstart evidence: {ARGS.out}")
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
