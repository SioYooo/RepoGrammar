# ADR-0028: Ownership-aware product uninstall and agent disconnect

- Status: Accepted
- Date: 2026-07-21

## Context

Before this decision, `repogrammar uninstall` removed only receipt-owned
coding-agent MCP registrations and managed instruction sections. The installed
command, first-party authority binary, bundled workers, and installer-owned
directories had to be removed through wrapper-specific paths. That split made
the obvious command name incomplete, allowed wrappers to grow separate deletion
rules, and provided no durable product-level ownership proof.

Self-removal adds stricter problems than agent disconnection. The running
executable cannot be assumed removable on every platform; legacy installs may
not have a product receipt; a command can be a symlink, copy, package-manager
launcher, or unrelated binary; and a failure after agent removal can otherwise
leave a broken native MCP entry or an unreported partial installation.

The machine-installation boundary must also remain separate from repository
state. No global operation may discover or remove repository-local
`.repogrammar/` directories.

## Decision

Make the public lifecycle commands distinct:

```text
repogrammar disconnect [--target <agent[,agent]>]
                       [--scope global|project-local]
                       [--dry-run] [--yes] [--print-config [agent]] [--json]

repogrammar uninstall [--dry-run] [--yes] [--json]
```

`disconnect` is the receipt-backed inverse of agent installation. It preserves
the prior target selection, native configuration ownership checks, managed
instruction rollback, and transactional removal behavior. It never removes the
RepoGrammar product or repository state.

Bare `uninstall` removes the first-party managed machine installation. This is
a pre-1.0 breaking CLI correction: `uninstall --target ...` is rejected with
the exact migration direction to use `repogrammar disconnect --target ...
--yes`; it is never interpreted as product scope or silently mapped to old
semantics.

### Product ownership receipt

Every first-party shell, PowerShell, and direct Rust install/update path uses the
Rust ownership service to atomically write:

```text
$DATA_DIR/receipts/product-install.json
```

The schema records its version, `managed_by`, installation kind, product
version, absolute data directory, managed authority path and SHA-256, command
path/kind and SHA-256, and every owned bundled worker path and SHA-256. Receipt
and staging paths must be regular non-symlink files inside the deterministic
first-party layout. The writer uses create-new private staging and atomic
activation; existing malformed or foreign ownership evidence fails closed.

An existing installation without this receipt may be inferred only when all
legacy evidence is unambiguous: the authority is exactly
`$DATA_DIR/bin/repogrammar` (or the platform executable spelling), the command
is that authority, an exact symlink to it, or a byte-identical regular copy, the
running executable is the authority, the already-validated managed command, or
a byte-identical regular copy, and a first-party worker exists at a
deterministic managed location. Any ambiguity causes zero deletion and
the recovery action is to reinstall once, thereby creating the receipt, then
rerun `uninstall`.

### Preflight and commit boundary

Product uninstall completes every product, command, worker, receipt, native MCP,
instruction-file, and helper-capability preflight before the first mutation.
Foreign, malformed, drifted, escaped, symlink-traversed, or non-regular state
fails closed. Truly absent unowned agent integrations are no-ops; any native
RepoGrammar entry that cannot be proved owned blocks the whole uninstall.

Live removal uses a private post-exit helper copied from the validated managed
authority. The parent creates a create-new, schema-validated cleanup plan with
restrictive Unix permissions. The plan contains only paths re-derived from the
validated installation layout plus deletion-time hashes and file/directory
identities; the hidden finalizer cannot accept caller-selected deletion paths.

The handoff is a three-part barrier:

1. the helper validates the plan, its own identity, and every planned product
   file, then emits the exact `READY` handshake;
2. only after `READY` does the parent transactionally remove owned agent
   integrations and send the one-shot `COMMIT` capability;
3. lifecycle-channel `EOF` and proof that the exact parent PID exited are both
   required before the helper revalidates and deletes product files.

The command, workers, product receipt, and authority are deleted in a bounded
order with the managed authority last. Only then may now-empty, known
RepoGrammar-owned installation directories be removed. Already absent owned
files are idempotent success. A changed identity, removal error, or non-empty
directory is preserved and reported, never broadened into recursive cleanup.
On Unix, deletion is bound to an identity-checked open parent directory: each
leaf is moved with `renameat` to an unpredictable same-directory quarantine
name, revalidated there, and removed with `unlinkat`. This prevents a
check-then-unlink path replacement from deleting a foreign leaf. The direct
`libc` dependency exists only for these POSIX handle-relative operations.

### Outcome and rollback semantics

Dry-run performs the ownership and agent-state preflight and returns the planned
paths with zero writes. A live parent can report only `finalizer_pending` and a
structured `report_path`; it cannot claim synchronous completion. The finalizer
report is authoritative and records `complete` or `partial`, removed,
preserved, failed, residual copies, and manual recovery.

Agent removal failure aborts before product cleanup. Helper spawn or `READY`
failure leaves agent and product state unchanged. Failure to commit the helper
triggers best-effort rollback of agent mutations; rollback failure is itself
reported. Once committed cleanup begins, any later failure produces a partial
report and a non-success finalizer result.

Product uninstall deliberately preserves:

- every repository-local `.repogrammar/` directory;
- telemetry preferences, research traces, experiment records, and unknown
  global files;
- npm/npx, Cargo, source-checkout, package-manager, and unmanaged PATH copies.

Residual executable copies may be reported, but this command does not invoke
their package managers. Repository state is removed only by an explicit
`repogrammar uninit --project <path> --yes` for each repository.

First-party wrappers retain acquisition, checksum, archive-validation, and
install-layout responsibilities. Agent-only actions delegate to `disconnect`;
complete removal delegates to `uninstall`. Deprecated command-only removal
prints migration guidance and never directly deletes product files.

## Alternatives considered

- Keep agent removal under `uninstall` and add `--product`: rejected because the
  obvious bare command would remain incomplete and the two ownership domains
  would continue sharing one overloaded option parser.
- Add `uninstall --all`: rejected because it is easily confused with the old
  `--target all` agent selector.
- Add a top-level `purge`: rejected because first-party acquisition remains in
  the install/uninstall lifecycle.
- Infer ownership from a binary name, PATH position, or matching bytes alone:
  rejected because those facts do not prove the full managed layout.
- Delete the running authority inline: rejected because operating-system file
  locking and partial-failure behavior require a post-exit boundary.
- Delete repository indexes with the machine product: rejected because it
  violates the explicit per-repository authorization and privacy boundary.

## Consequences

- The rename is a documented pre-1.0 CLI breaking change and requires a
  patch-forward release; immutable `v0.4.0` artifacts do not gain this behavior
  merely because `main` documents it.
- Product installation and agent integration now have independent request,
  runtime, receipt, and application-service contracts.
- First-party legacy users may need one reinstall before ownership-safe product
  uninstall is available.
- The private helper workspace and structured report remain outside the managed
  data directory so evidence survives product removal; they are not an
  additional source of deletion authority.
- Product uninstall means only that proven first-party managed machine assets
  were removed. It does not mean package-manager copies, user data, or
  repository state were removed.
