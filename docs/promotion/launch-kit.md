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

Use the no-build public installation path in the root [README](../../README.md).
The following bounded judge path needs Git, Node/npm, and `jq`, but not
Rust/Cargo, Docker, an API key, or a globally installed RepoGrammar binary. It
uses an isolated cache and the MIT-licensed
`fastapi/full-stack-fastapi-template` at commit
`4d3d5e92c1ea6b3fa0fab02c41124844ec45bca8`:

```bash
set -euo pipefail

JUDGE_ROOT="$(mktemp -d)"
JUDGE_REPO="$JUDGE_ROOT/full-stack-fastapi-template"
export npm_config_cache="$JUDGE_ROOT/npm-cache"
export REPOGRAMMAR_NPM_CACHE_DIR="$JUDGE_ROOT/repogrammar-cache"

git clone --filter=blob:none \
  https://github.com/fastapi/full-stack-fastapi-template.git "$JUDGE_REPO"
git -C "$JUDGE_REPO" checkout --detach \
  4d3d5e92c1ea6b3fa0fab02c41124844ec45bca8
test "$(git -C "$JUDGE_REPO" rev-parse HEAD)" = \
  4d3d5e92c1ea6b3fa0fab02c41124844ec45bca8

npx --yes --package @sioyooo/repogrammar@0.4.0 repogrammar version
npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar init --project "$JUDGE_REPO" \
  --no-autosync --yes --progress never

JUDGE_UNIT="$(
  npx --yes --package @sioyooo/repogrammar@0.4.0 \
    repogrammar units --project "$JUDGE_REPO" --json |
  jq -r '.units[] | select(
    .path == "backend/app/api/routes/items.py" and
    .kind == "fastapi_route" and
    (.id | contains(":read_item:"))
  ) | .id' |
  head -n 1
)"
test -n "$JUDGE_UNIT"

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar find --project "$JUDGE_REPO" \
  --mode compact --verbosity minimal "$JUDGE_UNIT"
npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar check --project "$JUDGE_REPO" \
  --mode compact --verbosity minimal "$JUDGE_UNIT"
npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar find --project "$JUDGE_REPO" --mode compact --json \
  unit:backend/app/api/routes/items.py#definitely_missing_summary_member

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar uninit --project "$JUDGE_REPO" --yes
```

This exact path was re-run from a fresh public cache on 2026-07-20. It
returned `repogrammar 0.4.0`, a 23-member Python FastAPI Route family with a
bounded required read, `PARTIAL_ALIGNMENT` with
`runtime_equivalence: UNKNOWN`, an `InsufficientSupport` typed `UNKNOWN` with
recovery for the deliberately missing member, and successful state cleanup.
It demonstrates:

1. repository setup and indexing;
2. a successful family query at `verbosity minimal`;
3. a bounded read plan;
4. static alignment with runtime equivalence still UNKNOWN;
5. a typed missing-member `InsufficientSupport` UNKNOWN; and
6. repository-state cleanup.

The recording-specific patch, target test, Codex MCP, and stale-evidence
recovery sequence are intentionally separate in the full
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

These fields are independently verified publication-phase facts.

- Exact version: `0.4.0`
- Git tag: `v0.4.0`
- Release commit: `12e07e7945a2dd5c618069dd640e826e81824297`
- Annotated tag object: `b4c02d45524637a19f81ca585b920480bc1da2b9`
- Candidate workflow: [run `29734557948`, attempt 1](https://github.com/SioYooo/RepoGrammar/actions/runs/29734557948)
- GitHub Release: [immutable `v0.4.0`](https://github.com/SioYooo/RepoGrammar/releases/tag/v0.4.0)
- Asset inventory: exactly 11 checksum- and attestation-verified assets
- npm stage ID: `e94e9612-3213-4b36-a20a-e40b0f3c289d`, human-approved
- npm package: [`@sioyooo/repogrammar@0.4.0`](https://www.npmjs.com/package/@sioyooo/repogrammar/v/0.4.0)
- npm integrity:
  `sha512-vwMEvpbBQQlIchW02Q2SRc9d97XwkYXLHKPCeHgV7MZs62qYY0mLnIOE/s5emToCLSqD12P6d9FVgLUn5K+vtQ==`
- npm provenance: SLSA provenance verified against the exact tag workflow,
  source ref, commit, candidate run, and attempt
- dist-tags: `latest=0.4.0`, `preview=0.2.0-preview.0`
- Public finalizer: [run `29747390860`, attempt 1](https://github.com/SioYooo/RepoGrammar/actions/runs/29747390860)
- Finalizer verdict: `STABLE_RELEASE_READY`

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
- public GitHub/npm availability without independently verified registry and
  finalizer evidence.

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
