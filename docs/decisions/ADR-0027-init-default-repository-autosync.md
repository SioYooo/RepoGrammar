# ADR-0027: Init starts repository auto-sync by default

- Status: Accepted
- Date: 2026-07-19
- Supersedes: The opt-in auto-sync default in ADR-0008; the repo-local state
  boundary itself remains accepted.

## Context

RepoGrammar already treats `init` as the one-command repository bootstrap: it
creates repo-local state and builds a readable active generation. ADR-0026 later
made `setup` start auto-sync by default, but standalone `init` retained the
older `--autosync` opt-in. That split leaves a normally initialized repository
fresh only until the next edit and requires users to understand a lifecycle
detail the product can handle safely after indexing succeeds.

The original ADR-0008 concern was that family mining should not eagerly rebuild
repositories before a safe incremental lifecycle existed. RepoGrammar now has a
repo-local daemon, bounded change fingerprinting, debouncing, incremental sync
with conservative full-rebuild fallback, atomic generation activation, typed
startup readiness, and explicit stop/disable commands. The old default is no
longer the best user journey.

## Decision

`repogrammar init` performs this ordered default bootstrap:

1. create or repair repository-local state;
2. build or refresh the active generation through the normal resync path;
3. start the repository-local auto-sync daemon only after that generation is
   readable.

`--no-autosync` is the explicit opt-out for CI, experiments, packaging,
one-shot indexing, and users who do not want a background process. The existing
`--autosync` spelling remains accepted as a compatibility-friendly explicit
request for the default. Supplying both flags is an error before any write.

`--state-only` remains the lifecycle-repair boundary: it neither indexes nor
starts auto-sync. `--state-only --autosync` and `--state-only --resync` remain
invalid before any write; `--state-only --no-autosync` is valid and redundant.

If resync fails, auto-sync is not attempted. If daemon startup fails after a
successful resync, `init` reports a partial failure, preserves the valid active
generation and repo-local state, and recommends `repogrammar autosync start`.

This decision does not create a global repository scanner or operating-system
service. Each initialized repository owns its own `.repogrammar/` index,
configuration, lock, log, and daemon. `install`, `serve`, MCP queries, and query
commands remain unable to initialize repositories or start auto-sync. A user or
agent must still be authorized to run the mutating `init` command. After a
reboot or daemon exit, `repogrammar autosync start --project <path>` remains the
explicit recovery command.

## Alternatives considered

- Keep standalone `init` opt-in while `setup` defaults on: preserves the old
  behavior but keeps two primary onboarding paths semantically inconsistent.
- Add a machine-global daemon that discovers repositories: reduces process
  count but weakens explicit repository authorization, privacy, deletion, and
  worktree isolation.
- Start auto-sync before indexing: shortens the command path but permits a
  daemon without a readable baseline and is rejected.
- Remove `--autosync`: simplifies help but breaks compatible scripts that
  explicitly request it.

## Consequences

Normal repository initialization stays fresh after later edits without an
extra command. Automation that requires deterministic one-shot state must use
`--no-autosync` or `--state-only`. Help, README, quickstarts, agent guidance,
tests, and experiment harnesses must state the new default and opt out wherever
a background worker is not part of the task.

No storage schema, MCP schema, telemetry contract, language-support claim, or
global installation boundary changes.

## Follow-up work

- Keep init/setup default behavior and opt-out terminology aligned.
- Consider a future explicit supervisor for a user-registered repository list;
  it must not discover or scan repositories implicitly.
