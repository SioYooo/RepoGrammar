"""Deterministic worktree tree hash, byte-compatible with the Rust harness.

This is a faithful Python reimplementation of `fixture_version_hash` in
`src/rust/bin/repo_guard.rs` (the product-eval harness `fixture_version`
convention). RQ5 (agent study) records a `worktree_sha256` per pinned repo and
per per-run worktree using this exact convention so that arm-isolation
identity (§9.1 of the design) is provable against the same algorithm the
product-eval lane already uses.

Byte-for-byte algorithm (transcribed from repo_guard.rs `fixture_version_hash`
/ `collect_fixture_files`):

  1. Walk `root` recursively. Skip any entry that is a symlink (both symlinked
     files and symlinked directories are skipped entirely, matching Rust's
     `symlink_metadata` + `is_symlink()` guard). Recurse into regular
     directories; collect regular files only (special files are ignored).
  2. For each collected file, the relative path is computed against `root`
     with the OS separator replaced by '/'.
  3. Sort the (relative_path, absolute_path) pairs by the *UTF-8 bytes* of the
     relative path. Rust's `String::cmp` is byte-lexicographic over UTF-8, so
     Python must sort on `rel.encode("utf-8")`, not on the default code-point
     ordering (they diverge for non-ASCII multibyte paths).
  4. Feed the SHA-256 hasher, per file, in this exact order:
       - u64 little-endian length of the relative path's UTF-8 bytes
       - the relative path's UTF-8 bytes
       - u64 little-endian length of the file contents
       - the raw file contents
  5. Return the lowercase hex digest.

The intermediate directory-listing sort in the Rust code only affects
traversal order; because the full flat list is re-sorted by relative path
before hashing, traversal order does not affect the digest. This module
therefore only needs to reproduce the final sort + length-prefixed feed.
"""

from __future__ import annotations

import hashlib
import os
import struct
from typing import List, Tuple


def _collect_files(base: str, current: str, out: List[Tuple[str, str]]) -> None:
    """Recursively collect (relative_path, absolute_path) for regular files.

    Mirrors `collect_fixture_files`: symlinks skipped, regular dirs recursed,
    regular files collected. `os.scandir` with `follow_symlinks=False` gives
    us the symlink check without an extra stat.
    """
    with os.scandir(current) as it:
        for entry in it:
            # is_symlink does not follow; matches Rust symlink_metadata guard.
            if entry.is_symlink():
                continue
            if entry.is_dir(follow_symlinks=False):
                _collect_files(base, entry.path, out)
            elif entry.is_file(follow_symlinks=False):
                rel = os.path.relpath(entry.path, base).replace(os.sep, "/")
                out.append((rel, entry.path))
            # anything else (fifo, socket, ...) is neither file nor dir: skip.


def tree_sha256(root: str) -> str:
    """Return the lowercase hex SHA-256 tree hash of `root`.

    Raises FileNotFoundError if `root` does not exist.
    """
    if not os.path.isdir(root):
        raise FileNotFoundError(f"tree root is not a directory: {root}")
    files: List[Tuple[str, str]] = []
    _collect_files(root, root, files)
    # Byte-lexicographic sort on the UTF-8 encoding of the relative path,
    # matching Rust's byte-wise String Ord.
    files.sort(key=lambda pair: pair[0].encode("utf-8"))
    hasher = hashlib.sha256()
    for rel, absolute in files:
        rel_bytes = rel.encode("utf-8")
        hasher.update(struct.pack("<Q", len(rel_bytes)))
        hasher.update(rel_bytes)
        with open(absolute, "rb") as fh:
            contents = fh.read()
        hasher.update(struct.pack("<Q", len(contents)))
        hasher.update(contents)
    return hasher.hexdigest()


if __name__ == "__main__":
    import sys

    if len(sys.argv) != 2:
        print("usage: python3 treehash.py <dir>", file=sys.stderr)
        raise SystemExit(2)
    print(tree_sha256(sys.argv[1]))
