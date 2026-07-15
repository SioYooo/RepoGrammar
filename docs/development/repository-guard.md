# Repository Guard

`repo-guard` is a repository governance CLI implemented in
`src/rust/bin/repo_guard.rs`. It is separate from the RepoGrammar product runtime.

## Commands

```text
cargo run --quiet --bin repo-guard -- check
cargo run --quiet --bin repo-guard -- sync-agent-guides --from AGENTS.md
cargo run --quiet --bin repo-guard -- sync-agent-guides --from CLAUDE.md
cargo run --quiet --bin repo-guard -- check-diff --base <git-revision> --head <git-revision>
```

## check

The check command verifies:

- `AGENTS.md` and `CLAUDE.md` exist.
- both guides are regular files and not symlinks.
- both guides are byte-identical.
- required bootstrap docs and workflows exist, including CI and release
  workflows plus
  `docs/decisions/ADR-0008-repo-local-state-boundary.md`, the v0.1 planning
  documents, the Python v0.1 analysis specification, ADR-0011, ADR-0012, the
  substrate hardening checkpoint, typed UNKNOWN specification, ADR-0009/ADR-0010,
  their durable memory mirrors under `.agents/memories/`, and the accepted
  ADR-0020 Top-20 language expansion gate plus its active implementation plan.
- required skills exist and have `name` and `description` front matter.
- nested `AGENTS.md` or `CLAUDE.md` files do not exist.
- lowercase `agents.md` or `claude.md` duplicates do not exist.
- source files with guarded extensions do not exist outside `src/`, regardless
  of implementation language. The guarded set includes `.rs`, `.c`, `.cc`,
  `.cpp`, `.cxx`, `.h`, `.hpp`, `.hh`, `.hxx`, `.go`, `.py`, `.js`, `.jsx`,
  `.ts`, `.tsx`, `.java`, `.cs`, `.kt`, `.kts`, and shell/SQL extensions, so C#
  and C/C++ fixtures must live under `src/fixtures/`.
- generated local state directories such as `.repogrammar/`,
  `.repogrammar-*`, `.codegraph/`, `target/`, and `.git/` are ignored. A direct
  child of `.claude/worktrees/` is ignored only when its bounded regular `.git`
  pointer resolves under this repository's `.git/worktrees/` directory. This
  permits complete isolated checkouts created for parallel agents without
  exempting unlinked files or directories named `worktrees` elsewhere.
- GitHub workflow files do not use deprecated Node.js 20 action majors for
  first-party checkout or Node setup actions; currently `actions/checkout@v4`
  and `actions/setup-node@v4` are rejected in favor of `@v5` or newer.

Check mode reports concrete paths and rules and does not modify the repository.
Linked-agent-worktree recognition reads at most 4 KiB from a regular,
non-symlink `.git` pointer and requires its canonical target to be a direct
child of this repository's canonical `.git/worktrees/` directory. Missing,
oversized, non-UTF-8, malformed, foreign, or unresolvable pointers fail closed:
the candidate directory remains inside the normal repository scan. The check
does not traverse a recognized linked checkout, so cost is bounded per active
agent worktree rather than by the size of each checkout.

## sync-agent-guides

The sync command accepts only root `AGENTS.md` or root `CLAUDE.md` as `--from`.
It copies raw bytes to the mirror file and re-checks byte equality.

## check-diff

The diff command compares two Git revisions with `git diff --name-only`. If any
`src/` path changes, at least one documentation or agent-material path must also
change. This is a minimum gate, not proof that the documentation is semantically
complete.

## Exit codes

- `0`: requested guard passed.
- nonzero: invalid arguments, failed Git comparison, filesystem error, or guard
  violation.

## False positives

Prefer changing the repository structure or documenting a narrower allowlist over
weakening the guard. Any guard behavior change must update this document and
include tests.

Required-document registration tests must remove each newly registered
authority document from an otherwise complete temporary repository and assert a
`RequiredDocumentMissing` violation for its exact path.

## CI integration

CI runs `repo-guard check` on every push and pull request. Pull requests also
run `check-diff` when base and head revisions are available.
