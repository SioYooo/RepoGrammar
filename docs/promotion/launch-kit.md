# RepoGrammar Launch Kit

This file is the single repository authority for product identity,
positioning, one-liners, video description, and launch copy. Public release
evidence must be copied here only after it has been independently verified.

Current claims reconciled against repository release records on 2026-09-30.
Historical evidence below retains its original version and measurement limits.

## Product identity

**Name:** RepoGrammar

**One-line proposition:**

> RepoGrammar gives coding agents local, source-backed implementation-family
> context, a bounded read plan, and a typed `UNKNOWN` when the repository has
> not proved the answer.

**Repository:** <https://github.com/SioYooo/RepoGrammar>

## Problem and core insight

Coding agents repeatedly reread repository source to rediscover local routes,
fixtures, models, and data-access conventions. Search can find plausible text,
but a plausible match is dangerous when freshness, evidence strength, unresolved
semantics, and remaining source reads disappear from the answer.

RepoGrammar treats bounded source fallback and typed abstention as first-class
product behavior. Context reduction is useful only while the agent can audit
why a family was selected, which source still must be read, and which claim
remains unresolved.

## What it does

RepoGrammar builds a repository-local SQLite index and groups compatible,
source-backed implementation facts into families. Its pattern-family CLI and
single read-only MCP tool return:

- compatible family members and source-backed provenance;
- freshness, variation, exception, and unresolved-obligation metadata;
- a prioritized, hash-checked read plan;
- optional bounded source spans;
- static-alignment certificates that preserve
  `runtime_equivalence: UNKNOWN`; and
- typed `UNKNOWN` or `PARTIAL_CONTEXT` plus recovery when evidence is stale,
  ambiguous, dynamic, unsupported, or insufficient.

It is local-first, does not execute target-repository application code, and
does not call an LLM, embedding service, vector database, or cloud API in its
analysis path. A receiving coding agent has its own provider/privacy policy;
metadata and explicitly requested source spans can leave the machine through
that client. Telemetry is off by default, source-free, and uploaded only
explicitly. See the [telemetry contract](../specifications/telemetry.md).

## Why it is not grep, RAG, CodeGraph, or a generic static analyzer

| Mechanism | Complementary RepoGrammar contract |
| --- | --- |
| grep / text search locates strings | Qualifies repeated repository-local implementation families and remaining source reads |
| semantic retrieval / RAG retrieves query-relevant material, sometimes for generation | Exposes local evidence compatibility, freshness, and unresolved obligations; its analysis needs no embeddings or generation |
| CodeGraph / symbol graphs connect declarations, references, and call paths | Adds family selection and typed abstention; optional graph facts remain lower-layer evidence |
| static analysis checks under a declared semantic model | Reports bounded convention evidence and static alignment; neither sound whole-program analysis nor runtime equivalence is claimed |

These are mechanism comparisons, not measured superiority claims. See the
[product specification](../specifications/product.md) and
[optional CodeGraph boundary](../decisions/ADR-0010-optional-codegraph-provider.md).

The coherent contribution is:

```text
repository-local implementation families
+ compatible source-backed evidence
+ bounded read obligations
+ freshness enforcement
+ static-alignment certificates
+ typed abstention and recovery
```

## Architecture

```text
filesystem + Git source boundary
  -> bounded language/framework adapters and semantic workers
  -> source-backed facts and typed UNKNOWNs
  -> compatible implementation-family mining
  -> immutable SQLite generations + repo-local autosync
  -> Rust CLI + read-only repogrammar_context MCP
  -> bounded read plan / static alignment / abstention recovery
```

Tree-sitter is a syntax and candidate-generation layer, never the sole semantic
oracle. Cross-version schema/daemon checks and query-time hashes fail closed on
stale or incompatible state.

## Pre-existing foundation

Before the product-core work described below, the repository already
contained the Rust architecture, local SQLite generations, bounded Python
family mining, the pattern-family CLI, read-only MCP transport, and
conservative UNKNOWN policy. The published baseline is recorded at commit
`33715e4` in the
[product-core RC verdict](../experiments/product-core-rc-verdict.md).

## Product-core additions

The commit, code, test, and specification history records these additions:

- query resolution v2, term retrieval, and precision-first managed targets;
- calibrated family prevalence and constraint profiles;
- static-alignment certificates with runtime equivalence still UNKNOWN;
- minimal response verbosity and deterministic payload measurement;
- dependency-aware incremental sync, Python interface hashes, and a
  full/incremental equivalence oracle;
- decomposed product readiness and shared recovery classification;
- all-outcome estimated read-displacement accounting with atomic query cohorts;
- cross-version lock, daemon, and schema compatibility;
- zero-friction setup with a product MCP self-test;
- repository-local autosync after init by default; and
- release-source, packaged-artifact, checksum, provenance, and public-finalizer
  hardening.

The exact mapping lives in the [CHANGELOG](../../CHANGELOG.md),
[v0.4.3 release checklist](../release/stable-v0.4.3-release-checklist.md), and
[RC verdict](../experiments/product-core-rc-verdict.md).

## Historical GPT-5.6 and Codex usage

- **ChatGPT on GPT-5.6:** planning, review, scope refinement, and claim audit.
- **Codex on GPT-5.6:** implementation, tests, documentation, release tooling,
  and release coordination against repository gates.
- **Human maintainer:** core insight, architecture, evidence policy, scope,
  review, merge authority, and protected public approvals.

RepoGrammar itself does not call GPT-5.6 or the OpenAI API. GPT-5.6 was the
development and demo reasoning surface; RepoGrammar supplied local repository
evidence to the coding agent.

## Five-minute walkthrough

Use the exact public installer and commands in the root [README](../../README.md).
The current quickstart pins the immutable `v0.5.0` `install.sh`, installs only the
product binary and bundled worker, optionally wires a coding agent with
`repogrammar install`, and initializes each repository separately with
`repogrammar init`. It does not require Rust or Cargo.
The existing [FastAPI/pytest fixture walkthrough](../examples/python-fastapi-pytest.md)
provides a source-checkout example with expected output shapes and negative
controls. Use the actual selected build's output for any new recording. A
useful technical demonstration should show:

1. binary installation followed by separate repository initialization and indexing;
2. a successful family query at `verbosity minimal`;
3. a bounded read plan;
4. static alignment with runtime equivalence still UNKNOWN;
5. a typed unsupported-query UNKNOWN; and
6. repository-state cleanup.

The [demo runbook](../demo/demo-runbook.md) is a historical recording plan pinned
to npm `0.4.0` and the MIT-licensed FastAPI template at commit
`4d3d5e92c1ea6b3fa0fab02c41124844ec45bca8`. Its pinned evidence is preserved;
it is not proof of a recorded video or a `0.5.0` rehearsal. Current release
recording requires a separately verified `0.5.0` flow before publication.

## Evidence boundaries and limitations

- RepoGrammar is pre-1.0. The MCP API and preview analyzers remain experimental.
- Python FastAPI, pytest, Pydantic, and SQLAlchemy are the official bounded
  family path. Other indexed language paths are narrower previews or discovery
  only; discovery is not support.
- Static alignment never proves runtime equivalence or behavioral conformance.
- `estimated_potential_token_savings` is estimated potential read displacement,
  not measured savings or a causal effect.
- The mechanics-only small-model pilot connected the MCP server in four
  treatment runs but observed `0/4` proactive RepoGrammar tool calls. The demo
  explicitly instructs the agent to use RepoGrammar and does not claim
  spontaneous adoption.
- Autosync is a best-effort convenience. Query-time hash checks reject stale
  evidence and explicit sync is the authoritative refresh path.
- Public artifacts cover the documented macOS and glibc Linux targets; Windows
  and musl are not public release targets.

See [limitations](../limitations.md) and the
[agent-study pilot](../experiments/agent-study-pilot.md) for the complete
boundaries.

## Current publication truth

Current GitHub stable: `0.5.0`.
Current npm stable at last verification: `0.4.3`.

| Channel | Verified publication | Evidence and freshness boundary |
| --- | --- | --- |
| GitHub stable | Immutable `v0.5.0`, ten assets, published 2026-09-09 | [checklist](../release/stable-v0.5.0-release-checklist.md), [summary](../release/stable-v0.5.0-release.summary.json), [public release](https://github.com/SioYooo/RepoGrammar/releases/tag/v0.5.0) |
| npm stable | `@sioyooo/repogrammar@0.4.3` | [historical finalizer evidence](../release/stable-v0.4.3-release-checklist.md); [registry metadata](https://registry.npmjs.org/@sioyooo%2Frepogrammar) rechecked read-only on 2026-09-30: `latest=0.4.3` |
| npm preview | `0.2.0-preview.0` | Same 2026-09-30 metadata check: `preview=0.2.0-preview.0` |
| npm `0.5.0` | Not published by the GitHub-only release | [v0.5.0 summary](../release/stable-v0.5.0-release.summary.json), `npm.published=false` |

The bounded registry JSON check on 2026-09-30 also found no `0.5.0` version.
This refresh checks availability metadata only; the integrity/provenance proof
for `0.4.3` remains its historical finalizer evidence.

This is a projection of the cited release evidence, not another publication
gate. `GITHUB_RELEASE_READY` is the authorized v0.5.0 gate; the older
`STABLE_RELEASE_READY` finalizer was a dual-channel gate. Historical npm
availability does not prove the current GitHub version exists on npm.

## Historical v0.4.3 public release evidence

These are historical publication-phase facts, not the current release identity.
Do not rewrite them to suggest npm `0.5.0` publication.

- Exact version: `0.4.3`
- Git tag: `v0.4.3`
- Release commit: `c0d72bb48f0edaed0a15ea1eb7ccbd01df0fa1b0`
- Annotated tag object: `eb4544012a0addf6ef36375d1a9893df266a29ef`
- Candidate workflow run and attempt:
  [`29870606932`](https://github.com/SioYooo/RepoGrammar/actions/runs/29870606932),
  attempt `1`
- GitHub Release:
  [immutable `v0.4.3`](https://github.com/SioYooo/RepoGrammar/releases/tag/v0.4.3)
- Asset inventory: exactly 11 public assets; all checksum and GitHub
  release/asset attestations verified
- npm stage ID: `82c8ebae-e4de-43c8-8155-64694762d952`, approved through the
  maintainer 2FA boundary
- npm package and integrity:
  [`@sioyooo/repogrammar@0.4.3`](https://www.npmjs.com/package/@sioyooo/repogrammar/v/0.4.3),
  `sha512-G2pzS0CAjxn1kornK2yLLgmqL/ZGuYDCMMus5Fc+UM+uWiBHFvPIzCXCRiN28Nlks/TDNYD/dLQehfXWUvQDiA==`
- npm provenance: verified SLSA provenance bound to the release workflow,
  `refs/tags/v0.4.3`, the release commit, and candidate run
- dist-tags: `latest=0.4.3`, `preview=0.2.0-preview.0`
- Public finalizer run:
  [`29871676832`](https://github.com/SioYooo/RepoGrammar/actions/runs/29871676832)
- Finalizer verdict: `STABLE_RELEASE_READY`

## Claim guardrails

| Claims we may say, with scope | Evidence | Claims we must not say |
| --- | --- | --- |
| Local source-backed implementation-family context and bounded read plans | [product contract](../specifications/product.md), [fixture walkthrough](../examples/python-fastapi-pytest.md) | Universal repository understanding or guaranteed correct edits |
| Typed `UNKNOWN` and explicit remaining obligations | [UNKNOWN contract](../specifications/unknowns.md) | Hallucination prevention or guessing through unresolved evidence |
| Static alignment; runtime equivalence stays `UNKNOWN` | [product contract](../specifications/product.md), [limitations](../limitations.md) | Proven runtime equivalence or behavioral conformance |
| Official bounded Python family path; other lanes are narrower and incomplete | [language audit](../reports/language-support/top-20-final-program-audit.md) | Full Python semantics, Top-20 support, or parser presence as support |
| v0.5.0 GitHub-only public binaries for documented macOS/glibc Linux targets | [release summary](../release/stable-v0.5.0-release.summary.json) | npm 0.5.0, Windows, musl, or expanded language readiness |
| No LLM/cloud calls in RepoGrammar's analysis path | [product contract](../specifications/product.md), [telemetry contract](../specifications/telemetry.md) | No data ever leaves the receiving agent session |
| Estimated potential read displacement | [metrics contract](../specifications/metrics.md) | Measured, typical, average, expected, or proven token savings |
| A search measurement corpus exists; discovery results are `NOT_MEASURED` | [GEO research](geo-research.md), [corpus](geo-query-corpus.json) | Improved rankings/citations, fabricated users, or an opaque visibility score |

Use:

- “source-backed implementation-family context”;
- “bounded read plan”;
- “typed `UNKNOWN` with source fallback”;
- “static-alignment certificate; runtime equivalence UNKNOWN”;
- “estimated potential read displacement”; and
- “local product MCP self-test passed” only when that exact fact was verified.

One exception is authorized: the 52% token reduction seen in a single recorded
demo run may be reported when it is attributed to that one run and carries its
limits, as `../reports/public-preview-growth-readiness.md` records. It is an
observation, not a measurement: one demo session, no saved run artifact, no
committed controlled pair. The drafts below omit that number.

Do not claim:

- measured token savings, or any percentage reduction stated as measured,
  benchmarked, typical, or expected rather than as the single-run observation
  above;
- hallucination prevention;
- proven conformance or runtime equivalence;
- sound/complete static analysis;
- production readiness or 1.0 API stability;
- unsupported platform/language coverage; or
- channel availability beyond the exact version and evidence in the current
  publication table.

## Canonical links for external publication

- Project and pinned install: [README](https://github.com/SioYooo/RepoGrammar#readme).
- Current release: [v0.5.0](https://github.com/SioYooo/RepoGrammar/releases/tag/v0.5.0).
- Mechanism and non-claims: [product specification](https://github.com/SioYooo/RepoGrammar/blob/main/docs/specifications/product.md).
- Reproducible example: [FastAPI/pytest fixture walkthrough](https://github.com/SioYooo/RepoGrammar/blob/main/docs/examples/python-fastapi-pytest.md).
- Boundaries: [limitations](https://github.com/SioYooo/RepoGrammar/blob/main/docs/limitations.md), [language audit](https://github.com/SioYooo/RepoGrammar/blob/main/docs/reports/language-support/top-20-final-program-audit.md).
- Integration: [Codex](https://github.com/SioYooo/RepoGrammar/blob/main/docs/quickstart-codex.md), [Claude Code](https://github.com/SioYooo/RepoGrammar/blob/main/docs/quickstart-claude.md).

Before publishing a draft, recheck these public URLs against the reviewed
commit. Local edits are not live public content. Evidence citations can pin a
reviewed commit; onboarding links can follow `main`. Attribute authorship to
the actual maintainer, not an invented team, user, or testimonial.

## Publication drafts

Prepared on 2026-09-30 for human review; none has been submitted. Publish only
where community rules permit, disclose the author's project affiliation, and
use one relevant draft rather than cross-posting identical text everywhere.

### Canonical project description

RepoGrammar indexes repeated repository-local implementation patterns and gives
coding agents source-backed family context, a bounded read plan, freshness
information, and typed `UNKNOWN` when the evidence is insufficient. It runs
locally, exposes a pattern-first CLI and read-only MCP, and focuses on bounded
Python FastAPI, pytest, Pydantic, and SQLAlchemy families. Other language paths
remain narrower and incomplete. See the [project](https://github.com/SioYooo/RepoGrammar#readme)
and [limitations](https://github.com/SioYooo/RepoGrammar/blob/main/docs/limitations.md).

### Technical launch post

Title: RepoGrammar: repository-local implementation families for coding agents

A coding agent can find a route by searching for its decorator. The next
question is whether it follows the same local pattern as its peers, what
variation is legitimate, and which source still needs reading before a change.
RepoGrammar indexes repeated implementations and returns that evidence as
family context, a bounded read plan, and unresolved obligations.

The [FastAPI/pytest walkthrough](https://github.com/SioYooo/RepoGrammar/blob/main/docs/examples/python-fastapi-pytest.md)
shows a three-route fixture, source-free metadata by default, optional bounded
spans, and negative cases. `UNKNOWN` is an intended result for insufficient or
stale evidence. Static alignment does not prove runtime equivalence.

The current release is [v0.5.0 on GitHub](https://github.com/SioYooo/RepoGrammar/releases/tag/v0.5.0),
with macOS and glibc Linux binaries and a Python worker. Installation and agent
connection are separate steps in the [README](https://github.com/SioYooo/RepoGrammar#readme).
RepoGrammar's analysis has no LLM or embedding-service calls. The receiving
agent's data policy still applies. Python family analysis is bounded; the
strict Top-20 program remains 0/20 complete. No controlled token-saving result
is claimed. Concrete counterexamples and reproduction reports are useful.

### Hacker News-style draft

Title: Show HN: RepoGrammar, local implementation-family context for coding agents

RepoGrammar is a local CLI and read-only MCP that indexes repeated
implementations and returns source-backed examples, freshness checks, and a
bounded read plan. It focuses on Python FastAPI/pytest/Pydantic/SQLAlchemy
families and returns typed `UNKNOWN` when evidence is insufficient. This is
convention evidence, not runtime-equivalence proof or full semantic analysis.

The [fixture walkthrough](https://github.com/SioYooo/RepoGrammar/blob/main/docs/examples/python-fastapi-pytest.md)
includes negative controls. [GitHub v0.5.0](https://github.com/SioYooo/RepoGrammar/releases/tag/v0.5.0)
has macOS/glibc Linux binaries; npm is a separate older channel. Install steps
and the evidence boundaries are in the [project](https://github.com/SioYooo/RepoGrammar#readme).

### Developer-community draft

Title: How do you decide which existing implementation an agent should follow?

I work on [RepoGrammar](https://github.com/SioYooo/RepoGrammar#readme), a local
tool for this narrow problem. Search locates plausible examples; we try to
keep the compatibility evidence, exceptions, freshness, and remaining source
reads visible when an agent chooses among them.

The [fixture example](https://github.com/SioYooo/RepoGrammar/blob/main/docs/examples/python-fastapi-pytest.md)
uses FastAPI routes and pytest fixtures. Low support, dynamic wrappers, and
ambiguous fixtures should stay unresolved rather than become confident family
matches. The result does not certify correct behavior, and we have no controlled
token-saving result. The official Python path is bounded; other language
frontends are not full support.

I'd be interested in concrete examples where this distinction matters, or
where a read plan misses a source dependency. A small positive case paired
with a lookalike that must abstain is more useful than an endorsement.

### Short social announcement

RepoGrammar v0.5.0 is available on GitHub for macOS/glibc Linux: local
implementation-family context, bounded read plans, and typed UNKNOWN for coding
agents. Bounded Python scope; no measured token-saving claim.
https://github.com/SioYooo/RepoGrammar

### Headline variants

1. RepoGrammar: repository-local implementation families for coding agents
2. A bounded read plan before a coding agent edits
3. Finding local route and fixture conventions with evidence
4. When a code-pattern tool should answer UNKNOWN
5. Source-backed examples without an embedding service in the analysis path
6. Static alignment with runtime equivalence left unresolved
7. Connecting Codex and Claude Code to local family context
8. A FastAPI fixture walkthrough with negative controls

## Launch checklist

```text
Public demo video: <PENDING HUMAN VIDEO WORK>
```

- [ ] Verify a current-version recording flow; preserve the historical demo runbook's pins.
- [ ] Add English voice, captions, and editing.
- [ ] Upload the video to YouTube and verify signed-out access.
- [ ] Replace the placeholder above with the public video URL.
