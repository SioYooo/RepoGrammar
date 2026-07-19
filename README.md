# RepoGrammar

**Local-first, source-backed repository context for coding agents.**

Coding agents repeatedly reread repository source to rediscover local
implementation conventions. RepoGrammar builds a local map of implementation
families, returns a bounded read plan before broad source inspection, and emits
a typed `UNKNOWN` when repository evidence cannot justify a confident answer.

The core insight is that context reduction is useful only while evidence
strength, freshness, unresolved semantics, and remaining source-reading
obligations stay explicit. Bounded fallback and typed abstention are product
behavior, not retrieval failures hidden behind a plausible result.

```text
repository source
  -> local index and compatible implementation families
  -> freshness and evidence gates
  -> metadata-first family context + bounded read plan
  -> static-alignment certificate or typed UNKNOWN + recovery
```

## Why this is not another search tool

| Tool shape | What it can answer | RepoGrammar's additional contract |
| --- | --- | --- |
| grep / text search | Where a string occurs | Which repeated implementations have compatible source-backed evidence |
| semantic search / RAG | Which chunks look relevant | What still must be read, what is stale, and when the result must abstain |
| CodeGraph | Symbol and relationship graph context | Repository-local implementation families; CodeGraph is only an optional lower-layer provider |
| generic static analysis | Program facts under its analysis model | Bounded convention evidence and static alignment without claiming sound whole-program or runtime equivalence |

RepoGrammar's product identity is:

```text
repository-local implementation families
+ compatible source-backed evidence
+ bounded read obligations
+ freshness enforcement
+ static-alignment certificates
+ typed abstention and recovery
```

## Five-minute judge path — no Rust or Cargo

RepoGrammar `0.4.0` is usable through npm only after both public registries pass
these checks. The source manifest, a Git tag, a GitHub draft, or a green build
alone does not prove public availability.

```bash
npm view @sioyooo/repogrammar@0.4.0 version
npm view @sioyooo/repogrammar dist-tags --json
curl -fsSI https://github.com/SioYooo/RepoGrammar/releases/download/v0.4.0/install.sh.sha256
npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar version
```

Continue only when the exact version exists, GitHub returns the immutable
release asset, and the dist-tags are `latest=0.4.0` and
`preview=0.2.0-preview.0`.

The evaluator below uses the MIT-licensed
[`fastapi/full-stack-fastapi-template`](https://github.com/fastapi/full-stack-fastapi-template)
at an immutable commit. It does not execute target-repository application code.

```bash
JUDGE_ROOT="$(mktemp -d)"
git clone --filter=blob:none https://github.com/fastapi/full-stack-fastapi-template.git \
  "$JUDGE_ROOT/full-stack-fastapi-template"
git -C "$JUDGE_ROOT/full-stack-fastapi-template" checkout \
  4d3d5e92c1ea6b3fa0fab02c41124844ec45bca8

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar setup \
  --project "$JUDGE_ROOT/full-stack-fastapi-template" \
  --target auto --dry-run --no-autosync --progress never

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar init \
  --project "$JUDGE_ROOT/full-stack-fastapi-template" \
  --yes --no-autosync --progress never

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar find "FastAPI route" \
  --project "$JUDGE_ROOT/full-stack-fastapi-template" \
  --mode compact --verbosity minimal

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar check "backend/app/api/routes/items.py:49" \
  --project "$JUDGE_ROOT/full-stack-fastapi-template" \
  --mode compact --verbosity minimal

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar find "definitely_missing_repo_pattern" \
  --project "$JUDGE_ROOT/full-stack-fastapi-template" \
  --mode compact --json

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar uninit \
  --project "$JUDGE_ROOT/full-stack-fastapi-template" --yes
rm -rf "$JUDGE_ROOT"
```

The judge path first executes a no-write preview of the complete `setup` plan,
then uses repository-only `init`, so it does not modify global agent
configuration. `uninit` removes the local index before the disposable clone is
deleted. The successful query should return a supported family plus a bounded
`read_plan`. `check` returns a static-alignment token while preserving
`runtime_equivalence: UNKNOWN`. The deliberately unsupported query returns a
typed `UNKNOWN` and source-fallback recovery. Exact IDs, hashes, generation
names, and estimated token counts are intentionally not asserted because they
are repository- and generation-dependent.

For the recording task and stale-evidence recovery path, use the
[Build Week demo runbook](https://github.com/SioYooo/RepoGrammar/blob/main/docs/demo/build-week-demo.md). For a permanent managed
command instead of repeated `npx`, use the verified `install.sh` asset described
in the [quickstart](https://github.com/SioYooo/RepoGrammar/blob/main/docs/quickstart.md).

## How it works

RepoGrammar ships a pattern-family-first Rust CLI and one read-only MCP tool,
`repogrammar_context`.

1. Language adapters and bounded semantic workers extract local structural
   facts without executing target-repository code.
2. Tree-sitter proposes candidates; compatible exact-anchor evidence, not
   syntax similarity alone, qualifies a family.
3. Results return repo-relative evidence metadata, hashes, bounded line/byte
   ranges, unresolved obligations, and a prioritized read plan. Source spans
   require explicit opt-in.
4. Stale, ambiguous, dynamic, unsupported, or insufficient evidence becomes
   `UNKNOWN` or `PARTIAL_CONTEXT`, with a recovery action.
5. Query-time hash checks reject stale evidence. Explicit sync authoritatively
   refreshes the local SQLite index; repo-local autosync is a best-effort
   convenience, not a freshness proof.

Each repository owns its `.repogrammar/` state and optional daemon. RepoGrammar
does not run a global repository scanner. `init` starts that repository's
autosync by default; `init --no-autosync` is the deterministic one-shot path.

## Pre-existing foundation and Build Week additions

Before the Build Week product-core line, RepoGrammar already had a Rust core,
local SQLite generations, bounded Python family mining, a pattern-family CLI,
and a read-only MCP surface.

The auditable Build Week delta added query resolution v2 and term retrieval,
constraint profiles, static-alignment certificates, minimal verbosity,
deterministic payload measurement, dependency-aware incremental sync and Python
interface hashes, full/incremental equivalence gates, decomposed readiness,
all-outcome estimated read-displacement accounting, cross-version compatibility,
zero-friction setup, precision-first managed instructions, and default
repository-local autosync after init. The
[release checklist](https://github.com/SioYooo/RepoGrammar/blob/main/docs/release/stable-v0.4.0-release-checklist.md),
[RC verdict](https://github.com/SioYooo/RepoGrammar/blob/main/docs/experiments/product-core-rc-verdict.md), and
[CHANGELOG](https://github.com/SioYooo/RepoGrammar/blob/main/CHANGELOG.md) map those claims to code, tests, and commits.

The mechanics-only agent-study pilot connected RepoGrammar in four treatment
runs but observed `0/4` proactive MCP calls from the small headless model. That
is an adoption finding, not an impact result. The demo therefore instructs the
agent to use RepoGrammar and does not claim spontaneous adoption or measured
token savings.

## Support and limitations

| Language | Current evidence boundary |
| --- | --- |
| Python — FastAPI, pytest, Pydantic, SQLAlchemy | Bounded framework-family context; official Python-first scope |
| TypeScript / JavaScript, Rust, Java/Spring, C#, C/C++ | Conservative structural or exact-anchor preview boundaries |
| Go, PHP, Ruby, Swift | File discovery only; not analyzed or supported |

The source identity is `0.4.0`; that manifest value is not publication proof.
RepoGrammar is pre-1.0, its MCP API and preview analyzers remain experimental,
and it is not a sound whole-program static analyzer or a runtime-equivalence
oracle. `estimated_potential_token_savings` is an **estimated** potential
read-displacement diagnostic, not measured savings or a causal claim. The
[limitations](https://github.com/SioYooo/RepoGrammar/blob/main/docs/limitations.md) state the exact evidence and platform
boundaries.

Stable artifacts cover macOS arm64/x86_64 and glibc Linux arm64/x86_64 at the
documented minimum versions. Windows and musl are not public release targets.

## GPT-5.6 and Codex usage

- **ChatGPT on GPT-5.6** helped the maintainer plan, review, refine scope, and
  audit claims.
- **Codex on GPT-5.6** implemented and tested Rust, adapters, CLI/MCP behavior,
  release tooling, and documentation against repository gates.
- **The human maintainer** owns the core insight, architecture, evidence policy,
  scope, review, merge authority, and public approvals.

RepoGrammar itself does not call GPT-5.6, the OpenAI API, embeddings, a vector
database, or any cloud model. It is the local evidence tool used by the coding
agent.

## License

RepoGrammar is licensed under the [MIT License](https://github.com/SioYooo/RepoGrammar/blob/main/LICENSE).
