# ADR-0022: Ruby Prism and Minitest preflight

- Status: Accepted
- Date: 2026-07-16
- Scope: Ruby in ADR-0020 wave N1; decision-only preflight
- Refines: ADR-0004 and ADR-0020
- Related: `docs/plans/top-20-language-expansion-plan.md`,
  `docs/specifications/semantic-workers.md`,
  `docs/specifications/indexing-pipeline.md`,
  `docs/architecture/dependency-rules.md`, and
  `docs/reports/language-support/ruby-completion-review.md`

## Context

ADR-0020 requires a bounded discovery/configuration path, an authoritative
frontend, RepoGrammar-owned IR, typed uncertainty, one exact-anchor family,
source-free product readiness, atomic delivery, and a final completion audit
before Ruby can be described as supported. RepoGrammar currently implements
none of those Ruby modules. This decision does not add them.

The verified language baseline is CRuby 4.0.6, released on 2026-07-14. Prism is
the CRuby-associated maintained Ruby parser candidate. It accepts supplied
bytes and returns a tolerant AST, byte locations, and diagnostics without
executing Ruby source. However, the published `ruby-prism` Rust wrapper is not
a pure-Rust parser: it wraps `ruby-prism-sys`, links the C99 `libprism` parser,
uses generated FFI bindings, and defaults to a vendored native build.

The candidate release also exposes a provenance hazard. The mutable hosted
Rust documentation is labelled `ruby_prism 1.9.0` and currently advertises
`parse_with_options` and a syntax `Version` API. Those APIs are absent from the
`v1.9.0` release source at GitHub-verified signed commit
`c0e37816e97e23e92524a4070e1b99a4025bc63f`. That source exposes
`parse(source: &[u8]) -> ParseResult<'_>` with no per-request Ruby syntax
version option. A version string on mutable hosted documentation is therefore
not sufficient dependency or API authority.

Ruby's classes, constants, loading, test discovery, and DSLs are runtime-
mutable. Prism can prove source syntax and exact source-visible shapes. It
cannot prove runtime constant identity, load-path behavior, method lookup,
plugins, generated sources, or metaprogrammed declarations. The first slice
must be deliberately narrower than Minitest's complete runtime behavior.

## Decision

### D1. Ruby remains not started and unsupported

This ADR is preflight authority only. It adds no Ruby discovery, parser,
dependency, worker, code unit, IR, fact, `UNKNOWN`, family, fixture, CLI/MCP
behavior, or readiness state. Ruby remains `not_started` in the Top-20 program.

The stable future discovery tokens are `ruby` for source and `ruby-config` for
project/configuration inventory. The latter is a RepoGrammar language token,
not permission to invoke the `ruby-config` executable. The first future family
token is `ruby.minitest.test_method`.

### D2. Discovery and configuration stay pure and source-free

The discovery/configuration module must use one pure classifier over normalized
repo-relative paths. Configuration classification precedes source-suffix
classification, so `gems.rb` is always `ruby-config`. Matching is
case-sensitive. It may classify:

- files with the exact `.rb` suffix as `ruby`, including the basename `.rb`
  itself; and
- root or nested files with exact basenames `Gemfile`, `Gemfile.lock`,
  `gems.rb`, `gems.locked`, or `.ruby-version`, plus root or nested basenames
  ending in `.gemspec`, including the basename `.gemspec` itself, as
  `ruby-config`.

Nested `*.gemspec` inventory is an intentionally conservative repository-wide
monorepo inventory, not a model of Bundler's default selection. Bundler's
`gemspec` directive defaults to the Gemfile directory unless an explicit path
or composition selects another location. Inventory does not decide which
gemspec a Gemfile selects. Multiple manifests, version files, lockfiles, or
gemspecs remain structural candidates until a later explicit project-root and
precedence policy resolves them.

The Ruby-specific classifier must reject candidates below path components named
`.bundle` or `.ruby-lsp`. Bundler stores local project configuration, including
potentially sensitive source settings, below `.bundle`; Ruby LSP generates a
composed bundle below `.ruby-lsp`. These are language-specific exclusions, not
new generic discovery rules: the walker must not prune either directory, and
non-Ruby languages below them remain eligible under their own policies. A Ruby
candidate rejected by this policy uses the stable
`language_specific_exclusion` skipped-path token rather than pretending that
its extension is unsupported. Ambiguous directories such as `tmp` and `pkg`
must not be globally excluded. `Rakefile`, `*.rake`, `config.ru`, ERB, gem
archives, installed bundle trees, and arbitrary custom `BUNDLE_GEMFILE` or
lockfile paths are deferred from the first discovery slice.

Discovery may hash bounded raw bytes but records only repository-relative path,
strict content hash, byte size, and the stable language token. Full and
incremental indexing must treat `ruby` and `ruby-config` as inventory-only
before any parser-facing source-store read. It must not evaluate or parse
Gemfiles or gemspecs as Ruby, infer dependency resolution from their text, read
Bundler configuration, select an ambient Ruby, or turn manifest presence into
framework or family support.

### D3. Prism is a candidate native frontend behind a sandboxed worker

The current candidate coordinate/profile is `ruby-prism` 1.9.0 from the
official `v1.9.0` release pointing to GitHub-verified signed commit
`c0e37816e97e23e92524a4070e1b99a4025bc63f`, under the MIT license. Its
published crate checksum has not been verified in this preflight and remains a
mandatory D7 gate. This is a candidate coordinate, not an authorized production
dependency or completed pin.

Upstream release source and the published crate artifact are distinct
authorities. The exact docs.rs 1.9.0 packaged source records the same commit and
the `rust/ruby-prism` path in `.cargo_vcs_info.json`, but this preflight has not
verified the crates.io checksum or proven complete upstream/package artifact
equivalence. D7 must do both. Any dependency review must use the exact upstream
commit plus the checksummed published artifact, not mutable hosted rustdoc, as
API and provenance authority. For the exact upstream commit and packaged
source inspected here:

- `parse(&[u8]) -> ParseResult` consumes supplied bytes;
- node and diagnostic locations are byte offsets/slices into those bytes;
- parse errors and warnings are iterators over parser diagnostics;
- parser initialization and unknown-node conversion contain panic paths; and
- the wrapper owns native parser and AST allocations until `ParseResult` is
  dropped.

The inspected release/package API has no syntax-version option. Its parser
artifact identity is therefore part of the syntax profile. The first frontend
profile is a candidate for CRuby 4.0 syntax using CRuby 4.0.6 as the reference
version. The qualification stage must cross-check parser equivalence against a
committed CRuby 4.0.6 syntax corpus. The first conservative selector accepts
only one `.ruby-version` in the entire repository, located at the repository
root, whose complete UTF-8
content is exactly `4.0.6` with at most one trailing LF. A bounded project-model
step validates that file and produces normalized profile metadata before worker
execution. It does not execute Ruby or evaluate any Ruby program.

An absent `.ruby-version`, multiple or nested version files, CRLF/whitespace or
engine-prefixed forms, a different version, preview/future versions, JRuby,
TruffleRuby, other engines, or conflicting version evidence must become
`ruby_syntax_version` `UNKNOWN` for affected claims after the obligation
registry lands; until then Ruby semantic capability is unavailable. Nested
version files remain inventory only until a successor decision accepts a
nearest-ancestor/project-root precedence policy. No ambient or nonexistent
request configuration may silently select the latest grammar.

RepoGrammar must not link the native parser into the primary process for
untrusted repository analysis. An in-process C parser provides no hard wall-
clock or memory limit and no containment for a native fault, abort, stack
overflow, or process-wide allocator failure. The frontend/IR stage therefore
requires a separately reviewed OS-sandboxed worker. If the declared filesystem,
network, descendant-process, wall-clock, CPU, memory, and output limits cannot
be enforced on a platform, Ruby semantic capability is unavailable there.

The worker receives only bounded `.rb` source bytes plus normalized, validated
syntax-profile metadata through the protocol. It must not receive or parse raw
Gemfile, gemspec, Bundler configuration, lockfile, or `.ruby-version` bytes.
The bounded project-model step owns `.ruby-version` validation; executable
project configuration remains inventory only. The worker must not traverse the
repository, home directory, installed gems, Bundler or Ruby caches, credentials,
version-manager state, or host configuration. It must have no network,
repository/host writes, child processes, target-repository working directory,
or ambient Ruby/Bundler environment. Rust-facing boundaries accept only
RepoGrammar-owned requests, units, IR, facts, evidence, provenance, diagnostics,
and typed `UNKNOWN`s.

### D4. The first family is an exact direct Minitest method slice

The first family target is `ruby.minitest.test_method`. Each anchor must satisfy
all of these source-visible conditions in one parse-clean `.rb` file:

1. an unconditional direct child of the program body has no receiver, is named
   exactly `require`, has exactly one argument, and that argument is the non-
   interpolated literal string `"minitest/autorun"`;
2. later in lexical program-body order, an unconditional direct named class
   child has the exact superclass constant path `Minitest::Test`;
3. a direct instance method declaration in that class body has a name beginning
   with `test_` and a non-empty suffix;
4. the method has zero required, optional, rest, forwarding, keyword,
   keyword-rest, or block parameters, and is source-visibly public under the
   direct class-body visibility state with no later visibility mutation that
   can affect it; and
5. no parser error, syntax-profile blocker, generated-source blocker, visible
   conflicting constant/load identity, or claim-relevant runtime-mutation
   `UNKNOWN` affects the anchor.

The slice is intentionally stricter than Minitest runtime discovery. Minitest
uses inherited public instance methods whose names match `^test_`; the first
RepoGrammar family accepts only methods directly declared in the exact class
body. The zero-parameter rule also prevents claiming a method that the runner
would invoke without a compatible source-visible signature.

A family requires at least three fresh, distinct, same-language exact anchors
that pass the existing complete-link compatibility and support gates. Three
method names are insufficient when their parse, profile, require identity,
constant identity, generated origin, freshness, or compatibility is unresolved.

The following are explicit non-claims for the first slice:

- the historical `MiniTest` spelling or aliases of `Minitest`/`Test`;
- indirect subclasses, inherited test methods, included modules, or class
  reopenings;
- singleton methods/classes, `define_method`, aliases, refinements, and dynamic
  method names;
- Minitest spec DSL, RSpec, Rails test helpers, or any other test DSL;
- dynamic, conditional, nested, or class-following `require`/`load`, load-path
  mutation, autoload, or conditional loads;
- private/protected test methods or dynamic/ambiguous visibility changes;
- `class_eval`, `module_eval`, `eval`, `const_set`, `autoload`,
  `method_missing`, runtime plugins, monkey patches, or generated sources; and
- runtime execution, pass/fail outcomes, hooks, order, filtering, parallelism,
  assertions, fixtures, or plugin behavior.

Some non-claims are ordinary abstentions; plausible source shapes made
ambiguous by loading, constant binding, mutation, generation, or parse/profile
degradation must remain the typed obligations in D5.

### D5. Evidence ladder and authoritative `UNKNOWN` obligations

The evidence order is:

1. **Primary:** a fresh, version-pinned, sandboxed Prism worker fact over
   supplied bytes, with artifact digest, exact upstream commit, protocol and
   syntax profile, sandbox profile, normalized path/hash/range, and deterministic
   diagnostic status.
2. **Claim-supporting derived fact:** a RepoGrammar-owned exact Minitest anchor
   emitted only after the D4 shape and authoritative claim-impact gates pass.
3. **Auxiliary:** `.rb` discovery, bounded manifest/version inventory, comments,
   warnings, and unsupported dynamic candidates. These explain context or
   uncertainty but do not support a family by themselves.
4. **Forbidden for the claim:** extension-only recognition, regex/text matching,
   mutable hosted API docs, unpinned parser output, Gemfile/gemspec evaluation,
   installed-gem inspection, Ruby/Minitest execution, editor/LSP caches, or
   structural similarity without every D4 identity gate.

The future Ruby obligation registry must define at least these stable internal
mechanism names:

| Mechanism | Initial claim impact | Resolution evidence |
|---|---|---|
| `ruby_parse_degraded` | Blocks anchors in the affected file | Clean whole-file parse under the pinned artifact, with deterministic bounded diagnostics |
| `ruby_syntax_version` | Blocks affected Ruby anchors | The sole repository `.ruby-version` is at the root with exact `4.0.6` plus optional LF, and matching parser/corpus provenance |
| `ruby_minitest_require_identity` | Blocks the affected Minitest anchor | Exact unconditional direct program-body literal `require "minitest/autorun"` lexically before the class, with no conflicting load identity |
| `ruby_constant_identity` | Blocks the affected class/method anchor | Exact `Minitest::Test` path with no source-visible alias, rebinding, or conflicting constant definition in the accepted scope |
| `ruby_minitest_test_definition` | Blocks the affected method anchor | One direct source-visibly public zero-parameter `test_*` instance method in the accepted class body, with no affecting visibility mutation |
| `ruby_runtime_mutation` | Blocks only a claim whose accepted scope can be changed by the mutation | A later authorized semantic mechanism proves the narrower identity; otherwise the dynamic residual stays `UNKNOWN` by design |
| `ruby_generated_source` | Blocks an anchor only when bounded positive evidence identifies generated origin or conflicting origin evidence | Proven primary-source provenance under the generated-source policy added with the registry; marker absence is neither provenance nor a blocker |

Stale evidence, conflicting facts, and insufficient support continue to use the
cross-language governance mechanisms. One authoritative Ruby claim-impact
classifier must map the table above into the existing family-`UNKNOWN`
classifier. Discovery, family builders, readiness, CLI, and MCP callers may not
rederive blocking behavior from raw AST fields or mechanism-name substrings.
Foreign-provenance `UNKNOWN`s never block a Ruby family.

These names are normative future obligations only. This preflight emits no
Ruby `UNKNOWN` fact or reason code and resolves none.

### D6. Proposed hard resource and determinism contract

The first worker implementation must propose no higher ceilings than:

- 256 `.rb` source files per worker request;
- 1 MiB per decoded file and 8 MiB aggregate decoded bytes;
- 12 MiB for the complete encoded request and 1 MiB for the complete encoded
  response/protocol stdout;
- 1 MiB of captured stderr;
- 250,000 visited AST nodes, maximum accepted traversal depth 512, 4,096 parser
  diagnostics, and 10,000 emitted facts/unknowns per request;
- a five-second wall-clock deadline, four CPU-seconds per request, and at most
  two worker threads including the main thread; and
- a 256 MiB worker-process memory ceiling, one worker process/request per
  repository indexing operation, and no descendants.

These are preflight ceilings, not implemented or benchmark-proven facts. The
frontend may lower them. Raising one requires measured corpus evidence and an
ADR/plan update before implementation.

Rust must reject file, decoded aggregate, encoded request, and path/count
overflow before spawning. The worker must stop traversal before AST/depth/fact/
diagnostic overflow, but the OS sandbox remains mandatory because those Rust
checks cannot constrain native parsing before traversal begins. Timeout, crash,
panic, signal, malformed protocol, truncation, overflow, invalid range, or
nonzero exit invalidates the complete response. Partial facts are never
accepted.

The qualification evidence must prove runtime enforcement of every ceiling on
each of the five D7 target triples. If CPU-time, thread-count, memory,
filesystem, network, descendant-process, wall-clock, or output enforcement is
missing or bypassable on one target, Ruby semantic capability must fail closed
as unavailable on that target.

Inputs and outputs must be sorted deterministically. Fact identifiers and cache
keys include ordered `(repo-relative path, content hash)` entries, immutable
worker artifact digest, crate/artifact/upstream-commit identity,
protocol/operation, CRuby syntax profile, classifier policy, sandbox profile,
and all explicit configuration.
No timestamp, filesystem enumeration order, absolute path, ambient environment,
or network state may enter claim facts. Identical normalized inputs and
identities must produce byte-identical accepted payloads.

### D7. No parser dependency is authorized by this ADR

Before adding `ruby-prism`, `ruby-prism-sys`, or another production frontend,
an independent dependency-and-sandbox qualification stage must commit only
documentation and reproducible evidence. It must demonstrate need and record:

- exact crate version, crates.io checksum, exact upstream release and commit,
  vendored-source inventory/digest, license, maintainership, and advisory review;
- the full transitive dependency and build-script surface;
- C compiler, `cc`, bindgen/libclang, static-link, vendoring, cross-compilation,
  and reproducible-build implications;
- compile, parser-corpus, and runtime sandbox proof on every claimed
  RepoGrammar release target: `x86_64-unknown-linux-gnu`,
  `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`,
  `aarch64-apple-darwin`, and `x86_64-pc-windows-msvc`; any target without all
  three proofs must report Ruby capability unavailable;
- deterministic byte ranges and sanitized diagnostics against committed valid,
  invalid, encoding, deep-nesting, and recovery corpora;
- malformed-input fuzzing, native crash/panic/stack behavior, memory growth,
  allocator-failure handling, and CPU/memory benchmarks at every D6 boundary;
- artifact acquisition and checksum verification with no request-time network;
  and
- the independently reviewed OS sandbox and fail-closed unsupported-platform
  behavior.

The mutable rustdoc/release-source mismatch must have a regression check against
the exact published artifact API. Only after that qualification stage is
accepted may a later stage admit the dependency and worker artifact into
production code. If the candidate cannot satisfy the platform, resource,
diagnostic, provenance, or containment gates, implementation stops. A different
frontend requires a successor decision rather than a silent substitution.

### D8. Repository and tool execution is forbidden

Default and first supported Ruby analysis must not invoke or execute:

- `ruby`, `ruby-config`, JRuby, TruffleRuby, or any version manager;
- `bundle`, Bundler APIs, `gem`, RubyGems plugins, or dependency installation;
- `rake`, Rails, Minitest, RSpec, tests, generators, application boot, or
  repository scripts;
- Gemfile, gemspec, Rakefile, `config.ru`, installed-gem, native-extension, or
  editor/LSP code;
- a compiler, linker, package manager, shell, or other child process from the
  target repository; or
- network access or artifact acquisition during indexing/query requests.

Gemfiles and gemspecs are Ruby programs. Reading them as inventory is allowed;
evaluating them is repository-code execution and is forbidden. Any future
trusted execution mode requires a successor ADR with separate consent,
acquisition, environment, credential, cache, network, subprocess, timeout,
provenance, and non-claim gates.

### D9. Ten atomic stages gate any Ruby support claim

Ruby delivery is staged as independently coherent Conventional Commits:

1. **Preflight:** this ADR, incomplete completion review, and synchronized
   plan/specification/roadmap/changelog/memory references; no code/dependency.
2. **Discovery/config:** the `ruby`/`ruby-config` tokens, pure path classifier,
   bounded source-free inventory, exclusions, multiple-root/config cases,
   invalid/oversized/symlink/Git/resource tests, deterministic one-per-token
   path-free warnings, honest Ruby-only `file_manifest_only` and mixed
   `syntax_only_code_units` output, inventory-only add/modify/remove deltas,
   legacy claim-record purge, autosync/fingerprint coverage, status/count
   behavior, and docs; advance at most to `discovered_only`. While the tokens
   remain inventory-only and absent from `ParserProjectContext`, their deltas do
   not force project-context fallback. Before frontend/IR emits any Ruby unit,
   fact, `UNKNOWN`, or family input, it must add the required Ruby context and
   restore token-based project-context invalidation.
3. **Dependency and sandbox qualification:** documentation and reproducible
   evidence only, with no production dependency or worker artifact admitted;
   complete D3/D6/D7 supply-chain, exact-artifact API, corpus/fuzz/resource,
   five-target build/runtime, and OS-sandbox proofs or record a no-go.
4. **Sandboxed worker, protocol, and artifact admission:** only after stage 3
   acceptance, add the pinned production dependency/artifact, fail-closed OS
   sandbox, bounded protocol, artifact acquisition/verification, and process,
   malformed-input, resource, leakage, and unsupported-target tests.
5. **Prism frontend and IR:** use only the accepted worker boundary to add
   RepoGrammar units/IR, clean/degraded parse and range handling, normalized
   profile metadata, restored project-context invalidation, persistence, and
   architecture/dependency documentation.
6. **`UNKNOWN` classifier/registry:** add the D5 authoritative classifier,
   obligation registry, generated-source provenance/detection policy,
   unresolved/resolved fixtures, provider fallback, freshness, conflict,
   provenance, and cache tests. Before this stage the named mechanisms are
   unavailable, not emitted obligations.
7. **Exact Minitest family and fixtures:** implement D4 with at least three
   compatible anchors plus positive, negative/lookalike, low-support,
   parse-degraded, syntax-profile, generated, mutation, stale/conflict, and
   unresolved/resolved fixtures and exact non-claims.
8. **Source-free product wiring:** persistence/readback, status, doctor, stats,
   unknowns, CLI/MCP inventory/readiness, synchronization, and rejection of
   source, absolute paths, raw diagnostics, credentials, and high-cardinality
   leakage.
9. **Four-part review and fixes:** audit correctness/bugs, security,
   design/completeness, and performance/resources; fix findings in scoped atomic
   commits with tests/docs rather than folding them into a completion claim.
10. **Completion audit:** link every prerequisite SHA, check the ADR-0020 nine-
   gate matrix, run every required check, and only then decide whether evidence
   reaches `bounded_preview` or another honest state.

Every implementation stage includes matching tests and documentation. No stage
may claim Ruby completion early, and no push is authorized by this decision.

### D10. Failure taxonomy and stop conditions

Classify failures before retrying:

- **unsupported profile:** engine/version/project-root evidence is absent,
  ambiguous, conflicting, or outside CRuby 4.0;
- **degraded input:** syntax, encoding, range, generated origin, require,
  constant, test-definition, or mutation identity is unresolved;
- **provider unavailable:** consent, artifact, compatible protocol, sandbox, or
  platform support is missing;
- **resource failure:** any D6 input, AST, diagnostic, output, time, memory, or
  descendant-process bound is exceeded;
- **native/process failure:** panic, abort, crash, signal, nonzero exit, or
  malformed/truncated protocol;
- **governance failure:** evidence is stale/conflicting, support is below three,
  a prerequisite SHA/check is missing, or source-free output leaks; and
- **environment failure:** a required build/release tool for producing the
  pinned worker artifact is unavailable outside analysis requests.

Stop rather than weaken a gate when the native dependency cannot be contained,
the artifact/API cannot be pinned reproducibly, a malformed corpus causes an
uncontained failure, byte ranges/diagnostics are nondeterministic, a repository
would require Ruby/Bundler execution, or a source-free completion claim cannot
be proven. A valid negative result is `not_started`, `discovered_only`,
provider-unavailable, typed `UNKNOWN`, or source-backed no-go; it is not a weak
support claim.

## Alternatives considered

- **Link Prism directly into the RepoGrammar process:** rejected for untrusted
  input because native faults and hard resource limits are not contained.
- **Use hosted rustdoc as the dependency pin:** rejected because its 1.9.0-
  labelled API differs from the `v1.9.0` release source at the verified commit.
- **Execute Ruby's bundled Prism API:** rejected because it requires selecting
  and running a Ruby engine in the target analysis environment.
- **Evaluate Gemfiles/gemspecs for accurate project context:** rejected because
  both are executable Ruby and can perform arbitrary process, filesystem, and
  network actions.
- **Run Minitest to discover tests:** rejected because it loads and executes
  repository and dependency code, plugins, hooks, and dynamic definitions.
- **Accept every `test_*` method or `Minitest::Test` spelling:** rejected because
  load identity, direct class/method shape, parameters, parse/profile status,
  mutation, generation, and support gates are necessary.
- **Model Minitest's inherited/runtime discovery in the first slice:** rejected
  because Prism syntax alone cannot prove the runtime ancestor/method set.

## Consequences

- Ruby now has a bounded preflight authority and exact first-family target, but
  remains unimplemented and unsupported.
- Prism remains the preferred syntax candidate only behind immutable artifact
  provenance, a native dependency review, and an OS sandbox.
- The first family intentionally favors falsifiable direct declarations over
  Ruby's broader runtime behavior.
- Dynamic loading, constant binding, metaprogramming, generated sources, and
  runtime test discovery stay typed uncertainty or explicit non-claims.
- No production dependency or product runtime behavior changes in this
  decision.

## Primary sources verified

The following live primary sources were checked on 2026-07-16:

- the official [Ruby 4.0.6 release](https://www.ruby-lang.org/en/news/2026/07/14/ruby-4-0-6-released/)
  and [Ruby release index](https://www.ruby-lang.org/en/downloads/releases/);
- the official [Prism v1.9.0 release](https://github.com/ruby/prism/releases/tag/v1.9.0),
  [`ruby-prism` 1.9.0 crate page](https://crates.io/crates/ruby-prism/1.9.0), and
  [published crate artifact](https://crates.io/api/v1/crates/ruby-prism/1.9.0/download),
  plus the exact docs.rs package
  [source](https://docs.rs/crate/ruby-prism/1.9.0/source/src/lib.rs),
  [manifest](https://docs.rs/crate/ruby-prism/1.9.0/source/Cargo.toml), and
  [VCS metadata](https://docs.rs/crate/ruby-prism/1.9.0/source/.cargo_vcs_info.json)
  that records commit `c0e37816e97e23e92524a4070e1b99a4025bc63f` and
  `rust/ruby-prism` as the path in version control; separately,
  immutable commit sources for the
  [license](https://github.com/ruby/prism/blob/c0e37816e97e23e92524a4070e1b99a4025bc63f/LICENSE.md),
  [`ruby-prism` manifest](https://github.com/ruby/prism/blob/c0e37816e97e23e92524a4070e1b99a4025bc63f/rust/ruby-prism/Cargo.toml),
  [wrapper build script](https://github.com/ruby/prism/blob/c0e37816e97e23e92524a4070e1b99a4025bc63f/rust/ruby-prism/build.rs),
  [`ruby-prism-sys` manifest](https://github.com/ruby/prism/blob/c0e37816e97e23e92524a4070e1b99a4025bc63f/rust/ruby-prism-sys/Cargo.toml),
  [FFI build entrypoint](https://github.com/ruby/prism/blob/c0e37816e97e23e92524a4070e1b99a4025bc63f/rust/ruby-prism-sys/build/main.rs), and
  [vendored C build](https://github.com/ruby/prism/blob/c0e37816e97e23e92524a4070e1b99a4025bc63f/rust/ruby-prism-sys/build/vendored.rs);
- the mutable hosted [`ruby_prism` Rust API](https://ruby.github.io/prism/rust/doc/ruby_prism/index.html)
  compared with the exact upstream-commit
  [`parse`/`ParseResult`/`Location`/`Diagnostic` source](https://github.com/ruby/prism/blob/c0e37816e97e23e92524a4070e1b99a4025bc63f/rust/ruby-prism/src/lib.rs),
  which establishes the mutable-document/release-source mismatch;
- Bundler's official [Gemfile documentation](https://bundler.io/man/gemfile.5.html)
  and [configuration documentation](https://bundler.io/man/bundle-config.1.html),
  including Gemfile/gemspec evaluation and `.bundle/config`;
- the official Ruby LSP [VS Code](https://shopify.github.io/ruby-lsp/vscode-extension)
  and [troubleshooting](https://shopify.github.io/ruby-lsp/troubleshooting.html)
  documentation for the generated `.ruby-lsp` composed bundle; and
- official Minitest [overview/examples](https://docs.seattlerb.org/minitest/),
  [`Minitest::Test`](https://docs.seattlerb.org/minitest/Minitest/Test.html), and
  [`Minitest::Runnable`](https://docs.seattlerb.org/minitest/Minitest/Runnable.html)
  documentation, including direct `test_*` examples and inherited public method
  discovery.

These sources constrain the decision. They do not prove RepoGrammar has
implemented Ruby support, nor do they satisfy the D7 dependency gate.

## Follow-up work

- Execute D9 on a dedicated Ruby major-feature branch, beginning with the
  independent discovery/configuration module.
- Before admitting any production dependency or worker artifact, complete the
  documentation/evidence-only qualification stage: capture the published crate
  checksum and exact vendored inventory, prove the D7 platform/corpus/resource
  gates, and independently review the OS sandbox.
- Keep `docs/reports/language-support/ruby-completion-review.md` incomplete until
  evidence links and commit SHAs are real.
- Write a successor ADR before changing syntax engine/profile, running any Ruby
  or package-management tool, inspecting installed gems, evaluating project
  programs, or enabling a trusted-repository mode.
