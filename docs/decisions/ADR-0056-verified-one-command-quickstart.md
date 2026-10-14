# ADR-0056: Verified one-command bootstrap over existing setup

- Status: Accepted for current source; publication pending
- Date: 2026-10-14
- Extends: ADR-0007, ADR-0026, ADR-0028

## Decision

Add `--quickstart` to `src/install/repogrammar-install.sh`, keeping the source
under `src/` and preserving the separate acquisition, agent integration and
repository lifecycle authorities. No source clone/build, Node/npm, Docker,
model or paid API is part of the binary path. Python 3.10+ remains a prerequisite
for Python analysis; no automatic interpreter installation is authorized.

Shell performs supported-platform/libc admission, bounded release download,
checksum/entry-type/path checks, temporary extraction and a `quickstart-v1`
capability check. It prints the actual resolved Git root and managed paths plus
the existing Rust setup plan. Interactive authorization reads `/dev/tty` with
default-no; stdin can contain the bootstrap script. Noninteractive writes require
`--yes`. Child commands receive null stdin and the terminal descriptor closes
before product activation.

Private binary entrypoints wire the existing install transaction, product receipt
preflight, bundled workers and product MCP self-test. Rust remains the authority
for destination ownership and rollback. Strict product drift/link/path checks
precede writes. Explicit unmanaged-command replacement retains a create-new
backup. Existing receipts and uninstall schemas are unchanged; no alternate
receipt or deletion implementation is introduced. Unrelated PATH copies and
telemetry preferences remain untouched by quickstart.

Setup adds explicit `none` agent selection and `--skip-repository`. Skipped
boundaries are not probed or mutated, and machine-only completion cannot claim
an index, autosync or repository query readiness. Existing fresh/stale indexing,
activation, autosync handshake, recovery and machine-integration rollback remain
owned by their current services. No local agent writer or Windows support is
added.

The human bootstrap plan is bound to execution by a SHA-256 fingerprint of its
scope, repository readiness, concrete agent states/actions and instruction
locations. Dry-run/confirmation mechanics are excluded. The fingerprint is a
same-candidate bootstrap handoff, not a persistent public storage contract. A
fresh plan that differs after confirmation must stop before agent/repository
writes and request review again; a successfully acquired CLI remains installed.

## Security and failure boundary

The bootstrap script runs before it can verify an archive. Its distribution
channel or independently reviewed immutable pin is therefore a separate trust
boundary. A checksum downloaded from the same publisher channel proves byte
agreement, not publisher authenticity. Keep a release-hosted installer and
checksum-before-execution alternative; do not make moving raw-main the only
route.

Product acquisition restores its own snapshots on failure; setup rolls back
only newly created agent integrations. Previously owned agent integrations,
active generations and unrelated files are preserved. The CLI may remain valid
when later indexing/autosync/self-test fails, and the result must say so.
Postactivation ownership drift prevents destructive rollback. Existing limits
against hostile same-directory concurrent rename remain explicit; this change
does not implement ADR-0023's future filesystem confinement work.

For CLI-only product removal, a confirmed unavailable native CLI may be skipped
only when both its exact owned receipt and backup are absent beneath safe
ancestors. The lane is reported `UNKNOWN` and any unreceipted configuration is
preserved, not declared absent; it may still reference the removed product.
Receipt presence, malformed/link/path evidence and all non-NotFound probe errors
remain blockers. Available native clients retain ADR-0028's foreign-entry gate;
opencode is always inspected through its file writer. Recheck the skipped lanes
before execution and refuse if a CLI or receipt appears after planning.

Setup freshness combines the repository dirty-record signal with the existing
hash-checked family-evidence freshness authority. Zero dirty markers alone must
not make changed family evidence ready; stale or unverifiable evidence schedules
the existing resync path. No new freshness classifier or indexing path is added.

## Alternatives rejected

- Bash-native agent/configuration/indexing logic: duplicates Rust authorities.
- Blindly forward `--yes` after an unbound dry-run: a newly detected agent could
  expand the reviewed write scope during the confirmation pause.
- Consume ordinary stdin for confirmation: piped script bytes or EOF could be
  interpreted as answers.
- Use an old stable binary and silently fall back: current-source onboarding and
  instruction capabilities are not present in immutable 0.5.0.
- Delete other PATH executables for convergence: unrelated installations are not
  owned by this quickstart operation.

## Qualification and publication

Require direct and piped real-candidate archive/worker tests, PTY consent and
plan-change refusal, missing authorization, no-agent/no-repository cases,
fresh/stale/repeat/upgrade, ownership drift, malformed archives, native/index/
autosync/self-test failure, rollback, and managed uninstall compatibility.
Mocks alone do not establish product onboarding. macOS native evidence and
simulated platform checks must not be called Linux native validation.

No release/tag/npm mutation is authorized by this ADR. Immutable 0.5.0 remains
unchanged. The [quickstart release checklist](../release/quickstart-release-checklist.md)
requires a new available version, coherent manifests, reviewed source,
four native build/ABI/package gates, exact installer/archive sidecars and a
separately authorized publication before public one-command availability.
