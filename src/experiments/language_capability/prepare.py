"""Fetch pinned public snapshots and emit input for the existing product-eval."""

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tarfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "agent_study"))
from treehash import tree_sha256

PREREGISTRATION = Path(__file__).with_name("preregistration.json")
PREREGISTRATION_SHA256 = "9854a75da31e451b78b6ba5d46905fef98cc905bb07dac1719cf1cdf7f0c2e64"
MAX_ARCHIVE = 64 * 1024 * 1024
MAX_CONTENT = 256 * 1024 * 1024
MAX_FILE = 16 * 1024 * 1024
MAX_ENTRIES = 20000


def download(url, destination, limit):
    subprocess.run(["curl", "-q", "--fail", "--silent", "--show-error", "--location",
                    "--proto", "=https", "--proto-redir", "=https", "--connect-timeout", "15",
                    "--max-time", "90", "--max-filesize", str(limit), "--output", str(destination), url],
                   check=True, timeout=95)
    with destination.open("rb") as stream:
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise ValueError("public snapshot download exceeds the declared byte bound")
    return hashlib.sha256(data).hexdigest()


def git_blob_sha(data):
    return hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()


def extract(archive, destination, prefix, expected, symlinks=()):
    """Materialize regular files; verify admitted Git symlink blobs without links."""
    destination.mkdir()
    seen, files, total = set(), set(), 0
    with tarfile.open(archive, "r:gz") as bundle:
        for count, member in enumerate(bundle, 1):
            if count > MAX_ENTRIES:
                raise ValueError("public snapshot has too many archive entries")
            parts = member.name.rstrip("/").split("/")
            if parts[0] != prefix or any(p in ("", ".", "..") for p in parts) or "\\" in member.name:
                raise ValueError("unsafe public snapshot path")
            relative = PurePosixPath(*parts[1:])
            if str(relative) in seen:
                raise ValueError("duplicate or non-regular public snapshot member")
            seen.add(str(relative))
            if relative.as_posix() in symlinks and not member.issym():
                raise ValueError("registered Git symlink was replaced by another archive member type")
            if member.issym() and relative.as_posix() in symlinks:
                if expected[relative.as_posix()] != git_blob_sha(member.linkname.encode()):
                    raise ValueError("public snapshot symlink target differs from the pinned Git blob")
                files.add(relative.as_posix())
                continue
            if not (member.isdir() or member.isfile()):
                raise ValueError("non-regular public snapshot member")
            path = destination.joinpath(*relative.parts)
            if member.isdir():
                path.mkdir(parents=True, exist_ok=True)
                continue
            total += member.size
            if member.size < 0 or member.size > MAX_FILE or total > MAX_CONTENT:
                raise ValueError("public snapshot exceeds the declared content bound")
            path.parent.mkdir(parents=True, exist_ok=True)
            with bundle.extractfile(member) as source:
                data = source.read(member.size + 1)
            if len(data) != member.size:
                raise ValueError("public snapshot member size mismatch")
            if expected.get(relative.as_posix()) != git_blob_sha(data):
                raise ValueError("public snapshot content differs from the pinned Git blob")
            with path.open("xb") as output:
                output.write(data)
            files.add(relative.as_posix())
    return files, total


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True, help="New directory outside the checkout")
    args = parser.parse_args()
    raw = PREREGISTRATION.read_bytes()
    if hashlib.sha256(raw).hexdigest() != PREREGISTRATION_SHA256:
        parser.error("the frozen preregistration changed")
    prereg = json.loads(raw)
    output = args.out.resolve()
    checkout = Path(__file__).resolve().parents[3]
    if output == checkout or checkout in output.parents:
        parser.error("public source snapshots must remain outside the checkout")
    output.mkdir(parents=True, exist_ok=False)
    records, fixtures = [], []
    for corpus in prereg["corpora"]:
        owner, name = corpus["url"].rstrip("/").split("/")[-2:]
        commit = corpus["commit"]
        base = output / corpus["id"]
        base.mkdir()
        commit_path = base / "git-commit.json"
        download(f"https://api.github.com/repos/{owner}/{name}/git/commits/{commit}", commit_path, 1024 * 1024)
        identity = json.loads(commit_path.read_text())
        if identity.get("sha") != commit:
            raise ValueError("Git commit response does not match the frozen source commit")
        tree_sha = identity["tree"]["sha"]
        tree_path = base / "git-tree.json"
        download(f"https://api.github.com/repos/{owner}/{name}/git/trees/{tree_sha}?recursive=1", tree_path, 8 * 1024 * 1024)
        tree = json.loads(tree_path.read_text())
        if tree.get("truncated") is not False or tree.get("sha") != tree_sha:
            raise ValueError("public Git tree inventory is incomplete")
        if any(item["type"] not in ("blob", "tree") for item in tree["tree"]):
            raise ValueError("public corpus contains a submodule requiring separate admission")
        expected = {item["path"]:item["sha"] for item in tree["tree"] if item["type"] == "blob"}
        symlinks = {item["path"] for item in tree["tree"] if item["mode"] == "120000"}
        archive = base / "snapshot.tar.gz"
        archive_hash = download(f"https://codeload.github.com/{owner}/{name}/tar.gz/{commit}", archive, MAX_ARCHIVE)
        snapshot = base / "snapshot"
        files, size = extract(archive, snapshot, f"{name}-{commit}", expected, symlinks)
        if files != set(expected):
            raise ValueError("archive does not contain the complete pinned Git tree")
        extensions = {}
        for path in sorted(files):
            suffix = Path(path).suffix.lower() or "[none]"
            extensions[suffix] = extensions.get(suffix, 0) + 1
        records.append({
            "corpus_id":corpus["id"], "url":corpus["url"], "commit":commit,
            "git_tree_sha":tree["sha"], "archive_sha256":archive_hash,
            "fixture_version":tree_sha256(str(snapshot)), "regular_files":len(files) - len(symlinks),
            "source_bytes":size, "files_by_extension":extensions,
            "complete_git_blob_inventory_verified":True, "materialized_full_worktree":not symlinks,
            "omitted_symlinks":[{"path":p, "mode":"120000", "git_blob_sha":expected[p], "reason":"verified target blob; symlink not materialized or followed"} for p in sorted(symlinks)],
            "repository_code_executed":False,
            "license_path":"LICENSE", "license_sha256":hashlib.sha256((snapshot / "LICENSE").read_bytes()).hexdigest(),
        })
        fixtures.append({"fixture_id":corpus["id"], "root":str(snapshot), "description":corpus["selection_reason"]})
    queries = []
    for query in prereg["prospective_queries"]:
        expected = {"outcome":query["expected"]}
        if "family_prefix" in query:
            expected["family_prefix"] = query["family_prefix"]
        queries.append({"query_id":query["id"], "fixture_id":query["corpus"],
                        "kind":"prospective_phrase", "intent":query["intent"],
                        "operation":"find", "target":query["target"], "mode":"compact", "expected":expected})
    (output / "public-corpus.json").write_text(json.dumps({"schema_version":"product-eval-corpus.v1", "fixtures":fixtures, "queries":queries}, indent=2) + "\n")
    (output / "provenance.json").write_text(json.dumps({"schema_version":"public-source-provenance.v1", "preregistration_sha256":PREREGISTRATION_SHA256, "corpora":records}, indent=2) + "\n")
    print(f"Prepared {len(records)} pinned regular-file snapshots; verified symlink blobs were not materialized; no repository code executed")


if __name__ == "__main__":
    main()
