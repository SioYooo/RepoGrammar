# Repository Guard

`repo-guard` is a repository governance CLI implemented in
`src/rust/bin/repo_guard.rs`. It is separate from the RepoGrammar product runtime.

## Commands

```text
cargo run --quiet --bin repo-guard -- check
cargo run --quiet --bin repo-guard -- sync-agent-guides --from AGENTS.md
cargo run --quiet --bin repo-guard -- sync-agent-guides --from CLAUDE.md
cargo run --quiet --bin repo-guard -- check-diff --base <git-revision> --head <git-revision>
cargo run --quiet --bin repo-guard -- product-eval --corpus <path> --out <dir> [--repetitions <n>] [--bin <path>] [--condition <token>] [--baseline token-overlap]
cargo run --quiet --bin repo-guard -- payload-measure --out <dir> [--bin <path>] [--fixture <repo-relative-fixture-root>]
cargo run --quiet --bin repo-guard -- smoke-packaged-artifact --binary <path> --worker <path> --fixture <path> --expected-version <version>
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
  ADR-0020 Top-20 language expansion gate plus its active implementation plan.
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
- the release workflow classifies preview versus stable through `repo-guard`,
  keeps manual dispatch build-only, and creates one exact npm candidate that
  downstream jobs download rather than repack;
- preview and stable both create draft GitHub Releases and then use Node 24,
  npm 11.18.0, and Trusted Publisher OIDC to stage that candidate. They contain
  no traditional npm token, direct publish, approval, rejection, or dist-tag
  mutation authority;
- preview staging has one registered assignment for its exact
  `./npm-candidate/...tgz` local tarball, and stable staging has one registered
  literal command for `./npm-candidate/sioyooo-repogrammar-0.3.2.tgz`; a bare
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

## sync-agent-guides

The sync command accepts only root `AGENTS.md` or root `CLAUDE.md` as `--from`.
It copies raw bytes to the mirror file and re-checks byte equality.

## check-diff

The diff command compares two Git revisions with `git diff --name-only`. If any
`src/` path changes, at least one documentation or agent-material path must also
change. This is a minimum gate, not proof that the documentation is semantically
complete.

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
repository `.repogrammar` state. It also proves truthful setup dry-run and live
setup JSON, the product MCP self-test, explicit full `resync`, unchanged
incremental copy-forward, and the packaged `find`/advisory `check` path. It then
starts the real detached autosync daemon at a 100 ms poll interval, verifies that
readiness survives at least three poll intervals, edits the isolated fixture,
waits for a new active generation while checking daemon liveness, stops the
daemon, and requires its lock/readiness ownership to be removed. It does not
inspect or modify the developer's real HOME, agent configuration, or repository
state.

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
native-smoke success line; and truthful pinned/latest live setup JSON.
Historical optional setup dry-run evidence is accepted only when it remains
truthful.

Each public npm launcher lane (`pinned`, `latest`, and `preview`) must execute
from its own external `${RUNNER_TEMP}` work directory, with its own HOME, npm
cache, binary cache, and tool-only PATH. That PATH must include `git` because
the public setup smoke performs repository initialization. The launcher helper
changes directory inside a child shell so one lane cannot change the workflow
step's ambient directory. Running `npx --package` from the checked-out
RepoGrammar root is not valid evidence: npm can treat the root's same-name
`package.json` as the current package without injecting the fetched public
package's `repogrammar` bin. The guard locks the `${RUNNER_TEMP}` root, rejects
verifier definitions dispatched from a ref other than `main`, and rejects a
launcher tool list that omits `git`.

The npm provenance gate consumes only the structured output from
`npm audit signatures --json --include-attestations`. It requires one verified
`@sioyooo/repogrammar@0.3.2` entry from the exact registry and exactly one SLSA
Provenance v1 declaration. npm 11.18 reports that declaration under the
`attestations.provenance` object and provides both npm publish-v0.1 and SLSA
entries in `attestationBundles`; the guard requires that exact two-bundle
inventory, including exactly one publish-v0.1 bundle and exactly one SLSA v1
bundle, then requires an in-toto JSON DSSE payload for SLSA provenance. Its
bounded dependency-free base64 decoder binds the decoded predicate and subject
digest to the candidate SHA-512, the GitHub-hosted workflow builder to
`.github/workflows/release.yml`, the push tag to `refs/tags/v0.3.2`, the
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
- the registered stable `0.3.2` policy requires exact `latest=0.3.2`, exact
  `preview=0.2.0-preview.0`, and both versions in the bounded complete
  inventory. The failed, unpublished `0.2.0` and `0.2.1` candidates are
  explicitly forbidden; either candidate's presence in the registry inventory
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

A preview or stable tag first attaches all native assets to a draft GitHub
Release. Only then does the workflow stage the exact retained npm tarball with
the protected `npm-release` environment, Trusted Publisher OIDC, Node 24, and
npm 11.18.0. Neither path reads `NPM_TOKEN`/`NODE_AUTH_TOKEN` or directly
publishes. A maintainer publishes the complete GitHub release (as a prerelease
for preview), approves the matching npm stage with 2FA, and then runs the
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
`main`, but its checkout remains pinned to immutable `v0.2.2` and its evidence
remains bound to candidate run `29586694524`, attempt 1. Updating verifier
orchestration therefore does not move the tag, rebuild release artifacts, or
replace publication authority. Expired retained artifacts, unavailable
attestations, absent provenance, or a failed public smoke prevents
`STABLE_RELEASE_READY`.

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
`smoke-packaged-artifact` against an unpacked candidate binary and worker.
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
`latest=0.3.2`/`preview=0.2.0-preview.0`. All inconsistent states fail visibly
without registry writes. Manual release dispatch remains build-only and manual
finalization remains read-only.
