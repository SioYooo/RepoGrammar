# Dependency Rules

## Allowed dependencies

- `core` may depend only on Rust standard library and explicitly accepted domain
  dependencies.
- `ports` may depend on `core`.
- `application` may depend on `core` and `ports`.
- `interfaces` may depend on `application`, `core`, and stable transport-neutral
  types.
- `adapters` may depend on `core`, `ports`, and their concrete external
  libraries.
- `bin` may wire all layers together.
- language-native workers under `src/workers/` may use their native compiler or
  language-service ecosystem behind a versioned protocol.
- optional providers, including any future CodeGraph provider, belong behind
  ports and adapters. Their SDK, CLI, MCP, or file formats must not enter
  `core`, `ports`, or `application` as concrete third-party types.
- reviewed library-contract registration belongs in `core` and may match only
  exact resolved versions under the language-neutral contract. Ecosystem-native
  range parsing or ordering belongs behind a separately qualified adapter; core
  must not compare opaque version text or select an overlapping contract.

## Forbidden dependencies

- `core` must not depend on `interfaces`, `adapters`, `ports`, Tree-sitter,
  SQLite, MCP SDKs, filesystem, process execution, or network access.
- `ports` must not expose Tree-sitter nodes, SQLite rows, MCP payloads, or other
  concrete third-party transport types.
- `application` must not run SQL, inspect Tree-sitter nodes, or parse MCP
  transport payloads directly.
- `interfaces` must not implement pattern-family mining rules.
- `repo_guard` must not call product runtime code.

## Filesystem and Git discovery boundary

Repository traversal, filesystem metadata, symlink inspection, content-hash
calculation, and Git ignore probing belong in filesystem adapters. The
application layer may orchestrate discovery through ports and may classify
deferred indexing outcomes, but it must not expose absolute paths, source text,
or concrete process payloads across the port boundary.

Content hashes must use the accepted `sha2` production dependency rather than a
hand-written SHA-256 implementation. Native Git context such as worktree root
and `info/exclude` location must be resolved through the shared filesystem Git
helper, which shells out to `git rev-parse`; repository lifecycle hygiene must
not manually parse `.git` files when native Git can provide the same fact.

Aggregate filesystem admission belongs to one private adapter policy shared by
full discovery and the autosync metadata fingerprint. The ports layer owns only
the stable fixed default constants plus the transport-neutral typed resource
error; application code maps that error to invalid input and must not duplicate
filesystem counters. Both walkers must consume `read_dir` incrementally, charge
the visited-entry budget before storing a child for deterministic sorting, use
checked arithmetic, and fail without partial output. The fingerprint adapter,
not the composition-root binary, owns recursive fingerprint traversal. The
reported-skipped-path budget applies only to discovery because fingerprinting
does not produce a skipped-path report.

Concurrent filesystem confinement is governed by ADR-0023. After a repository
root is pinned, one private filesystem-adapter authority must enumerate and
open validated `Component::Normal` names one component at a time relative to
retained directory handles, refuse the final symlink/reparse component, reject
special-file replacements without blocking, and obtain regular-file metadata
plus bounded bytes from the same opened file handle. Discovery, source-store
reads, and autosync fingerprinting must migrate together; concrete capability/
OS handle types must not cross `ports` or `application`, and no ambient-path
fallback is allowed.

The candidate `cap-std`/`cap-fs-ext` pair is not an accepted production
dependency until its exact transitive, advisory, build-script, five-target
compile, and Linux/macOS/Windows runtime gates pass.

Go discovery/path-shape policy belongs in `adapters/languages/go.rs`; the file
discovery adapter may use it for stable `go`/`go-config` classification, while
application indexing keeps `.go` inventory-only. Until the authorized source
frontend lands, neither the application nor generic parser may read Go source,
inspect markers, select GOOS/GOARCH, or derive Go-source units/facts/families.
The bounded parser adapter may read only discovered `go.mod`/`go.work` bytes and
return RepoGrammar-owned project-config units, scoped UNKNOWNs, and generic
manifest-declared dependency records; it must not execute Go or resolve a
module graph. `.go` metadata deltas remain incremental and filter all
claim-bearing copy-forward records. Static file-local `go-config` outputs may
also update/copy incrementally. A future source frontend must add its real
cross-file inputs and token-based context invalidation before introducing Go
source semantics. The dated suffix list is discovery metadata, not a core
support authority.

Java project-config classification belongs to the pure normalized-path policy
in `adapters/languages/java.rs`. Exact root/nested `pom.xml` bytes may enter
`adapters/parsing/java/maven.rs`, which owns a bounded static XML subset and
returns only RepoGrammar-owned project-config units, evidence-bound direct
Maven dependency declarations, and `java_dependency_inventory` typed UNKNOWNs.
It must fail closed on malformed/duplicated markup, DTD/entity/CDATA input,
element prefixes, ambiguous coordinates, effective-model obligations, and
resource overflow. It must not resolve parents, profiles, properties,
dependency management/BOMs, classpaths, artifacts, or repositories, and it
must not invoke Maven, Gradle, javac/JDT, plugins, annotation processors,
project/dependency code, child processes, caches, or network access. Because
the accepted subset is file-local and absent from `ParserProjectContext`, its
changed evidence may update incrementally and unchanged rows may copy only with
their unchanged evidence unit.

Ada and Fortran discovery policy belongs in `adapters/languages/ada.rs` and
`adapters/languages/fortran.rs`. Their source and deferred configuration paths
must be rejected before SourceStore dispatch. The bounded parser may read only
supplied exact `alire.toml`/`alire.lock` and `fpm.toml` bytes and return owned
project-config units, scoped UNKNOWNs, and generic manifest-declared dependency
records. It must not evaluate GPR, interpret Alire's internal lock schema, run
GNAT/gprbuild/alr/fpm/compiler/preprocessor commands, execute repository or
dependency code, or resolve a graph. Libadalang and Flang remain qualification
subjects, not accepted production dependencies.

## Native filesystem notification boundary

ADR-0053 accepts exact `notify` 8.2.0 with only `macos_fsevent` enabled.
Concrete native watcher and event types stay in the filesystem adapter. The
composition root receives only a coalesced boolean hint or a source-free typed
failure, retains periodic fingerprint reconciliation, and visibly falls back to
polling when native notification fails. A notification is never index freshness
or filesystem-confinement evidence.

## Tree-sitter boundary

Tree-sitter is the intended universal syntax technology, but parser AST nodes
stay in `src/rust/adapters/parsing/` and language-specific adapter modules.
Adapters convert parser output into `core::model` types before returning
through `ports::parser`. The current Rust production dependencies include
`tree-sitter-rust` for internal Rust self-dogfood extraction,
`tree-sitter-java` for conservative Java/Spring structural extraction, and
`tree-sitter-c`/`tree-sitter-cpp` for bounded C/C++ structural extraction.
Java JUnit/TestNG data-source linking stays inside the Tree-sitter adapter: it
indexes source-visible declarations once per class-like boundary and emits only
structural facts or typed `UNKNOWN`. It must not invoke javac, Maven, Gradle,
annotation processors, test engines, or repository code, and must not add a
dependency merely to resolve runtime/inherited provider behavior.
Direct annotation extraction uses one bounded lexical pass; matching
parentheses are tracked only for at most 64 candidate annotations and 64 KiB of
cumulative segment text, so nested or oversized untrusted input abstains
without repeated suffix scans or source-sized close-index allocation.
C/C++ Tree-sitter nodes stay inside `src/rust/adapters/parsing/cpp/`:
`mod.rs` owns structural scanning and fact emission, `preprocessor.rs` performs a linear,
non-executing pass over Tree-sitter preprocessor nodes for exact unconditional
include evidence, conditional ranges, verified whole-file guards, and ERROR/
macro-boundary helpers, and `test_framework.rs` owns the pure bounded macro-shape
contract plus source-ordered Boost.Test suite validation. Argument validators
scan each identifier, Catch2 tag string, and Boost decorator argument once;
decorator arity checks inspect only direct named children. The suite validator
uses one linear occurrence pass, an explicit `O(depth)` stack, and a
source-ordered issue vector consumed monotonically by `mod.rs`; it never
resolves aliases or copies suite names into case facts. Bounded
`compile_commands.json`, `vcpkg.json`, and
`conanfile.txt` decoding stays in `cpp/project_config.rs`, which returns only
RepoGrammar-owned project-config facts and bounded vcpkg/Conan generic
dependency records through the parser port. It never invokes either package
manager or interprets unsupported conditional, feature, revision, range, or
build semantics.

C# Tree-sitter nodes stay in `src/rust/adapters/parsing/csharp.rs`; the pure
`csharp/test_data.rs` helper owns xUnit `MemberData` argument classification and
same-class source inventory state. The scanner walks only direct class members
once, stores names in a `BTreeMap`, and resolves each direct literal lookup in
`O(log m)` for `m` class members. Traversal contexts share immutable inventories
through `Arc`; cloning a context must never copy the map. C# using evidence is a
persistent lexical-scope chain built only from Tree-sitter `using_directive`
nodes, so comments, strings, and sibling namespaces cannot contribute. It must
store conditional regions as merged disjoint intervals and query overlap in
`O(log c)` for `c` intervals rather than scanning every region per class member.
It must not invoke reflection, MSBuild, Roslyn,
the xUnit runtime, or user data providers, and it must preserve open-world
UNKNOWNs for partial, inherited, generic, conditional, external, or ambiguous
member sets.

Tree-sitter is not a complete semantic analyzer. It can generate syntax
features, changed ranges, code-unit candidates, decorator/call shapes, and
structural fingerprints, but compiler-native semantic facts take precedence over
structural heuristics.

## Semantic worker boundary

Language-native semantic workers belong under `src/workers/` or
`src/rust/adapters/semantic_workers/`. They may use official compiler,
type-checker, or LSP APIs for their language. All compiler-native facts must be
translated into RepoGrammar-owned semantic facts before entering `core`.
Rust-side semantic-worker adapters may depend on transport-neutral JSON parsing
for versioned NDJSON validation, but parser/compiler SDK payloads must still be
translated at the adapter boundary.

When a semantic worker is unavailable, version-incompatible, conflicting with
another analyzer, or unable to decide a dynamic behavior, the result must be
`UNKNOWN` or abstention.

Python v0.1 worker/adapters should use CPython `ast`, `symtable`, and
`tomllib` as the primary frontend. Tree-sitter Python is a fallback for
syntax-error, incomplete-file, or worker-unavailable cases; it is not the
primary Python semantic frontend. Pyrefly may become the primary static
semantic provider only through public CLI/LSP-style boundaries. Pyright may be
used as a selective cross-check provider for claim-upgrading facts. Mypy is
project-native auxiliary evidence only when the target repository already uses
it. RightTyper-style observed evidence is deferred, explicitly opt-in, and must
not run during default indexing because it executes user code.

The current implemented Python slice uses a checked-in CPython `ast` worker for
structural code-unit extraction only. It does not run Pyrefly, Pyright, mypy,
`ty`, RightTyper, or repository code.
The Rust ports layer now defines a RepoGrammar-owned future Python provider
boundary for candidate-scoped provider requests, provenance assumptions, cache
key dimensions, and recoverable provider-unavailable `UNKNOWN`s. That port is
not an adapter and does not execute Pyrefly, Pyright, RightTyper, or repository
code by itself.

Go source parsing and semantics remain unimplemented. The in-process static
project-config reader is limited to bounded supplied `go.mod`/`go.work` text and
language-neutral declaration inventory; it adds no production dependency and
authorizes no Go runtime behavior. ADR-0021 permits a later version-pinned
Tree-sitter Go dependency only as syntax fallback and defines an explicit,
opt-in, sandboxed standard-library worker over supplied inputs as the
authoritative source path. The safe default must not invoke `go/packages`, `go
list`, gopls, cgo, or repository build/test/generate commands.
The existing TypeScript process adapter is not a Go sandbox and must not be
reused as one. Go claim impact must enter the existing authoritative cross-
language family-`UNKNOWN` classifier; language callers may not infer blocking
behavior from raw build/module/generated assumptions.

PHP discovery/configuration classification and bounded inventory persistence
are implemented without a source frontend or production dependency. PHP source
and PHPUnit XML cause no parser-facing source-store read and create no code
unit, IR, fact, typed `UNKNOWN`, dependency, or family. Exact `composer.json`
and `composer.lock` are the static-metadata exception: `php.rs` reads supplied
UTF-8 JSON only after the shared bounded unique-member gate, then emits
RepoGrammar-owned Composer rows and claim-scoped inventory uncertainty. Lock
rows retain unknown directness and unverified manifest coherence; source URLs,
installation, autoload, build, and runtime state are not inferred. Exact
`.composer`/`.phpunit.cache` components are PHP-only exclusions; exact `vendor`
retains the existing global policy. A future selected project-model boundary,
not discovery or a caller, must own custom `vendor-dir`/cache selection before
semantic admission.

ADR-0024 names `mago-syntax` 1.43.0 only as the production frontend candidate
in a separately reviewed OS-sandboxed worker and authorizes no dependency or
runtime behavior. Official PHP 8.5.8 `php -n -l` may participate only as the
isolated syntax-validity oracle; `nikic/PHP-Parser` 5.8.0 is the isolated AST/
location differential and separately qualification-gated fallback. Tree-sitter
PHP 0.24.2 is a syntax-candidate fallback, never the semantic oracle. The
current Composer inventory does not implement a selected project profile or
lock-content-hash coherence. A future bounded non-executing stage may add that
profile and pin Composer 2.10.2 coherence semantics; PHPUnit XML remains future
input. A frontend worker receives only one PHP source plus the future bounded
profile, never raw configuration. No worker or project-model path may execute
Composer, PHPUnit, autoloaders, plugins, scripts, repository PHP, or target
dependencies. The exact artifact, transitive/advisory, sandbox, protocol,
resource, five-target, and native-runtime gates must pass before any dependency
or worker is added.

Swift discovery/config classification and bounded inventory persistence are
implemented without a source frontend, toolchain, or production dependency.
Swift source, executable manifests, version-specific manifests, and
`.swift-version` cause no parser-facing source-store read and create no code
unit, IR, semantic fact, typed `UNKNOWN`, dependency, family, project model, or
support/readiness claim. Exact `Package.resolved` is the sole static-metadata
exception: `adapters/parsing/swift.rs` may decode supplied schema-2/3 JSON only
after the shared `adapters/parsing/bounded_json.rs` gate proves bounded depth,
object-member count, decoded-key size, and unique decoded member names. It may
emit only RepoGrammar-owned SwiftPM lock rows and typed project-inventory
uncertainty. Package locations and revisions are discarded. Exact
`.build`/`.swiftpm` components are Swift-only exclusions and do not globally
prune other languages. The future source/project-model boundary must restore
token-based context invalidation before cross-file Swift semantics are admitted.

ADR-0025 names SwiftSyntax 603.0.2 `SwiftParser`
only as the syntax-frontend candidate in a separately reviewed OS-sandboxed
worker and authorizes no dependency, toolchain, artifact, or runtime behavior.
It must be differentially qualified against the exact Swift 6.3.3 compiler;
recovery or disagreement is not semantic evidence. Exact 6.3.3
SourceKit-LSP/sourcekitd is only a separately admitted semantic identity
candidate. It must consume synthesized supplied inputs without opening the
repository, evaluating `Package.swift`, building/indexing modules, resolving
dependencies, loading macros/plugins, or using ambient SDK/toolchain state.

The current lockfile boundary parses only the ADR-0025 bounded
`Package.resolved` schema-2/3 subset; a future project-model boundary may add a
separately reviewed bounded static SwiftPM manifest subset. Neither may execute
Swift, SwiftPM, Xcode, manifests, plugins, macros, generators, target code, tests, child
processes, or network requests. Exact archive/installer hashes and signatures,
toolchain/source mapping, transitive packages, licenses, advisories, build
scripts/C/C++ shims/generated code, SBOM/reproducibility, five-target compile
and corpus evidence, and native Linux/macOS/Windows sandbox evidence must pass
before admission. A Swift Testing `@Test` spelling is not dependency or macro
identity evidence and cannot anchor the first family.
The paused baseline and exact next qualification goal are recorded in
`docs/plans/swift-n1-qualification-handoff.md`; qualification evidence and
production artifact/worker admission must remain separate atomic stages.

Ruby discovery/config classification and bounded dependency inventory are
implemented without a production dependency. Ruby source and every config
except exact `Gemfile.lock` remain source-free inventory. A pure Rust lock
parser may read only bounded exact-lock bytes, create one project-config unit,
emit source-free typed `ruby_dependency_inventory` `UNKNOWN`s, and parse
direct `DEPENDENCIES` declarations. It must not read or evaluate executable
DSLs or claim resolved versions. This `discovered_only` state creates no Ruby source IR,
framework family, or support/readiness claim. ADR-0022 names `ruby-prism` 1.9.0
only as a candidate and authorizes no dependency. The
wrapper's native C99/FFI, vendored source, bindgen/libclang, compiler,
static-link, checksum, license,
platform, fuzz/corpus, range/diagnostic, benchmark, and supply-chain surface
must pass the ADR gate before addition. Untrusted Ruby parsing must run in a
separately reviewed OS sandbox over supplied bytes, not in the primary process.
No Ruby worker may evaluate Gemfiles/gemspecs or execute Ruby, Bundler,
RubyGems, Rake, Rails, tests, generators, installed gems, repository tooling,
or network access. Ruby claim impact must enter the authoritative
cross-language family-`UNKNOWN` classifier.

Visual Basic .NET discovery and bounded NuGet declaration inventory are
implemented without a source frontend, MSBuild/NuGet execution, or production
dependency. Exact lowercase `.vb` source remains unread `visual-basic`
inventory. Exact lowercase `.vbproj` is the sole `visual-basic-config` input:
`adapters/parsing/visual_basic.rs` reads supplied UTF-8 bytes only through the
RepoGrammar-owned bounded XML reader, creates one project-config unit, and may
emit literal direct `PackageReference` rows plus claim-scoped
`visual_basic_dependency_inventory` uncertainty. It must not evaluate SDK
imports, explicit imports, properties, conditions, item updates, version
overrides, targets, restore, analyzers, generators, or project code. VB6
`.vbp`/`.frm`/`.bas`/`.cls` inputs remain outside this lane. No dependency row
is family evidence or a support claim.

Object Pascal source discovery and Delphi project inventory are likewise
source-free and dependency-free. Exact `.pas`/`.dpr`/`.dpk` paths use the
generic `object-pascal` token because the suffix does not prove a compiler
dialect. Only exact `.dproj` enters `adapters/parsing/delphi.rs` as
`delphi-config`; the parser may emit bounded literal `DCC_UsePackage` rows with
runtime scope and unknown directness. It must not evaluate Delphi MSBuild,
properties, conditions, imports, targets, `.dpk` source, compiled packages, or
any compiler/package manager. Free Pascal/Lazarus `.pp`/`.lpr`/`.lpi`/`.lpk`
formats are not aliases for Delphi metadata and remain deferred. Dynamic,
malformed, conflicting, or over-limit claims remain scoped
`delphi_dependency_inventory` `UNKNOWN`s.
SQL discovery is implemented by the pure normalized-path classifier in
`adapters/languages/sql.rs`; the bounded DDL frontend belongs in
`adapters/parsing/sql.rs` and its role registry in `adapters/frameworks/sql.rs`.
Under ADR-0040 application indexing reads SQL source, but the frontend may scan
only constructs PostgreSQL 16 and SQLite 3 lex identically, so it still selects
no dialect and keeps dialect `UNKNOWN` for the reason ADR-0035 records. It must
not invoke a parser generator, client, database, driver, or migration tool,
retain statement literals or repository names in any output, or emit
`sql_extension` rows. Admitting a construct the declared dialects lex
differently, widening the invariance set, or adding an extension manifest each
require a superseding ADR; an external grammar or database artifact additionally
requires pinned, source-backed dependency and sandbox qualification.

R discovery belongs in `adapters/languages/r.rs`; bounded metadata parsing
belongs in `adapters/parsing/r.rs`. `.R` and `.r` source is inventory-only. The
parser may
read only exact supplied `DESCRIPTION`, `NAMESPACE`, and `renv.lock` bytes and
must not run R, parse/eval/source R code, profiles, renv, packages, native code,
children, or network operations. DESCRIPTION/NAMESPACE cannot default to CRAN.
Only explicit bounded renv CRAN/Bioconductor records may cross the parser port
as language-neutral dependencies; remote/custom/URL/local source values must be
discarded, never sanitized into an output surrogate. This slice uses existing
Rust/serde_json APIs and authorizes no production dependency.
MATLAB discovery and bounded package inventory are implemented without a
source frontend, licensed toolchain, or production dependency. Lowercase `.m`
is inventory-only and never reaches the source store or parser. Only exact
root/nested `resources/mpackage.json` bytes may enter the shared bounded,
duplicate-key-rejecting JSON gate. The parser may emit one project-config unit,
direct `matlab_add_on` declaration rows keyed by package name plus UUID, and
typed `matlab_dependency_inventory` uncertainty. Provider/contact fields are
discarded; compatibility strings are requirements, never installed or resolved
versions. No path may invoke MATLAB, Octave, Simulink, project startup code,
package installation, child processes, or network access, and no `.m` path may
produce source IR, semantic facts, external symbols, families, or support.

Assembly source admission is restricted to lowercase `.s`; uppercase `.S` is a
language-specific exclusion because it conventionally requires preprocessing.
The in-process parser is a byte/line/fact-bounded lexical scanner for one
candidate profile: x86-64 ELF GNU as 2.46 AT&T syntax. It may emit a module,
generic label units, containment IR, and selected source-visible directive,
label, direct-call, and direct-jump facts. It must always emit an unproven-profile
UNKNOWN, and macro/include/repetition, conditional, Intel-syntax, `.code16`,
`.code32`, malformed, or over-budget evidence remains typed uncertainty. The
scanner never assembles, preprocesses, links, executes, reads include targets,
spawns a child, or accesses the network; lexical facts cannot support a family.

Scratch remains outside product discovery and indexing. The disconnected
`adapters/languages/scratch.rs` prerequisite accepts only caller-supplied classic
single-disk, non-ZIP64 `.sb3` bytes, validates bounded central-directory/path/
size/ratio/JSON structure, rejects encryption, links, traversal, duplicate
paths, and unsupported compression, and parses only a stored root
`project.json` without extraction. It is not a general ZIP reader and does not
create a language token, source document, code unit, IR, fact, dependency, or
family. A binary-document port and a maintained ZIP/deflate qualification are
mandatory before `.sb3` discovery may be added.

Provider SDK objects, LSP payloads, private Pyrefly data structures, Pyright
internals, Python AST nodes, and runtime trace payloads must be translated into
RepoGrammar-owned facts at the adapter boundary before entering `core`. Do not
reimplement a Python parser, whole-program call graph, or general type checker
when existing tooling and a bounded adapter can provide the needed evidence.

## SQLite boundary

SQLite and migration execution logic belong in `src/rust/adapters/persistence/`.
The `rusqlite` production dependency is allowed only at that adapter boundary.
Storage use cases depend on RepoGrammar-owned ports such as
`ports::index_store`, `ports::family_store`, and `ports::source_store`, not
direct SQL calls or SQLite row types.

## MCP boundary

MCP tool names, schemas, transport errors, and serialization rules belong in
`src/rust/interfaces/mcp/`. Domain classifications are expressed in core types before
they are serialized for MCP.

## Optional provider boundary

Optional providers such as a future CodeGraph provider may supply auxiliary
candidate, call/dependency, or graph-neighborhood facts only through
RepoGrammar-owned port types. Missing, stale, unavailable, or conflicting
provider facts must not fail default indexing and must become typed `UNKNOWN` or
auxiliary diagnostics for affected claims.

RepoGrammar must not create, initialize, modify, delete, or require
provider-owned local state such as `.codegraph/`.

## Test code boundary

All test source lives under `src/`, either next to modules with `#[cfg(test)]`,
in `src/rust/integration_tests/`, or in documented test-support modules. Root
`tests/`, `benches/`, `examples/`, and `scripts/` directories are not allowed.

## src-only rule

All source, executable, test, benchmark, migration-tool, fixture-source, worker,
and automation-tool code must live under `src/`, regardless of implementation
language. `repo-guard check` enforces this by detecting common source extensions
outside `src/`.
