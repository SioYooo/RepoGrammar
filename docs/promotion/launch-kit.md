# RepoGrammar Build Week Submission Kit

This file is the single repository authority for Devpost, video-description,
and launch copy. Public release evidence must be copied here only after it has
been independently verified.

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
does not call an LLM, embedding service, vector database, or cloud API.

## Why it is not grep, RAG, CodeGraph, or a generic static analyzer

| Comparison | RepoGrammar difference |
| --- | --- |
| grep / semantic search | Qualifies repeated repository-local implementation families instead of returning text matches alone |
| RAG | Keeps evidence compatibility, freshness, unresolved semantics, and source-reading obligations explicit |
| CodeGraph | Uses graph facts only as optional lower-layer evidence; the product contract is family selection and abstention |
| generic static analysis | Reports bounded convention evidence and static alignment without claiming sound whole-program or runtime equivalence |

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

Before the Build Week product-core line, the repository already contained the
Rust architecture, local SQLite generations, bounded Python family mining, the
pattern-family CLI, read-only MCP transport, and conservative UNKNOWN policy.
The published baseline is recorded at commit `33715e4` in the
[product-core RC verdict](../experiments/product-core-rc-verdict.md).

## Build Week additions

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
[v0.4.0 release checklist](../release/stable-v0.4.0-release-checklist.md), and
[RC verdict](../experiments/product-core-rc-verdict.md).

## GPT-5.6 and Codex usage

- **ChatGPT on GPT-5.6:** planning, review, scope refinement, and claim audit.
- **Codex on GPT-5.6:** implementation, tests, documentation, release tooling,
  and release coordination against repository gates.
- **Human maintainer:** core insight, architecture, evidence policy, scope,
  review, merge authority, and protected public approvals.

RepoGrammar itself does not call GPT-5.6 or the OpenAI API. GPT-5.6 was the
development and demo reasoning surface; RepoGrammar supplied local repository
evidence to the coding agent.

## Five-minute judge path

Use the exact public package and commands in the root [README](../../README.md).
Every command is pinned to `@sioyooo/repogrammar@0.4.0` and runs through `npx`,
so the path does not require Rust/Cargo or assume a globally installed binary.
It clones the MIT-licensed `fastapi/full-stack-fastapi-template` at commit
`4d3d5e92c1ea6b3fa0fab02c41124844ec45bca8`, then demonstrates:

1. repository setup and indexing;
2. a successful family query at `verbosity minimal`;
3. a bounded read plan;
4. static alignment with runtime equivalence still UNKNOWN;
5. a typed unsupported-query UNKNOWN; and
6. repository-state cleanup.

The recording-specific task and stale-evidence recovery sequence are in the
[demo runbook](../demo/build-week-demo.md).

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

## Public release evidence

These fields are publication-phase facts, not source-state claims. Replace
them only after the exact public finalizer emits `STABLE_RELEASE_READY`.

- Exact version: `0.4.0`
- Git tag: `v0.4.0`
- Tag SHA: `<PENDING PUBLICATION EVIDENCE>`
- Candidate workflow run and attempt: `<PENDING PUBLICATION EVIDENCE>`
- GitHub Release: `<PENDING PUBLICATION EVIDENCE>`
- Asset inventory: `<PENDING PUBLICATION EVIDENCE>`
- npm stage ID: `<PENDING HUMAN-APPROVED STAGE>`
- npm package and integrity: `<PENDING PUBLICATION EVIDENCE>`
- npm provenance: `<PENDING PUBLICATION EVIDENCE>`
- dist-tags: expected `latest=0.4.0`, `preview=0.2.0-preview.0`
- Public finalizer run: `<PENDING PUBLICATION EVIDENCE>`
- Finalizer verdict: `<PENDING STABLE_RELEASE_READY>`

## Claim guardrails

Use:

- “source-backed implementation-family context”;
- “bounded read plan”;
- “typed `UNKNOWN` with source fallback”;
- “static-alignment certificate; runtime equivalence UNKNOWN”;
- “estimated potential read displacement”; and
- “local product MCP self-test passed” only when that exact fact was verified.

Do not claim:

- measured token savings or a percentage reduction;
- hallucination prevention;
- proven conformance or runtime equivalence;
- sound/complete static analysis;
- production readiness or 1.0 API stability;
- unsupported platform/language coverage; or
- public GitHub/npm availability before the registry and finalizer evidence
  above exists.

## Human-only submission fields

```text
Public YouTube demo: <PENDING HUMAN VIDEO WORK>
Codex /feedback Session ID: <PENDING HUMAN ACTION>
Devpost submission URL: <PENDING HUMAN SUBMISSION>
```

## Final human checklist

- [ ] Record from the exact current demo runbook.
- [ ] Add English voice, captions, and editing.
- [ ] Upload the video to YouTube and verify signed-out access.
- [ ] Run `/feedback` in the appropriate Codex session and copy the accepted
      Session ID; do not infer one from a local identifier.
- [ ] Paste the final text from this file into Devpost and submit.
- [ ] Replace only the three human-only placeholders above with the public
      video URL, Session ID, and Devpost submission URL.
