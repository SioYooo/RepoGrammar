# Autosync efficiency verification

Date: 2026-09-09. Scope: the 0.5.0 candidate on macOS arm64.

## Implemented behavior

- Native filesystem hints replace per-second idle fingerprint traversal.
- A one-token channel coalesces events; ignored directories and unsupported
  file writes are filtered before waking the indexing loop.
- Git's ignored-directory inventory prunes wholly untracked ignored subtrees
  before Rust traversal. Tracked descendants and ignore negations remain visible.
- Native mode retains sixty-second metadata reconciliation and ten-second
  lifecycle checks. Watcher failure is visible and switches to bounded polling.
- Failed syncs retain pending changes and retry with bounded backoff. Native
  events request content-hashing sync even when size and mtime are unchanged.

## Evidence

The full Rust suite and dedicated fingerprint, watcher, and schedule regressions
cover coalescing, overflow, narrow retry deadlines, Git failure fallback,
tracked/negation semantics, and real native file creation. The pruning fixture
requires only four visited root entries; its 102 ignored descendants do not
consume Rust traversal depth or entry budgets. This count does not include Git's
internal work.

A separate temporary Git repository was initialized using the actual product
binary with default autosync. After a two-second settling period, writing 100
Python files beneath an ignored `scratch-output/` directory produced no daemon
sync log during the following three seconds. A supported Python file was then
changed from `"ok": 1` to `"ok": 2`, preserving its size and restoring its exact
mtime with `os.utime`. Native autosync reparsed exactly one file and activated
`gen-000002` within the fifteen-second deadline. The daemon was stopped before
the temporary repository was removed.

The source-free result and actual daemon log are in
[autosync-efficiency.summary.json](autosync-efficiency.summary.json).
To reproduce, run the same temporary-repository sequence with the candidate
binary and run `cargo test --workspace --all-features`; the named fingerprint,
change_watcher, and autosync_schedule tests exercise deterministic boundaries.

## Limits

This is correctness and reduced-background-work evidence, not a CPU or battery
benchmark. No percentage of battery savings is claimed. Native Linux behavior
requires the release CI jobs. Native watcher registration may retain watches
under ignored trees; metadata-only fallback cannot detect a same-size,
same-mtime edit without another observable change or explicit sync. Existing
filesystem-confinement limitations remain unchanged.
