# Repository Guard

`repo-guard` is a repository governance CLI implemented in
`src/rust/bin/repo_guard.rs`. It is separate from the RepoGrammar product runtime.

## GitHub-only 0.5.0 publication

The maintainer's 2026-09-09 release instruction selects GitHub binary assets
only. Follow `docs/release/stable-v0.5.0-release-checklist.md`: use an exact-SHA
successful build-only run, retain four native archives and their checksums plus
the installer/checksum, and verify the immutable public GitHub release.
Stable npm staging and automatic stable draft creation are disabled. Existing
npm packing, provenance, dist-tag, and dual-channel finalizer commands remain
compatibility tools; they do not gate or describe 0.5.0 GitHub-only completion.
Do not run them to change npm state or manufacture `STABLE_RELEASE_READY`.
The prior npm `latest=0.4.3` is preserved.

## Commands

```text
cargo run --quiet --bin repo-guard -- check
cargo run --quiet --bin repo-guard -- check-geo
cargo run --quiet --bin repo-guard -- sync-agent-guides --from AGENTS.md
cargo run --quiet --bin repo-guard -- sync-agent-guides --from CLAUDE.md
cargo run --quiet --bin repo-guard -- check-diff --base <git-revision> --head <git-revision>
cargo run --quiet --bin repo-guard -- product-eval --corpus <path> --out <dir> [--repetitions <n>] [--bin <path>] [--condition <token>] [--baseline token-overlap]
cargo run --quiet --bin repo-guard -- payload-measure --out <dir> [--bin <path>] [--fixture <repo-relative-fixture-root>]
cargo run --quiet --bin repo-guard -- performance-eval --out <dir> [--bin <path>] [--worker <path>] [--fixture <local-snapshot>] [--condition <token>] [--repetitions <1..9>] [--python-files <1..256>] [--timeout-seconds <1..3600>]
cargo run --quiet --bin repo-guard -- smoke-packaged-artifact --binary <path> --worker <path> --fixture <path> --expected-version <version> [--require-product-uninstall]
cargo run --quiet --bin repo-guard -- smoke-npm-package --tarball <path> --expected-version <version>
cargo run --quiet --bin repo-guard -- verify-npm-pack-evidence --pack-json <path> --candidate-manifest <path> --expected-version <version>
cargo run --quiet --bin repo-guard -- verify-stable-release-evidence --evidence-dir <path>
cargo run --quiet --bin repo-guard -- release-source --event-name <workflow_dispatch|push> --ref-name <name>
cargo run --quiet --bin repo-guard -- release-channel --version <version>
cargo run --quiet --bin repo-guard -- release-dist-tag-action --version <version> --preview <version-or-empty> --latest <version-or-empty> --tags-json <json-object> --versions-json <json-array>
cargo run --quiet --bin repo-guard -- preview-dist-tag-action --version <version> --preview <version> --latest <version-or-empty> --versions-json <json-array>
```

## check

The check command verifies:

- `AGENTS.md` and `CLAUDE.md` exist.
- both guides are regular files and not symlinks.
- both guides are byte-identical.
- required bootstrap docs and workflows exist, including CI and release
  workflows plus
  `docs/decisions/ADR-0008-repo-local-state-boundary.md`, the v0.1 planning
  documents, the Python v0.1 analysis specification, ADR-0011, ADR-0012, the
  substrate hardening checkpoint, typed UNKNOWN specification, ADR-0009/ADR-0010,
  their durable memory mirrors under `.agents/memories/`, and the accepted
  ADR-0020 Top-20 language expansion gate plus its active implementation plan,
  ADR-0030 dependency/library semantics, the provider/UNKNOWN analysis, the
  dated strict baseline, ADR-0031/ADR-0032, and the
  Go/PHP/Swift/Ruby/Visual-Basic/Delphi-Object-Pascal completion-review records
  referenced by the language-expansion program. Missing review evidence fails
  the guard; unchecked review gates remain an honest incomplete state and are
  not treated as support.
- required skills exist and have `name` and `description` front matter.
- nested `AGENTS.md` or `CLAUDE.md` files do not exist.
- lowercase `agents.md` or `claude.md` duplicates do not exist.
- source files with guarded extensions do not exist outside `src/`, regardless
  of implementation language. The guarded set includes `.rs`, `.c`, `.cc`,
  `.cpp`, `.cxx`, `.h`, `.hpp`, `.hh`, `.hxx`, `.go`, `.py`, `.js`, `.jsx`,
  `.ts`, `.tsx`, `.java`, `.cs`, `.kt`, `.kts`, and shell/SQL extensions, so C#
  and C/C++ fixtures must live under `src/fixtures/`.
- generated local state directories such as `.repogrammar/`,
  `.repogrammar-*`, `.codegraph/`, `target/`, and `.git/` are ignored. A direct
  child of `.claude/worktrees/` is ignored only when its bounded regular `.git`
  pointer resolves under this repository's `.git/worktrees/` directory. This
  permits complete isolated checkouts created for parallel agents without
  exempting unlinked files or directories named `worktrees` elsewhere.
- GitHub workflow files do not use deprecated Node.js 20 action majors for
  first-party checkout or Node setup actions; currently `actions/checkout@v4`
  and `actions/setup-node@v4` are rejected in favor of `@v5` or newer.
- Every `dtolnay/rust-toolchain` step uses the reviewed commit
  `4cda84d5c5c54efe2404f9d843567869ab1699d4` and explicitly requests
  `toolchain: stable`; mutable branches, tags, or a different implementation
  commit fail the guard.
- the release workflow classifies preview versus stable through `repo-guard`,
  keeps manual dispatch build-only, and creates one exact npm candidate that
  downstream jobs download rather than repack;
- preview tags create draft GitHub Releases and use Node 24, npm 11.18.0,
  and Trusted Publisher OIDC to stage that candidate. Stable npm staging is
  explicitly disabled and automatic draft creation is preview-only under the
  2026-09-09 GitHub-only decision. Neither path contains a traditional npm token,
  direct publish, approval, rejection, or dist-tag mutation authority;
- preview staging has one registered assignment for its exact
  `./npm-candidate/...tgz` local tarball, and stable staging has one registered
  literal command for `./npm-candidate/sioyooo-repogrammar-0.5.0.tgz`; a bare
  package path that npm could parse as GitHub shorthand, dynamic npm
  subcommands, marker-only comments, or alternate packing and staging paths
  fail the guard;
- draft creation must retain the exact runner-compatible paginated release
  lookup, filter every page by the requested tag without `--slurp`, and exit
  before upload when any matching public release or draft exists;
- manual stable finalization is read-only and delegates the authoritative
  asset, checksum, SRI, provenance, dist-tag, version, and setup decisions to
  `verify-stable-release-evidence`.

Check mode reports concrete paths and rules and does not modify the repository.
Linked-agent-worktree recognition reads at most 4 KiB from a regular,
non-symlink `.git` pointer and requires its canonical target to be a direct
child of this repository's canonical `.git/worktrees/` directory. Missing,
oversized, non-UTF-8, malformed, foreign, or unresolvable pointers fail closed:
the candidate directory remains inside the normal repository scan. The check
does not traverse a recognized linked checkout, so cost is bounded per active
agent worktree rather than by the size of each checkout.

## check-geo

`check-geo` validates the repository-native discovery assets without making any
network requests or changing product behavior. It is a separate explicit check,
not a search-ranking or indexing oracle. The corpus is bounded to 256 KiB and
20–30 unique queries, includes branded and unbranded questions, and names
ChatGPT Search, Google Search, and Bing Search separately. Each expected
canonical URL must agree with an existing, repository-contained source path.

`NOT_MEASURED` and `UNKNOWN` observations require a reason and null mention,
citation, cited URL, rank, and evidence fields. An `OBSERVED` row requires typed
outcomes and an existing evidence reference. The guard checks the shape and
reference; a reviewer must still verify that the evidence captures the named
engine, query, date, and result. A passed check does not prove that a URL is
indexed, cited, visible, or causally improved.

The corpus's pinned publication version is checked against the immutable
GitHub release summary. Explicit current-version markers in `limitations.md`
and the launch kit, the launch kit's dated npm record, and README installer
pins must agree with that evidence. Historical release files, recorded demo
transcripts, and dated experiment reports are not mechanically scanned or
rewritten. This is a local consistency check; it does not discover newer remote
releases or refresh the npm registry. Refresh the release evidence and baseline
together when a later public release is verified.

Unit tests reject stale current markers/install pins, fabricated unmeasured
results, observations without captures, duplicate queries, missing or escaping
canonical paths, malformed JSON, and oversized input. Existing release-byte
read helpers are reused; no GEO runtime dependency or telemetry is added.

## sync-agent-guides

The sync command accepts only root `AGENTS.md` or root `CLAUDE.md` as `--from`.
It copies raw bytes to the mirror file and re-checks byte equality.

## check-diff

The diff command compares two Git revisions with `git diff --name-only`. If any
`src/` path changes, at least one documentation or agent-material path must also
change. This is a minimum gate, not proof that the documentation is semantically
complete.

## Independent warm serving and full-generation comparison

The performance experiment helpers under `src/experiments/performance/` are
report-only complements to the cold-process `performance-eval` command:

```text
python3 src/experiments/performance/warm_queries.py --bin <pinned-binary> --project <frozen-initialized-copy> --out <temporary-output> --repetitions 8
python3 src/experiments/performance/compare_generations.py <baseline.sqlite> <candidate.sqlite> --out <comparison.json>
python3 src/experiments/performance/warm_queries.test.py
python3 src/experiments/performance/compare_generations.test.py
```

Warm serving pins the complete active-index fingerprint, selected target
hashes, binary/harness/helper hashes and whole response hashes. Use the same
index in both arms and compare those hashes explicitly. Native macOS/Linux
CPU/RSS covers one sequential persistent serving process; two warmups per
case/tier are discarded. Requests, partial frames and child shutdown have
bounded deadlines. Estimated tokens remain `ceil(UTF-8 bytes / 4)`.

The full-generation comparator admits all current owned SQLite tables and
checks integrity/foreign keys. It canonicalizes JSON with duplicate rejection
and compares sorted row-hash multisets, preserving duplicates. Clock/generation
identity and planner statistics are excluded. It is a strict full-index A/B
check with resource caps, not a replacement for `sync-equivalence` or
product-eval. Outputs contain hashes/counts, never source rows. See
[the measured sprint](../experiments/efficiency-sprint.md) for bounds and scope.

For private transport qualification, `performance-eval` accepts
`--python-project-session on|off` and applies the selection only to its isolated
child environment. This avoids adding a shell wrapper to the timed path. The
report records the requested state; actual worker counters must establish
whether the selected binary supports it. It is not a public product option.

The raw incremental `semantic_facts` DTO currently includes an allocator
high-water offset and is not an actual fact/write/work count. Preserve it as a
reported diagnostic; use complete stored-row counts/hashes for equivalence,
and do not rank optimizations from that field. Other unobservable host phases
remain `NOT_MEASURED`. See the
[invalidation audit](../experiments/incremental-invalidation-audit.md).

## product-eval

`product-eval` is the deterministic product-core evaluation harness. It is
report-only measurement infrastructure separate from the release gates: it
changes no production behavior and never modifies the real repository. Given a
committed query corpus (`--corpus`) it indexes each fixture in an isolated
temporary workspace and writes `product-eval-results.json` under `--out`. In the
default `product` condition it drives the product binary through each corpus
query; with `--baseline token-overlap` it instead runs a naive deterministic
control that, per fixture, only indexes (`init`+`resync`) and fetches the
`families --json` listing once, then scores each query by token overlap without
driving the product. `--condition <token>` tags the recorded condition verbatim
(for product-side ablation runs); a top-level `baseline` field records the control
independently. `--repetitions` (default 3) sets per-query latency samples and
`--bin` overrides the product binary (otherwise the sibling `repogrammar` next to
`repo-guard` is used). Mismatches
are baseline data, so the command exits `0` on completion and nonzero only on a
harness error such as a missing binary, an unparseable corpus, a subprocess
failure, or non-JSON query output. The corpus, result schema, and current
baseline reading are documented in
`docs/experiments/product-core-baseline.md`.

The committed corpus (`src/fixtures/evaluation/query-corpus-v1.json`,
`product-eval-corpus.v1`) covers the exact/path/role/natural-language retrieval
and abstention surface plus, over the nested `directory-scopes` fixture, the
directory and composite scope resolution and hard-constraint conflict behavior,
the two-sided `explain`/`check` conformance certificate, and the MCP scoped
readiness projection. A scope query gold may carry an optional
`expected.resolution_cardinality` (`one`/`many`/`none`/`truncated`) matched
against the product's additive top-level `resolution.cardinality`. Additive gold
fields, all enforced field-by-field: `expected.target_relationship`
(`MEMBER`/`NEAR_MISS`/`COMPETING_PATTERN`/`BLOCKED_UNKNOWN`/`OUT_OF_SCOPE`/`EXCEPTION`)
against a two-sided certificate's top-level `target_relationship`; and
`expected.queryability`, `expected.scope_coverage`, and
`expected.resolvable_family_count` against the scoped-readiness
`scoped_readiness.queryability`/`scope.coverage`/`scope.resolvable_family_count`;
and `expected.abstention_reason` (e.g. `margin_too_close`) against
`query_route.term_retrieval.abstention_reason`, which pins a scored
term-retrieval tie apart from a generic `InsufficientSupport` abstention.
A case may also carry an optional `against` string (the `--against`
comparison-family scope); the corpus loader rejects it on any operation other
than `explain`/`check`. The `inspect_readiness` operation has no CLI verb, so the
harness drives it over the product's read-only MCP `serve` stdio surface,
interpreting the case `target` as the `within` directory scope. The results
schema is `product-eval-results.v3` (strictly additive over v2), whose
`summary.metrics` adds `committed_precision`, `answerable_rate`,
`partial_context_rate`, `candidate_recall_at_k`, `directory_query_recall`,
`composite_query_recall`, `conflict_accuracy`, and the `unknown_by_reason`
histogram alongside the existing retrieval and safety counters. Wave 3 lands
`directory_truncated` (the `wave3-truncated` fixture's >64-file `pkg/bulk/`
scope), `same_basename_multi_directory` (the `wave3-ambiguity` fixture), and
`term_tie` (the `wave3-term-tie` fixture) with real binary gold; only
`multi_family_member` stays deferred, for a structural reason enumerated in the
corpus `_deferred_wave2_kinds` marker. See
[Testing](testing.md#directory--composite-scope-and-conflict-cases) for the full
kind taxonomy and metric definitions.

## payload-measure

`payload-measure` is the deterministic response-payload byte-measurement harness
for the response-precision policy. Like `product-eval` it is report-only
measurement infrastructure: it changes no production behavior and never modifies
the real repository. It indexes one committed fixture
(`src/fixtures/evaluation/payload-measure` by default, overridable with
`--fixture`) in an isolated temporary workspace (`init`+`resync`), then drives a
fixed query corpus and records the exact serialized response byte count plus
top-level field-group attribution per operation x category x tier (mode x
verbosity x source-spans). The corpus covers every reachable report shape on the
fixture: Found (big/small/NL/TypeScript families), abstention `UNKNOWN`,
`PARTIAL_CONTEXT`, exact family hydration, and static-alignment conformance, each
measured across `compact`/`deep` x `minimal`/`standard`/`full`; plus one
`inspect_readiness` row. The big Found family and conformance are additionally
measured at `--mode deep --include-source-spans` (tagged `source_spans: on`) so
the `read_plan` <-> `source_spans` overlap (the S6 dedup target) is measurable.
The summary also records a `fixture_shape` block (big-family `member_count`,
`members_rendered`, `members_truncated`) so fixture drift is detectable from the
artifact.

Readiness is measured through the product's MCP `serve` `inspect_readiness`
surface — the bounded, source-free readiness report — rather than the CLI
`status` lifecycle command, whose storage internals (`wal_bytes`, `shm_bytes`,
`journal_mode`, ...) are volatile and out of scope for the response-precision
policy.

It writes two artifacts under `--out`: `payload-bytes.summary.json` (the stable,
sorted, timestamp-free machine artifact) and `payload-bytes.md` (a human byte
table). The summary is a pure function of the fixture and the product binary, so
two runs against the same fixture and binary produce byte-identical
`payload-bytes.summary.json` files. `--bin` overrides the product binary
(otherwise the sibling `repogrammar` next to `repo-guard` is used). The command
exits `0` on completion and nonzero only on a harness error (missing binary,
unavailable fixture, subprocess failure, or non-JSON output).

The harness only measures; it never asserts a savings figure. A "we saved X
bytes" claim is declarable only from a before/after diff of two
`payload-bytes.summary.json` runs — one at a baseline commit and one after a
precision slice lands — over the same fixture. The before/after protocol and the
guardrail expectations are documented in `docs/development/testing.md`.

## performance-eval

`host-stage-eval` accepts the same bounded options and frozen fixtures, but
performs full-resync attribution only. It uses fresh same-source workspaces for
the same linked application pipeline with diagnostics off/on, alternates arm
order, and independently checks public CLI analysis parity. Each successful
pair compares all fourteen active owned SQLite tables through the existing
read-only snapshot oracle, including WAL-visible state. Producer hashes are
pinned before runs and rechecked each repetition. It never copies a live main
database as a substitute for a SQLite snapshot.

```text
cargo run --release --bin repo-guard -- host-stage-eval --bin target/release/repogrammar --worker src/workers/python/worker.py --python-files 16 --repetitions 3 --out <outside-repo-output>
```

Output `host-stage-results.json` uses `host-stage-eval.v1`, nesting the fixed
`host-resync-phases.v1` wall/counter projection. Ten exclusive phases sum to
the root span; separately reported unassigned time accounts for construction
outside it. Each counter has an explicit `work_scope`; undefined counts/bytes
are null. Write rows/transactions/checkpoints come from the actual session's
stats, not a semantic-facts DTO or all SQLite internal/cascade writes.
Phase CPU remains NOT_MEASURED; native total wall/user/system/RSS uses the
existing platform tool and caveats. On timing includes instrumentation overhead;
off/on differences do not represent product optimization or memory savings.

The collector is explicitly passed, single-thread, depth-bounded to16 and
disabled on normal product paths. Parser/session wrappers delegate unchanged;
EOS/freshness/validation/activation/rollback gates remain authoritative.
`host-stage-run` is an internal worker restricted to owned temporary eval
projects, with the parent's temporary-root selection explicitly propagated.
Never invoke it on a real index. The optional TypeScript worker is absent.
The default Rust Cargo-metadata adapter matches the CLI; Cargo is deliberately
not on the isolated tool PATH, so its unavailability remains typed UNKNOWN.
This is not qualification of provider execution or repository builds.
Failures retain workspaces and `host-stage-failure.json`; incomplete timings
are missing data, not zero or COMPLETE. Raw retained workspaces may contain
source and stay outside git. See [host attribution](../experiments/host-stage-attribution.md).

`performance-eval` writes `performance-results.json` (`performance-results.v1`)
from isolated copies of a committed synthetic template or an explicitly supplied
local source snapshot. It never downloads a corpus, changes a real index, starts
autosync, runs repository code, or changes agent configuration. Pin and record a
real corpus's commit/tree before supplying `--fixture`; export a self-dogfood
snapshot with `git archive`, rather than copying live build/index/Git state.

The default template expands `app.py` to `--python-files` identical module
templates; its conftest/test files are additional Python files. Each repetition
uses separate, identical native and diagnostic workspaces. Both receive the
same sequential mutations. This prevents the diagnostic pass from consuming a
change before the native measurement. Fresh `init --yes --no-autosync` includes
actual indexing, followed by repeated full resync, zero-delta sync, Python
body/comment/interface edits, file add/remove, config change, and Rust/TSJS
one-file edits. A supplied
real snapshot is left byte-identical: only full/unchanged indexing is measured;
its synthetic mutation scenarios are explicitly unavailable.

Native measurements use `/usr/bin/time -l` on macOS and `-v` on Linux with
`LC_ALL=C`. They report wall/user/system CPU and peak RSS separately from
portable product counters. RSS is the native child-rusage maximum, not the
simultaneous sum of every process's resident memory. Missing or malformed
resource output is `NOT_MEASURED`. macOS sandbox denial of `kern.clockrate`
fails the run explicitly; rerun with the requested resource access rather than
accepting partial CPU output as valid evidence. Timings are machine-dependent;
there are no absolute timing thresholds in CI.

The diagnostic pass loads only the selected, hashed RepoGrammar-owned Python
worker through `src/experiments/performance/observe_worker.py`. Delegated
binary/text reads support both legacy EOF requests and future newline sessions.
At most one 2 MiB frame is retained. Numeric aggregates record worker dispatch
count, exact input/output bytes, request modes, repeated module/conftest source
bytes and AST-parse calls. They do not count separate interpreter-version
probes. Inclusive worker-function timers overlap and include observer overhead;
they are diagnostic, excluded from native A/B measurements, and cannot be
subtracted from native wall time to infer storage time. Native and diagnostic
semantic-fact/IR/aggregate-shape fingerprints must agree. This limited observer
check omits file/unit/family-detail ledgers and is explicitly marked separately
from complete canonical analysis. It does not replace the full equivalence and
product safety gates. Diagnostic worker-load and command-wall times belong to
the instrumented pass only. Binary/worker/observer/harness hashes are pinned at
entry and rechecked before reporting; the checkout HEAD is metadata, while the
hashes identify the actual producer, including uncommitted instrumentation.

Exact family/member/path, fuzzy/abstention CLI requests and MCP
`find_analogues`/`show_family`/`inspect_readiness` requests record bytes, the shared
bytes/4 token estimate, read-plan/candidate counts, and latency samples. These
are cold launches; MCP wire bytes include initialization. They are not warm
serving latency, host token usage, savings or an adoption experiment. Detailed
host phases, rows written, live autosync and warm-agent observations remain
explicitly `NOT_MEASURED` where this harness cannot identify them.
Every query repetition must agree on canonical response content and byte
count. A compact response's exact path may come from `read_plan.items` when
selected evidence is omitted. Unavailable family/member/path loci produce
explicit `NOT_MEASURED` rows instead of disappearing from the matrix.

All product commands have an explicit timeout and bounded 16 MiB stdout/stderr.
Timeout/pipe-drain failure terminates the isolated Unix process group. Raw
source, query text and worker responses are not written into the report; local
workspace cleanup follows the existing evaluation harness. Failed or
incomplete runs are harness errors, not performance improvements.

## smoke-packaged-artifact

The packaged-artifact smoke is the executable macOS/Linux release-candidate
gate. It accepts only regular, non-symlink files for the unpacked product
binary, its bundled Python worker, and the committed Pydantic release fixture.
It runs the product with a fresh temporary HOME, XDG directories, Codex home,
repository, and tool-only PATH. It requires the worker at the product's bundled
layout and removes any worker-path override so the unpacked binary must resolve
that exact sibling worker itself. Temporary state is removed after success or
failure.

The gate proves exact version agreement before making repository state. It then
runs the packaged `instructions sync` path against an explicit `AGENTS.md` in
the isolated HOME, requires managed-contract version 3 and exact managed-block
content, and proves that this operation neither creates a `CLAUDE.md` mirror nor
repository `.repogrammar` state. It then proves explicit live `init` JSON and
repository state before running the combined `setup` compatibility path only
for the product MCP self-test, followed by explicit full `resync`, unchanged
incremental copy-forward, and the packaged `find`/advisory `check` path. It then
starts the real detached autosync daemon at a 100 ms poll interval, verifies that
readiness survives at least three poll intervals, edits the isolated fixture,
waits for a new active generation while checking daemon liveness, stops the
daemon, and requires its lock/readiness ownership to be removed. It does not
inspect or modify the developer's real HOME, agent configuration, or repository
state.

`--require-product-uninstall` adds the receipt-backed machine-lifecycle gate for
a newly built candidate. In the same isolated environment it stages the
candidate into the deterministic managed authority and command-symlink layout,
writes the product receipt through `install --target none`, and creates a
receipt-owned Codex integration and managed instruction section. It proves that
`disconnect --dry-run` is write-free, live `disconnect` removes only the agent
state while preserving the installed product, and the integration can be
installed again for the complete uninstall path. It then requires product
`uninstall --dry-run` to preserve byte-identical owned state, invokes live
`uninstall`, observes only `finalizer_pending` from the parent, waits for the
post-exit report, and requires that report to be `complete`.

The lifecycle assertions require removal of the native MCP entry, agent
receipt, managed instruction section, command symlink, both deterministic
worker copies, product receipt, and managed executable authority. They also
require preservation of unrelated instruction text, repository-local
`.repogrammar/`, telemetry and unknown global files, and an unmanaged
package-manager/PATH copy that the report lists as residual. The finalizer
helper is derived from the validated receipt-backed managed authority; neither
the smoke option nor the hidden helper accepts caller-selected deletion paths.
The option is intentionally explicit because hosts outside the declared
macOS/Linux lifecycle matrix retain the ordinary packaged-artifact smoke.

## npm candidate and final evidence

`smoke-npm-package` accepts only a bounded regular, non-symlink tarball whose
filename matches its version. It requires the exact four-file npm allowlist,
verifies package name/version/bin metadata, installs that same tarball offline
into an isolated prefix, and exercises the installed wrapper against local
checksummed fake release assets. Its deterministic JSON records the exact file
set, SHA-512/SRI, and both smoke results.

`verify-npm-pack-evidence` compares npm's pack metadata with that smoke
manifest. The finalizer must use it to prove that the fetched public tarball
matches the retained candidate before executing the public launcher.
`verify-stable-release-evidence` is the single stable final verdict. It checks
immutable GitHub state and the exact eleven assets, including the public
`npm-candidate-manifest.json`; release metadata digests; release and per-asset
attestation evidence; SHA-256 sidecars; semantically identical retained,
GitHub, and public npm candidate manifests; registry SRI; exact `latest` and
`preview` tag keys; public channel and installer versions; the exact packaged
native-smoke success line; and truthful installer, pinned, and latest live
`init` JSON.
Historical optional setup dry-run evidence is accepted only when it remains
truthful.

Each public npm launcher lane (`pinned`, `latest`, and `preview`) must execute
from its own external `${RUNNER_TEMP}` work directory, with its own HOME, npm
cache, binary cache, and tool-only PATH. That PATH must include `git` because
the public `init` smoke resolves repository identity. The launcher helper
changes directory inside a child shell so one lane cannot change the workflow
step's ambient directory. Running `npx --package` from the checked-out
RepoGrammar root is not valid evidence: npm can treat the root's same-name
`package.json` as the current package without injecting the fetched public
package's `repogrammar` bin. The guard locks the `${RUNNER_TEMP}` root, rejects
verifier definitions dispatched from a ref other than `main`, and rejects a
launcher tool list that omits `git`.

The npm provenance gate consumes only the structured output from
`npm audit signatures --json --include-attestations`. It requires one verified
`@sioyooo/repogrammar@0.5.0` entry from the exact registry and exactly one SLSA
Provenance v1 declaration. npm 11.18 reports that declaration under the
`attestations.provenance` object and provides both npm publish-v0.1 and SLSA
entries in `attestationBundles`; the guard requires that exact two-bundle
inventory, including exactly one publish-v0.1 bundle and exactly one SLSA v1
bundle, then requires an in-toto JSON DSSE payload for SLSA provenance. Its
bounded dependency-free base64 decoder binds the decoded predicate and subject
digest to the candidate SHA-512, the GitHub-hosted workflow builder to
`.github/workflows/release.yml`, the push tag to `refs/tags/v0.5.0`, the
resolved dependency URI to the same repository and tag, its git commit to the
checked-out release SHA, and the invocation identity to the exact retained
Actions run id and attempt. It does not inspect certificates, raw signature
bytes, log payloads, source files, credential values, or environment values.

## release-source

`release-source` is the workflow entry classifier. It reads bounded regular
root `package.json`, `Cargo.toml`, and `Cargo.lock` files, requires one exact
RepoGrammar version shared by all three, and emits exactly `channel=<channel>`
and `version=<version>` lines. When `GITHUB_OUTPUT` is present it safely appends
the same two lines to an existing bounded regular non-symlink output file; local
invocation without that variable remains read-only.

A `workflow_dispatch` is build-only and has no tag constraint. A `push` must
name the exact `v<version>` tag and the checked-out `HEAD` must equal
`refs/remotes/origin/main`, not merely be an ancestor. Unsupported events,
malformed manifests or refs, mismatched versions, absent publication authority,
and invalid output files fail with sanitized errors.

## release-channel

The release-channel classifier is the single typed decision point for workflow
routing. A bounded version with prerelease identifiers is `preview`; a bounded
version without prerelease identifiers is `stable`. Malformed, oversized, or
non-canonical numeric versions fail closed. Declarative workflows must not infer
the channel independently with shell substring tests.

## release-dist-tag-action

The release dist-tag classifier verifies the complete public npm state after a
publication becomes visible:

- preview preserves the existing `preview-dist-tag-action` policy;
- the registered stable `0.5.0` policy requires exact `latest=0.5.0`, exact
  `preview=0.2.0-preview.0`, and the preview, prior public `0.4.3`, and new
  stable versions in the bounded complete inventory. The failed or abandoned,
  unpublished `0.2.0`, `0.2.1`, `0.3.0`, `0.3.1`, and `0.3.2` candidates are
  explicitly forbidden; any candidate's presence in the registry inventory
  fails closed. Other stable versions fail closed until explicitly registered.

For stable, the complete dist-tag object must contain exactly `latest` and
`preview`. For preview, it must contain exactly `preview` plus `latest` only
when the registry exposes one. Extra, missing, malformed, unpublished, or
cross-channel tag state fails closed. The
only stable success action is `stable_latest_verified`. The command is read-only:
it does not authenticate to npm, mutate tags, stage a package, approve a stage,
or publish a package.

## preview-dist-tag-action

The preview dist-tag classifier is the compatibility policy entry point used by
the dual-channel release classifier; tag publication itself is stage-only and
does not invoke a public-registry classifier before human approval. The manual
npm tag-reconciliation workflow uses `release-dist-tag-action` after
publication.
It requires the manifest version to be a bounded prerelease and the `preview`
tag to match it exactly, requires that version in the bounded complete list of
published versions, and verifies that `latest` references a published version
when present. It returns `no_latest`, `preserve_stable_latest`, or the narrowly
bounded `allow_prerelease_latest_without_stable` only when every published
version is a prerelease. A prerelease-valued `latest` fails closed as soon as
any stable version exists. Missing/mismatched preview state, incomplete or
malformed version inventory, and unpublished tag targets also fail closed. The
command and declarative workflow are read-only: they do not access authenticated
package state, modify npm tags, publish a package, or synthesize a stable
version. The command remains available as the preview-only compatibility entry
point; release workflows use `release-dist-tag-action` for dual-channel final
verification.

## Staged publication boundary

Version `0.5.0` follows the [GitHub-only checklist](../release/stable-v0.5.0-release-checklist.md):
retain a successful build-only run and manually publish ten verified assets.
The installer contract tests require both the exact disabled stable-stage
condition and the preview-only automatic-draft condition. The retained stable
staging command and dual-channel finalizer are compatibility infrastructure,
not authorization to publish npm or the completion gate for this version.

A preview tag first attaches all native assets to a draft GitHub
Release. Only then does the workflow stage the exact retained npm tarball with
the protected `npm-release` environment, Trusted Publisher OIDC, Node 24, and
npm 11.18.0. Neither path reads `NPM_TOKEN`/`NODE_AUTH_TOKEN` or directly
publishes. A maintainer publishes the complete GitHub prerelease, approves the matching
npm stage with 2FA, and then runs the
read-only channel verifier.

A rerun after successful staging may fail because npm already reserved the
version. The OIDC job deliberately cannot list, download, approve, reject, or
silently reuse a pending stage. A maintainer must authenticate separately,
compare it with the retained candidate, and either continue or reject it with
2FA before retrying. CI never guesses that a pending stage matches.

Release immutability remains a maintainer preflight before tag creation. The
read-only finalizer needs no long-lived admin token: the public release API plus
`gh release verify` and every `gh release verify-asset` result are the release
evidence. The corrected post-public finalizer definition is dispatched from
`main`, but its checkout remains pinned to immutable `v0.5.0` and its inputs
must identify the exact successful `v0.5.0` tag-run attempt. Updating verifier
orchestration after tagging therefore cannot move the tag, rebuild release
artifacts, or replace publication authority. Expired retained artifacts,
unavailable attestations, absent provenance, or a failed public smoke prevents
`STABLE_RELEASE_READY`.

The already-public `v0.4.0` release and its finalizer evidence retain the
historical packaged-artifact invocation and uninstall behavior embedded in
that immutable tag. `--require-product-uninstall` is required only in current
macOS/Linux CI and the release workflow for a new patch-forward candidate; it
must not be used to reinterpret, rebuild, or retrofit `v0.4.0` evidence.

## Exit codes

- `0`: requested guard passed.
- nonzero: invalid arguments, failed Git comparison, filesystem error, or guard
  violation.

## False positives

Prefer changing the repository structure or documenting a narrower allowlist over
weakening the guard. Any guard behavior change must update this document and
include tests.

Required-document registration tests must remove each newly registered
authority document from an otherwise complete temporary repository and assert a
`RequiredDocumentMissing` violation for its exact path.

## CI integration

CI runs `repo-guard check` on every push and pull request. Pull requests also
run `check-diff` when base and head revisions are available. Native Linux and
macOS jobs, plus every supported release-matrix build, invoke
`smoke-packaged-artifact` against an unpacked candidate binary and worker. The
macOS and Linux product-lifecycle jobs, and every newly built release candidate,
must explicitly pass `--require-product-uninstall`; repository guard rejects a
workflow that omits any of those three calls. The immutable public `v0.4.0`
finalizer remains on its historical invocation until a patch-forward release
identity is built and published.
The release workflow uses `release-source`, exports its exact outputs through
`GITHUB_OUTPUT`, and runs the npm candidate evidence commands before OIDC
staging. Its draft is guarded against replacement and contains exactly eleven
assets. The guard requires both the positive draft-collision query contract and
the rejection of runner-incompatible `--slurp` plus `--jq` usage. The stable
finalizer projects the selected run attempt into eight
canonical fields, verifies the public npm pack before any public product
execution, runs each public npm channel from a separate external lane working
directory, and delegates the final verdict to `repo-guard`. Manual verification
uses `release-dist-tag-action` against public tags and the complete inventory;
stable requires exact
`latest=0.5.0`/`preview=0.2.0-preview.0`. All inconsistent states fail visibly
without registry writes. Manual release dispatch remains build-only and manual
finalization remains read-only.

The bounded provenance Base64 decoder iterates exact four-byte arrays after
rejecting non-multiple lengths. Padding and canonical trailing-bit checks also
apply to a partial final block after complete blocks; these boundaries are
covered by the standard Base64 regression. This form satisfies current stable
Clippy without suppressing its fixed-chunk lint.
