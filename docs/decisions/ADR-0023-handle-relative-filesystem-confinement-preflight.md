# ADR-0023: Handle-relative filesystem confinement preflight

- Status: Accepted
- Date: 2026-07-16
- Scope: cross-platform repository traversal and source access; decision-only
  preflight
- Refines: ADR-0008
- Related: `docs/architecture/dependency-rules.md`,
  `docs/specifications/indexing-pipeline.md`,
  `docs/specifications/storage.md`, and
  `docs/reports/filesystem-confinement-completion-review.md`

## Context

RepoGrammar currently rejects filesystem objects observed as symlinks and
canonical paths observed outside the repository. Discovery, source-store
reads, and the autosync change fingerprint nevertheless canonicalize a path and
later reopen or inspect that pathname. An attacker who can rename or replace
tree entries between those operations can invalidate the earlier observation.
On Unix this includes symlink swaps; on Windows it includes junction and other
reparse-point swaps.

The fixed aggregate discovery and fingerprint ceilings bound work and retained
output. They do not authenticate the filesystem object opened after a path
check and therefore do not close this time-of-check/time-of-use (TOCTOU) gap.
The Rust standard-library filesystem documentation likewise warns that path
metadata checks are subject to TOCTOU and recommends retaining open files. Its
experimental `std::fs::Dir` is still nightly-only and may fall back to an
absolute pathname on unsupported systems, where it makes no TOCTOU guarantee.

The affected current paths are:

- `FilesystemFileDiscovery::discover`, `discover_files`,
  `discover_files_with_limits`, and `DiscoveryState::{walk,
  visit_directory, visit_file}` in
  `src/rust/adapters/filesystem/discovery.rs`;
- `FilesystemSourceStore::read_source`, `read_repository_source`,
  `validate_repo_relative_path`, and `repo_relative_path` in
  `src/rust/adapters/filesystem/source_store.rs`;
- `repository_change_fingerprint`,
  `repository_change_fingerprint_with_limits`, and
  `FingerprintState::walk` in
  `src/rust/adapters/filesystem/change_fingerprint.rs`; and
- `read_file_bounded` in `src/rust/adapters/filesystem/bounded_read.rs`, which
  reopens a pathname before delegating to the already generic `read_bounded`.

This ADR fixes the required security invariant, candidate qualification, and
atomic migration sequence. It does not add a dependency or change runtime
behavior. The confinement issue remains open until the completion review links
all required implementation and platform evidence.

## Decision

### D1. Retained directory handles are the access authority

After the repository root is atomically opened and pinned, all repository
enumeration, child opens, metadata, content reads, content hashes, and autosync
fingerprint observations must be relative to retained directory handles. A
repository-relative pathname is an identifier for reporting, ignore queries,
storage, and deterministic ordering; it is not access authority.

Every repository-relative access path must pass the shared lexical validator
and then split into `std::path::Component::Normal` one-component names. Each
relative directory or file open receives exactly one such name. Multi-component
child/file opens are forbidden because final-component no-follow does not
protect an intermediate component. Discovery validates each enumerated child
name under the same one-component rule. Source-store access must walk every
parent component from the root handle with a relative no-follow directory open,
then open only the final one-component file name with no-follow semantics.

A directory is accepted only from a successful relative no-follow directory
open. A file is accepted only from a successful relative read-only no-follow
open whose qualified options cannot block indefinitely when an attacker swaps
the candidate to a FIFO, device, or other special file before open. Metadata,
obtained from that opened handle, must then prove it is a regular file. File
metadata and bytes come from that same opened file handle. Discovery hashes the
bytes actually returned by its bounded read. Source-store expected-hash
comparison remains the ordinary-mutation detector after that same-handle read.
The autosync fingerprint remains a point-in-time change hint, not a snapshot-
consistency claim.

There is no ambient-path or canonicalize-and-reopen fallback after the root
handle is pinned. An unsupported relative operation, no-follow result, object
type, filename, or platform condition fails closed under the consumer's
existing typed error/skip contract; it must not retry through `std::fs` paths.
The implementation must not add unbounded retry on rename races.

### D2. Root pinning is part of the proof

Opening an arbitrary multi-component repository pathname with normal follow
semantics and then wrapping the returned directory is insufficient. Root
bootstrap must be qualified for ordinary relative and absolute repository
paths and filesystem/volume roots:

- a relative path starts from one pinned ambient current-directory handle;
- an absolute path starts from the corresponding pinned filesystem or volume
  root handle; and
- each non-root path component, including the requested repository root's
  final component, is opened relative to its retained parent with no-follow
  semantics.

Platform prefix parsing is bootstrap-only. It must not become a string-prefix
containment test. Windows drive, UNC, and volume-prefix behavior must be
specified from actual supported APIs and fixtures; unsupported or unavailable
root forms must be rejected explicitly. A filesystem or volume root with no
descended final component must still be opened as a directory handle and
covered by runtime evidence.

Path authentication before the first bootstrap handle opens is outside this
ADR's claim. Once that starting handle exists, replacing the original root
pathname must not redirect later repository access.

If final-component no-follow root opening cannot be proved on every required
target, this design is `NO_GO`; a partial platform implementation must not be
merged as filesystem confinement.

### D3. Keep the handle abstraction private to the adapter

The future implementation uses one private filesystem-adapter abstraction,
provisionally named `RepositoryRootHandle`. It owns the pinned root and the
relative open/enumeration policy. Concrete capability crate types, OS handles,
and platform path types must not cross `ports`, `application`, CLI, MCP, or
storage contracts.

A traversal work item contains only:

- a retained directory handle;
- its normalized RepoGrammar repository-relative reporting path; and
- the already bounded directory depth.

Enumeration is handle-relative and incremental. The walker charges the existing
visited-entry budget before retaining each child name, retains only the bounded
names needed for deterministic sorting, sorts those names with the existing
ordering contract, and then reopens each child relative to the parent handle.
`DirEntry` or equivalent file-type metadata is advisory only and must never
authorize recursion or reading. Directories use a relative no-follow directory
open with one validated name. Files use one validated name and a relative read-
only open configured not to follow the final component and not to hang on a
special-file replacement; regular-file confirmation follows from the returned
handle. No adapter method accepts a multi-component descendant path for an
open.

The existing `read_bounded<R: Read>` remains the byte-budget authority. The
pathname-opening `read_file_bounded` entrypoint must not be used by the migrated
consumers. A private handle-oriented helper may adapt an already opened file to
`read_bounded`, but it must not perform another open.

Native Git ignore probing may remain subprocess- and pathname-based because it
is exclusion advice, not access authority. Its result may cause an already
handle-observed candidate to be omitted. It must never authorize an open,
recursion, metadata read, content read, or retry, and Git-unavailable behavior
retains the existing strict/fallback policy.

### D4. The three consumers migrate atomically

Discovery, source-store reads, and autosync fingerprinting must move to the
shared handle invariant in one coherent implementation series before the
security issue can be closed. Partial migration is a stop condition because an
unmigrated consumer would retain an externally reachable reopen path.

The migration preserves existing public contracts:

- fixed file, byte, skip, visited-entry, and depth budgets and their check
  order;
- incremental enumeration, bounded child-name retention, deterministic output,
  checked arithmetic, and no partial discovery/fingerprint output;
- inclusive per-file bounded reads via `read_bounded`;
- repository-relative path validation, source-free discovery/fingerprint
  reports, no source snippets or absolute-path leakage in errors/reports, and
  path-free aggregate-limit errors;
- discovery content hashes over the exact accepted bytes;
- source-store expected-hash mismatch behavior for ordinary mutation; and
- the autosync fingerprint's role as a conservative point-in-time hint.

No retry loop attempts to create a snapshot. Content changed through another
already-open handle is outside the confinement claim. Callers continue to rely
on hashes, generation validation, and later freshness checks for mutation.

### D5. `cap-std` plus `cap-fs-ext` is a candidate, not an admission

The preferred candidate after qualification is the mutually compatible pair:

- `cap-std` `4.0.2`, exact published crate SHA-256
  `7281235d6e96d3544ca18bba9049be92f4190f8d923e3caef1b5f66cfa752608`;
  and
- `cap-fs-ext` `4.0.2`, exact published crate SHA-256
  `d78e5a3368ae89b7cb68186411452b4b9fac8b41be9c19bf3f47c2d2c8e36e6b`.

Both packages record upstream commit
`715e4ed607ae9a93c7446b0fa63296f7898831c2`, were non-yanked in the crates.io
index when checked on 2026-07-16, and declare
`Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT`. The candidate feature
profile is exact `cap-std` with default features disabled and exact
`cap-fs-ext` with default features disabled plus only `std`. The qualification
must prove that Cargo resolves one compatible `cap-std`/`cap-primitives` line;
this ADR does not authorize those manifest entries or freeze the transitive
lockfile.

The relevant candidate APIs are `cap_std::fs::Dir`, handle-relative directory
enumeration/opening, `cap_fs_ext::DirExt::open_dir_nofollow`, and
`cap_fs_ext::OpenOptionsFollowExt` with
`cap_fs_ext::FollowSymlinks::No`. `cap_std::fs::File` exposes metadata and
`Read`, permitting same-handle validation and content consumption. These API
shapes are necessary but not target-semantics proof.

Both candidate crates contain a `build.rs` that probes compiler features by
invoking the configured Rust compiler. Neither packaged manifest declares a
`rust-version`. Dependency admission must therefore record the resolved build-
script surface and prove the exact candidate against RepoGrammar's stable
toolchain policy rather than infer an MSRV. The current checkout used Rust
1.96.0 for this preflight, but that local observation is not an upstream MSRV
claim.

The known `cap-primitives` Windows device-name advisory
RUSTSEC-2024-0445 is patched in `>=3.4.1`; candidate `4.0.2` is beyond the
published fixed boundary. This does not replace a current full resolved-tree
advisory scan at dependency admission.

### D6. Dependency and platform qualification precede `Cargo.toml`

Before any production dependency is added, an evidence-only qualification
must record:

1. exact direct and transitive versions, crates.io checksums, upstream commits,
   licenses, selected features, duplicate versions, build scripts, native code,
   proc macros, and dependency `unsafe` surface; the RepoGrammar implementation
   diff itself must contain no new `unsafe` block or custom platform FFI;
2. declared MSRV when present and compile proof against the repository's
   supported stable toolchain when absent;
3. current RustSec/advisory results plus disposition of every finding;
4. source review proving the required operations do not contain an ambient-
   path fallback or multi-component descendant open on the selected targets;
5. compile proof for
   `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
   `x86_64-apple-darwin`, `aarch64-apple-darwin`, and
   `x86_64-pc-windows-msvc`; and
6. native Linux, macOS, and Windows runtime proof for no-follow root bootstrap,
   one-component handle-relative enumeration/opening, regular-file
   verification, confinement, and the deterministic adversarial fixtures in
   D8; and
7. source and native runtime proof that final file-open options are nonblocking
   or otherwise special-file-safe before metadata validation, so a FIFO,
   device, or relevant reparse replacement returns a bounded rejection instead
   of hanging.

The runtime matrix must include ordinary repository paths and available
filesystem/volume roots and prefixes. Windows proof must exercise junctions
and other relevant reparse points, not only developer-mode symbolic links.
Unix proof must exercise symbolic links. Cross-compilation, docs.rs builds,
API documentation, or passing on one operating system is auxiliary evidence
only.

Qualification stops with `NO_GO` on a target-semantic gap, inability to prove
final root no-follow, ambient fallback, unacceptable dependency/build-script/
advisory/license surface, a multi-component descendant open, missing native
runtime evidence, inability to prove bounded return for relevant special-file
replacement, or a design that requires custom unsafe/platform FFI in
RepoGrammar. A target may not silently fall back to path containment.

### D7. Rejected alternatives

- **Stable standard library only:** rejected for this implementation horizon.
  The handle-relative `std::fs::Dir` API is nightly-only, and its documented
  fallback may store an absolute path without TOCTOU guarantees.
- **Linux `rustix`/`openat2` as the authority:** rejected as the cross-platform
  contract. Linux `openat2` offers valuable `RESOLVE_BENEATH` and
  `RESOLVE_NO_SYMLINKS` semantics but is Linux-specific. It may become a later
  optimization only behind the identical tested invariant and without changing
  errors or fallback safety.
- **Custom OS FFI or unsafe handle walking:** rejected. RepoGrammar will not
  own a bespoke Unix/Windows filesystem-security substrate for this change.
- **Canonical path or string-prefix containment:** rejected because it checks a
  name at one instant rather than retaining authority to the opened object.
- **Windows `GetFinalPathNameByHandle` prefix comparison:** rejected as the
  access decision. It reports a resolved name after an open and has volume,
  normalized/opened-name, network-share, and permission behavior; it does not
  replace relative no-follow access from a retained parent.
- **Best-effort path fallback:** rejected on every target. Unsupported semantics
  are a fail-closed error or `NO_GO`, not permission to reopen an ambient path.

### D8. Deterministic adversarial evidence is mandatory

Tests must use explicit barriers or test-only hooks at the operation boundary;
timing sleeps and probabilistic race loops are not acceptance evidence. The
outside fixture contains a sentinel opener/read detector so a test proves not
only that output omitted the file, but that the outside target was never
opened.

The implementation and native-platform suites must cover:

- an in-root file replaced by an outside-pointing symlink/reparse point after
  enumeration and before the file open;
- an in-root file replaced by a FIFO before final open on Unix, proving bounded
  return/no hang, plus relevant Windows device/reparse equivalents;
- an in-root directory replaced before recursive open;
- an already present in-root symlink/reparse point;
- source-store outside-target denial and expected-hash mismatch for ordinary
  same-file mutation, including a multi-component source path whose parent is
  swapped before its one-component no-follow descent;
- replacement of the original root pathname after the root is pinned, proving
  later component opens remain based on the pinned handle rather than the
  replacement pathname, without claiming stability across mount changes;
- Windows directory junction/reparse swaps and Unix symbolic-link swaps;
- exact/plus-one file, byte, skip, visited-entry, and depth gates without
  unbounded handle or name retention;
- non-UTF-8 names where the platform supports them, unreadable/open failures,
  non-regular files, and disappearing entries;
- fingerprint identity and deterministic ordering under the same handle policy;
  and
- proof that no rejected case uses an ambient/pathname fallback.

The test hook must be private and test-only. It may coordinate replacement
before a relative open; it must not weaken production ordering or become an
injected production policy.

### D9. Non-goals and claim boundary

This decision does not attempt to prevent or detect:

- hard-linked objects or mounted/bind-mounted descendants reachable through
  accepted entries, including backing objects outside the initial filesystem,
  device, or inode ancestry;
- concurrent mount, unmount, bind-mount, or other mount-topology changes and
  the physical device/inode origin of an accepted object;
- content changes made through another open handle while RepoGrammar reads;
- malicious kernel, filesystem driver, or storage hardware behavior; or
- manipulation before the first bootstrap handle is opened.

It does not create a filesystem snapshot, transactional repository view, or
proof that the autosync fingerprint and a later index saw identical content.
It does not change ignore semantics, public paths, storage schemas, CLI/MCP
contracts, or resource budgets.

The completed claim is deliberately narrow: the qualified handle policy blocks
redirection through symlink/reparse components and covered pathname
rename/replacement races. It does not prove that a retained handle names a
stable namespace under concurrent mount-topology mutation or that accepted
objects share a physical filesystem origin.

Until every D6-D8 gate and the completion audit passes, documentation must say
that concurrent filesystem confinement is incomplete. The accepted design,
package metadata, and aggregate resource bounds are not a fix or a safety
claim.

### D10. Atomic implementation sequence

The work proceeds in six gated stages:

1. **Decision:** accept this invariant, non-goals, candidate, stop conditions,
   and completion review structure without runtime changes.
2. **Evidence-only qualification:** freeze package/source/advisory metadata and
   obtain five-target compile plus three-OS native runtime results outside the
   production manifest. Stop on any D6 failure.
3. **Dependency and handle abstraction admission:** add only the qualified exact
   dependency/feature set and the private `RepositoryRootHandle` bootstrap,
   open, enumeration, and same-handle bounded-read primitives with focused
   tests. No consumer may claim confinement yet.
4. **Simultaneous consumer migration:** move discovery, source store, and
   fingerprint to the shared handle authority in one coherent branch/series;
   remove their canonicalize-then-reopen access paths.
5. **Cross-platform review and fixes:** run deterministic native adversarial,
   resource, correctness, security, completeness, and performance reviews;
   fix findings without adding path fallback.
6. **Completion audit:** link exact commits, resolved dependency evidence,
   five-target compile results, three-OS runtime artifacts, tests, and full
   repository gates. Only this stage may close the limitation.

Each implementation stage requires tests and synchronized documentation in the
same atomic commit. Any partial migration, missing runtime platform, unresolved
security finding, or failed required gate leaves the completion review
`Incomplete`.

## Consequences

- Repository access gains one auditable authority instead of three similar
  pathname-check sequences.
- Retained handles consume resources proportional to bounded traversal depth;
  the existing depth and entry ceilings remain mandatory.
- Deterministic sorting retains bounded child names, not pathname-derived
  metadata or open handles for the whole repository.
- A qualified dependency adds supply-chain and build-script surface, so exact
  resolution and platform evidence are prerequisites rather than follow-up.
- Unsupported platform behavior fails closed. Cross-platform parity takes
  precedence over accepting a Linux-only partial security claim.

## Verification sources

The preflight used primary or package-authoritative sources current on
2026-07-16:

- [Rust nightly filesystem TOCTOU guidance](https://doc.rust-lang.org/nightly/std/fs/)
  and the [experimental `std::fs::Dir` platform behavior](https://doc.rust-lang.org/nightly/std/fs/struct.Dir.html);
- [`cap-std` 4.0.2 package source and metadata](https://docs.rs/crate/cap-std/4.0.2/source/),
  [`cap_std::fs::Dir`](https://docs.rs/cap-std/4.0.2/cap_std/fs/struct.Dir.html),
  [`cap_std::fs::File`](https://docs.rs/cap-std/4.0.2/cap_std/fs/struct.File.html),
  and [feature list](https://docs.rs/crate/cap-std/4.0.2/features);
- [`cap-fs-ext` 4.0.2 package source and metadata](https://docs.rs/crate/cap-fs-ext/4.0.2/source/),
  [`DirExt::open_dir_nofollow`](https://docs.rs/cap-fs-ext/4.0.2/cap_fs_ext/trait.DirExt.html),
  [`OpenOptionsFollowExt`](https://docs.rs/cap-fs-ext/4.0.2/cap_fs_ext/trait.OpenOptionsFollowExt.html),
  [`FollowSymlinks`](https://docs.rs/cap-fs-ext/4.0.2/cap_fs_ext/enum.FollowSymlinks.html),
  and [feature list](https://docs.rs/crate/cap-fs-ext/4.0.2/features);
- [upstream commit recorded by both published crates](https://github.com/bytecodealliance/cap-std/commit/715e4ed607ae9a93c7446b0fa63296f7898831c2);
- [Linux `openat2(2)`](https://man7.org/linux/man-pages/man2/openat2.2.html)
  for Linux-only beneath/no-symlink comparison;
- [Apple `open(2)`](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/open.2.html)
  for final-component `O_NOFOLLOW` semantics;
- [Windows `CreateFile` flags](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)
  and [`GetFinalPathNameByHandleW`](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfinalpathnamebyhandlew)
  for the reparse/open and rejected path-reporting boundaries; and
- [RUSTSEC-2024-0445](https://rustsec.org/advisories/RUSTSEC-2024-0445.html)
  for the known patched Windows device-name issue in `cap-primitives`.

Crates.io sparse-index records and downloaded published crate archives were
used to verify the exact direct-package checksums and packaged VCS commit. The
future qualification must reproduce and archive those checks rather than rely
on this prose.

## Follow-up

- Keep `docs/reports/filesystem-confinement-completion-review.md` incomplete
  until the six-stage ledger and all platform evidence are complete.
- Do not add `cap-std`, `cap-fs-ext`, or another production dependency during
  the decision or evidence-only stages.
- Do not close or weaken the concurrent-filesystem limitation based on this ADR
  alone.
