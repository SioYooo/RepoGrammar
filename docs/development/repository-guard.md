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
- required bootstrap docs and workflows exist, including
  `docs/decisions/ADR-0008-repo-local-state-boundary.md`, the v0.1 planning
  documents, the Python v0.1 analysis specification, ADR-0011, ADR-0012, the
  substrate hardening checkpoint, typed UNKNOWN specification, ADR-0009/ADR-0010,
  and their durable memory mirrors under
  `.agents/memories/`.
- required skills exist and have `name` and `description` front matter.
- nested `AGENTS.md` or `CLAUDE.md` files do not exist.
- lowercase `agents.md` or `claude.md` duplicates do not exist.
- source files with guarded extensions do not exist outside `src/`, regardless
  of implementation language.
- generated local state directories such as `.repogrammar/`,
  `.repogrammar-*`, `.codegraph/`, `target/`, and `.git/` are ignored.

Check mode reports concrete paths and rules and does not modify the repository.

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

## CI integration

CI runs `repo-guard check` on every push and pull request. Pull requests also
run `check-diff` when base and head revisions are available.
