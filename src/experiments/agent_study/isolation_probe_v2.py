"""Malicious offline controls, run only inside the runner's OS sandbox."""
import glob
import errno
from contextlib import closing
import json
import os
from pathlib import Path
import socket
import sqlite3
import subprocess
import sys


def probe(worktree, index, product, oracle, other_profile, guide_sha):
    import hashlib
    worktree, index = Path(worktree), Path(index)
    checks = {}

    def denied(name, action):
        try:
            action()
        except OSError as error:
            checks[name] = error.errno in (errno.EACCES, errno.EPERM)
        else:
            checks[name] = False

    checks["source_read"] = (worktree / "allowed.txt").read_text() == "allowed\n"
    checks["source_glob"] = "allowed.txt" in os.listdir(worktree)
    (worktree / "written.txt").write_text("written\n")
    checks["source_write"] = (worktree / "written.txt").read_text() == "written\n"
    checks["source_shell"] = subprocess.run(
        ["/bin/cat", str(worktree / "allowed.txt")], capture_output=True,
        timeout=5, check=True).stdout == b"allowed\n"
    with closing(sqlite3.connect(worktree / "allowed.sqlite")) as db, db:
        db.execute("create table positive(value int)")
        db.execute("insert into positive values (1)")
        checks["source_sqlite"] = db.execute("select value from positive").fetchone() == (1,)
    denied("read", lambda: index.read_bytes())
    denied("grep", lambda: "secret" in index.read_text())
    denied("glob", lambda: os.listdir(index.parent))
    checks["recursive_glob"] = not glob.glob(str(worktree / "**/.repogrammar/*"), recursive=True)
    checks["shell"] = subprocess.run(
        ["/bin/cat", str(index)], capture_output=True, timeout=5).returncode != 0
    try:
        with closing(sqlite3.connect(f"file:{index}?mode=ro", uri=True)) as db, db:
            db.execute("select value from sentinel").fetchall()
    except sqlite3.DatabaseError:
        checks["sqlite"] = True
    else:
        checks["sqlite"] = False
    denied("symlink", lambda: (worktree / "index-alias").read_bytes())
    denied("hardlink", lambda: os.link(index, worktree / "hardlink-alias"))
    denied("rename", lambda: os.rename(index.parent, worktree / "renamed-index"))
    denied("product_read", lambda: Path(product).read_bytes())
    denied("product_exec", lambda: subprocess.run(
        [product, "version"], capture_output=True, timeout=5))
    denied("oracle", lambda: Path(oracle).read_bytes())
    denied("other_profile", lambda: Path(other_profile).read_bytes())
    guide = Path(os.environ["CLAUDE_CONFIG_DIR"]) / "CLAUDE.md"
    checks["global_path"] = (hashlib.sha256(guide.read_bytes()).hexdigest() == guide_sha
                             if guide_sha else not guide.exists())
    checks["environment"] = set(os.environ) == {
        "HOME", "CODEX_HOME", "CLAUDE_CONFIG_DIR", "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "PATH", "LANG", "LC_ALL", "TMPDIR"}
    try:
        with socket.socket() as connection:
            connection.bind(("127.0.0.1", 0))
    except OSError as error:
        checks["network_bind"] = error.errno in (errno.EACCES, errno.EPERM)
    else:
        checks["network_bind"] = False
    try:
        with socket.socket() as connection:
            connection.settimeout(1)
            connection.connect(("127.0.0.1", 9))
    except OSError as error:
        checks["network_connect"] = error.errno in (errno.EACCES, errno.EPERM)
    else:
        checks["network_connect"] = False
    return checks


if __name__ == "__main__":
    print(json.dumps(probe(*sys.argv[1:]), sort_keys=True))
