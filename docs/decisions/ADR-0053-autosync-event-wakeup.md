# ADR-0053: Native filesystem events wake auto-sync

- Status: Accepted
- Date: 2026-09-09
- Complements: ADR-0027; changes no freshness or filesystem-confinement claim.

## Context

Periodic recursive metadata scans consume work even when a repository is idle.
Rust's standard library has no portable native filesystem notification API and
no existing production dependency provides one. Hand-written platform FFI would
add a second operating-system abstraction and unsafe code.

## Decision

Accept exact `notify` 8.2.0, with default features disabled and only
`macos_fsevent` enabled, behind the filesystem adapter. Cargo.lock records the
resolved package versions and checksums. Use the recommended native backend
(FSEvents on macOS and inotify on Linux), disable recursive symlink following
where supported by the backend, and retain periodic authoritative fingerprint
reconciliation in the composition root. The FSEvents backend does not consume
that configuration option; no watcher configuration establishes confinement.
Windows support remains deferred; target-specific transitive packages in the
lockfile do not expand the supported platform matrix.

The adapter retains one bounded notification token, never source bytes or a
queue of event paths. A full token channel coalesces changes into the already
pending whole-repository reconciliation. Each wait consumes at most one token;
continuous events cannot trap the caller in a drain loop. Native overflow/rescan
flags and backend errors set a persistent failure latch, even when the token
channel is full. Initialization failure or the latch must make the caller
reconcile and use its visible polling fallback. Raw backend errors and absolute
paths do not enter public diagnostics.

Access-only events are discarded. File creation, removal, and data-change
events reuse the supported-language path classifier to suppress unsupported
files. Directory, rename, metadata, and unknown events remain conservative.
Existing shared default-excluded directories and `.repogrammar`/`.repogrammar-*`
state directories are suppressed. Cross-boundary renames retain
the relevant endpoint. Unknown or pathless events request reconciliation.
Language-specific excluded directories are not globally suppressed: `.build`
can contain a supported TypeScript source. The watcher caches the shared Git helper's bounded ignored-directory result;
that helper preserves directories containing tracked descendants. No callback
runs Git. Ignore-control or Git metadata events invalidate the cache before
exclusion, so index changes (including force-added tracked files) wake the
caller. Refresh before reconciliation; Git failure clears exclusions, and a
generation check prevents a concurrent policy event from publishing an old
result. External global/worktree Git metadata changes are not all observable
from this repository watcher and remain covered by periodic refresh and
reconciliation. Git remains the ignore authority; no glob matcher is added.

Native notifications are hints, not a journal, a snapshot, or freshness proof.
Missing notifications on network filesystems, backend-specific behavior, root
replacement, and external Git configuration are covered by periodic
reconciliation, not by claiming complete delivery. Existing content-hash and
active-generation checks remain authoritative. This does not implement or
weaken ADR-0023's pending handle-relative filesystem confinement work.

## Alternatives and consequences

Keeping short-interval scans alone preserves avoidable idle traversal. An
unbounded event queue risks memory growth during builds. A new debounce crate
is unnecessary because auto-sync already owns scheduling. The selected adapter
adds one direct dependency and its platform backends; recursive watcher
registration can still consume resources in large trees, and callback filtering
does not remove operating-system watches on ignored directories. Polling remains
the fallback if registration fails.

Tests cover native temporary-file creation, shared exclusion policy, cross-boundary
renames, coalescing, and overflow/error preservation. Native Linux and macOS CI
must pass before release. Local event tests establish event delivery on that host;
they do not establish battery savings. Report any CPU or battery results with
the actual workload and observation interval.

## Primary references

- [notify 8.2.0 platform behavior and limitations](https://docs.rs/notify/8.2.0/notify/)
- [Event rescan contract](https://docs.rs/notify/8.2.0/notify/struct.Event.html#method.need_rescan)

## Follow-up

Keep periodic reconciliation and visible fallback covered in daemon tests. Record
native platform CI and any measured idle-work comparison with release evidence.
