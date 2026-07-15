# Contributing

RepoGrammar is pre-alpha and conservative by design. Contributions are welcome
when they preserve source-backed evidence, typed `UNKNOWN`, local-first
privacy, and pattern-family-first CLI behavior.

## Good First Contribution Categories

- Documentation examples: clearer README, quickstarts, limitations, and
  framework examples that do not overclaim support.
- Framework fixtures: positive and negative fixtures for supported or proposed
  family boundaries.
- `UNKNOWN` regressions: cases where dynamic, stale, ambiguous, or unsupported
  behavior should remain `UNKNOWN`, or cases where a focused analyzer
  improvement can reduce `UNKNOWN` without creating false certainty.
- Installer reports: source-checkout, release, npm, Codex, and Claude Code
  install plans or failures with sanitized logs.
- Release validation: local validation of release artifacts, npm package shape,
  installer dry-runs, and source-checkout dogfood.

## Fixture Requirements

Framework-support changes need both positive and negative fixtures.

Positive fixtures should:

- use small repo-like examples, not isolated one-line snippets;
- show at least three compatible support members when a family claim is
  expected;
- include exact local anchors and stable expected outputs;
- verify family evidence, read plans, and source-free default output where
  relevant.

Negative fixtures should:

- cover lookalikes, dynamic behavior, unsupported wrappers, stale evidence,
  ambiguous imports or fixtures, and low-support examples;
- prove unsupported cases produce typed `UNKNOWN`, fallback, or no family row;
- prevent package/config presence from becoming support evidence by itself;
- avoid broad fixtures that make failures hard to classify.

## Preserve UNKNOWN

Do not weaken `UNKNOWN` behavior to make a demo look better. An analyzer change
that reduces an `UNKNOWN` must introduce stronger replacement evidence and
tests that control false certainty. Unknown-rate reduction is not a quality
claim unless false certainty is measured or controlled.

## Do Not Overclaim

Do not describe RepoGrammar as production-ready, a sound static analyzer, a
universal repo-understanding engine, full JS/TS semantic analysis, React
support, full Rust support, or a measured token-saving tool. Token-saving claims
require paired baseline/treatment evidence.

Do not describe npm or release installation as available until the exact package
or release asset has been verified.

## Validation

Run the focused checks for your change and, before a complete PR, run:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
node src/workers/typescript/worker.test.js
node src/npm/repogrammar.test.js
npm_config_cache="${TMPDIR:-/tmp}/repogrammar-npm-cache" npm pack --dry-run
python3 src/workers/python/worker.test.py
bash src/install/repogrammar-install.test.sh
cargo run --quiet --bin repo-guard -- check
git diff --check
cmp -s AGENTS.md CLAUDE.md
```

Report any command you could not run and why.

## Commit Rules

- Inspect `git status` before editing.
- Make the smallest coherent change.
- Stage only relevant files.
- Avoid unrelated formatting, generated files, logs, `.repogrammar/`, and temp
  files.
- Use the repository's existing style and Conventional Commits, for example
  `docs: clarify preview install caveats`.
- Keep tests and documentation in the same commit as behavior changes.
- Do not rewrite shared history or push unless explicitly authorized.
- After a PR or integration branch is merged to `main`, delete the merged branch
  once containment or patch-equivalence is verified. Do not delete stale-looking
  stacked branches based on ancestry alone.
