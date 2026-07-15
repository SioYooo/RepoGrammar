# ADR-0026: Zero-friction onboarding orchestration

- Status: Accepted
- Date: 2026-07-16

## Context

RepoGrammar currently exposes the safe lifecycle boundaries needed for a
coding-agent workflow, but first-time users must compose them themselves:
acquire the CLI, configure an agent MCP entry, initialize and index each
repository, optionally start autosync, inspect readiness, and translate family
or `UNKNOWN` results into a next action. The individual boundaries are
deliberately conservative, but their composition requires knowledge that should
belong to the product rather than to a new user.

The Build Week usability baseline also shows that top-level help and default
human query output expose the full internal command surface and query-routing
diagnostics. This makes the first interaction depend on lifecycle vocabulary,
family identifiers, and internal pipeline fields even though those details are
not required to ask a pattern question.

This decision defines a zero-decision onboarding path without weakening the
ownership, evidence, freshness, provenance, compatibility, telemetry, or
abstention contracts established by earlier ADRs.

## Decision

### One user-facing orchestration entrypoint

Add this user-facing command:

```text
repogrammar setup [--project <path>] [--target auto|codex|claude-code] [--yes] [--dry-run] [--no-autosync] [--json] [--progress auto|always|never]
```

`setup` is an application-layer orchestration entrypoint, not a new ownership
boundary. It composes existing services directly and must not shell out to the
RepoGrammar CLI. The composition root supplies the existing machine-integration,
repository-lifecycle, autosync, MCP self-test, and readiness capabilities.

The ownership boundaries remain:

- release installers or the npm wrapper acquire the CLI;
- the install application service owns machine-level agent wiring and global
  receipts;
- init, resync, and autosync application services own repo-local state;
- the MCP server remains read-only with exactly one default
  `repogrammar_context` tool;
- telemetry consent remains separate from every lifecycle operation.

`setup` defaults `--project` to the current directory and `--target` to `auto`.
It may choose only an installed agent for which the current binary has a live,
reversible writer. Codex and Claude Code are the only allowed explicit target
names in this first slice; their actual availability is detected at runtime.
The absence of an agent CLI must not destroy the repository-only initialization
path or its valid active generation.

### Plan, confirmation, and execution

Before any write, the application service builds one reviewable plan that
separates:

1. machine-level writes and owned receipts;
2. repo-local state and index writes;
3. the optional background autosync process;
4. telemetry state, which is always off unless separately enabled outside
   `setup`.

Interactive setup asks for one final confirmation after presenting this plan.
`--yes` is the explicit noninteractive confirmation. A live noninteractive run
without `--yes` fails before writes. `--dry-run` is always non-mutating: it
must not create directories, receipts, configuration entries, indexes, daemon
state, telemetry preferences, or temporary files outside an already-existing
process-local scratch area.

After confirmation, setup executes this ordered path:

1. resolve the repository root and inspect existing repo-local state;
2. detect live agent targets and select the requested target;
3. configure the MCP entry through the existing install boundary;
4. persist its RepoGrammar-owned receipt;
5. initialize or refresh the repository through the existing init/resync
   boundary;
6. preserve the atomically activated generation;
7. start autosync unless `--no-autosync` was requested;
8. run the product binary's bounded MCP self-test;
9. derive one completion or recovery action from authoritative readiness;
10. print a compact summary that separates product self-test, native agent,
    repository index, auto-sync, and family-evidence readiness;
11. print a natural-language question only when at least one supported native
    agent integration is verified ready.

Re-running setup is idempotent. It refreshes only state already owned by
RepoGrammar and preserves valid pre-existing machine and repository state.
An owned native entry and receipt that agree with the current managed
executable are `OwnedCurrent` and skipped. If they agree with each other but
point at an obsolete managed executable, they are `OwnedOutdated` and may be
safely refreshed through the existing install reconciliation service. A
foreign entry, malformed state, or receipt/native drift remains preserved and
is not automatically overwritten.

Auto-sync process creation is not readiness. Setup may mark auto-sync ready only
after a bounded parent/child handshake proves that the spawned child wrote the
expected PID and startup nonce after acquiring the daemon lock, while the child
is still alive. Immediate child exit, lock refusal, and timeout are typed
failures.

### Transaction and rollback boundary

Machine-level changes created by the current setup attempt form one reversible
transaction. A native agent failure, receipt failure, repository initialization
or indexing failure, or MCP self-test failure rolls back only machine-level
configuration and receipts newly created by that attempt. Newly configured and
reconfigured pre-existing targets are separate mutation classes: an install-
service refresh must restore its own pre-existing snapshot if it fails, while a
later setup failure must never uninstall that refreshed pre-existing target. It
must not remove a foreign entry, malformed foreign configuration, a
pre-existing receipt, a pre-existing command, or pre-existing repo-local state.

Repository generations retain their existing atomic activation rule. If index
activation fails, the previous valid generation remains active. If indexing
succeeds and autosync start later fails, the new active generation is retained
and the result is a partial success with one recovery action; it is not reported
as total rollback. A failed self-test likewise reports which valid repository
state was retained.

### One authoritative recovery classifier

Create one application-layer recovery classifier consumed by setup completion,
status, doctor, human queries, and MCP recommendations. Callers may format or
serialize its decision but must not re-derive it from raw fields.

The classifier covers at least these action classes, with repository-appropriate
names chosen during implementation:

- setup;
- resync;
- start autosync;
- use source fallback;
- repair storage;
- resolve a lock;
- unsupported;
- no action.

Its inputs include initialization state, active generation availability,
freshness, storage and lock health, autosync configuration and liveness, family
evidence availability, target-specific `UNKNOWN` or stale causes, and agent
wiring status. A ready active index and existing families must not coexist with
a doctor statement that all query or family evidence is deferred. Stale
evidence must produce one consistent recovery action across CLI and MCP.

### Progressive disclosure and compact human output

Default help is a product entrypoint rather than a complete command reference:

- top-level help is at most 25 lines and emphasizes `setup`, `find`, `doctor`,
  and `help --all`;
- `help --all` exposes the existing full command surface;
- command-specific help and advanced commands remain available;
- running with no command remains read-only and must not start setup.

Default human output has these hard caps:

- `families`: at most 15 lines, grouped by language and public pattern role;
- `find`: at most 20 lines;
- `check`: at most 20 lines.

Compact human output must not contain cluster signatures,
`query_pipeline`, `query_candidate_family_ids`,
`query_follow_up_family_ids`, or raw protocol class names. It presents the
conclusion, confidence boundary, most important evidence or read-plan location,
unverified boundary, and one next action. `UNKNOWN` may be explained to humans
as “Cannot verify safely,” but JSON and MCP retain canonical `UNKNOWN` tokens.
`PARTIAL_CONTEXT` must remain explicitly a read plan, not family or conformance
evidence.

Existing JSON fields and MCP operation semantics remain compatible. Detailed
family identifiers and route diagnostics remain available through `--json`, an
explicit verbose/evidence mode, or the existing machine contract. This ADR does
not authorize renaming `repogrammar_context`, adding MCP tools, or adding
top-level graph commands.

### Privacy, scope, and success gates

Setup never enables telemetry. Anonymous telemetry remains explicit opt-in and
default-off, research traces remain separately consented, and no setup output
may disclose source, absolute paths, repository names, symbols, prompts, query
text, credentials, or raw errors.

Until the zero-friction onboarding release candidate is complete, all new
language, framework, parser, semantic-worker, and provider work is frozen. The
slice does not add a GUI, dashboard, local model, embedding store, cloud API, or
production dependency. It does not weaken `UNKNOWN`, `PARTIAL_CONTEXT`,
freshness, provenance, evidence, or compatibility gates.

Implementation is accepted only when automated tests prove:

- setup parsing, single-confirmation behavior, dry-run zero writes,
  idempotency, rollback, and pre-existing-state preservation;
- current-versus-obsolete owned authority reconciliation, bounded auto-sync
  readiness, and family-inventory failure remaining unknown rather than zero;
- agent-missing, receipt/native/index/autosync/self-test failures have typed,
  truthful outcomes and one recovery action;
- setup JSON exposes ready/blocked agent targets, product self-test state,
  agent-query readiness, repository-index readiness, auto-sync readiness,
  family-evidence state, and every limitation; repository-only success emits no
  suggested coding-agent question;
- output line caps and internal-field leakage rules;
- JSON backward compatibility and canonical `UNKNOWN` behavior;
- status, doctor, query, setup, and MCP recovery consistency;
- a clean temporary HOME flow, product MCP self-test, npm argument
  passthrough, and local release-fixture integrity.

A locally verified release candidate is not a published product. The label
`PUBLISHED_JUDGE_READY` is allowed only after a maintainer authorizes and
verifies the remote tag, GitHub Release assets, npm publication, and a clean
published-package onboarding run.

## Alternatives considered

- Keep install, init, and autosync as documented manual steps: preserves the
  current implementation but leaves product lifecycle decisions with every new
  user.
- Make `install` initialize the current repository: shorter superficially, but
  breaks the machine/repository ownership boundary and makes uninstall intent
  ambiguous.
- Make the npm wrapper implement onboarding: would duplicate Rust application
  behavior and create divergent release and source-checkout paths.
- Automatically write on first invocation: removes confirmation and violates
  the explicit-consent boundary.
- Hide uncertainty to make results shorter: improves appearance by weakening
  the product's primary safety contract and is rejected.

## Consequences

The application layer gains an orchestration use case and a shared recovery
decision, while adapters retain their existing ownership. CLI rendering becomes
progressively disclosed, so snapshot and compatibility tests must distinguish
default human output from full JSON/verbose output.

Failures become more explicit because a setup result must report completed,
retained, and rolled-back stages. Local release proof remains useful but cannot
be promoted to an external publication claim.

## Follow-up work

- Implement the staged plan in
  `../plans/build-week-zero-friction-onboarding-plan.md`.
- Synchronize CLI, installation, initialization-progress, architecture, README,
  CHANGELOG, release, and durable-memory documents in the same commits as the
  corresponding behavior.
- Add the completion report and summary JSON only after the required automated
  and full-repository validation has run.
- Keep publication, tagging, pushing, and npm release behind explicit maintainer
  authorization.
