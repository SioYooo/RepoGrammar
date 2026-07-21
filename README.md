<h1 align="center">RepoGrammar</h1>

<p align="center">
  <strong>Give coding agents your repository's conventions—not another pile of search results.</strong>
</p>

<p align="center">
  Local-first, source-backed pattern context with bounded read plans and honest abstention.
</p>

<p align="center">
  <a href="https://github.com/SioYooo/RepoGrammar/releases/tag/v0.4.1"><img alt="Stable version 0.4.1" src="https://img.shields.io/badge/stable-0.4.1-7c3aed?style=flat-square"></a>
  <img alt="Local first" src="https://img.shields.io/badge/context-local--first-0f766e?style=flat-square">
  <img alt="Read-only MCP" src="https://img.shields.io/badge/MCP-read--only-2563eb?style=flat-square">
  <a href="https://github.com/SioYooo/RepoGrammar/blob/main/LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-f59e0b?style=flat-square"></a>
</p>

<p align="center">
  <a href="https://github.com/SioYooo/RepoGrammar/blob/main/docs/quickstart.md">Quickstart</a>
  ·
  <a href="https://github.com/SioYooo/RepoGrammar/tree/main/docs">Documentation</a>
  ·
  <a href="https://github.com/SioYooo/RepoGrammar/blob/main/docs/limitations.md">Limitations</a>
  ·
  <a href="https://github.com/SioYooo/RepoGrammar/blob/main/CHANGELOG.md">Changelog</a>
</p>

---

Coding agents repeatedly read the same files to rediscover how a repository
implements routes, fixtures, models, and data access. RepoGrammar turns those
repeated implementations into a compact map of **pattern families** before an
agent reads source broadly.

When repository evidence is strong, the agent gets representative examples,
source-backed metadata, and a hash-checked read plan. When it is not,
RepoGrammar returns a typed `UNKNOWN` with a recovery action instead of filling
the gap with a plausible guess.

> **The promise:** less source rereading without pretending uncertain static
> evidence is fact.

```text
repository source
      │
      ▼
local evidence index ──► compatible pattern families ──► bounded read plan
      │                              │
      └── freshness checks           └──► UNKNOWN / PARTIAL_CONTEXT + recovery
```

## Why RepoGrammar

| Pattern-aware | Evidence-gated | Agent-ready |
| --- | --- | --- |
| Finds how this repository repeatedly implements a role, not merely where a string appears. | Keeps provenance, freshness, unresolved semantics, and exceptions attached to every claim. | Serves compact context through a pattern-first CLI and one read-only MCP tool, `repogrammar_context`. |

RepoGrammar complements text search, semantic search, and symbol graphs. Those
tools locate code; RepoGrammar adds a repository-local contract for deciding
which repeated implementations are compatible, what still needs to be read,
and when the answer must abstain.

## Quick start

### 1. Download and install

RepoGrammar provides prebuilt binaries for supported macOS and Linux systems;
Rust and Cargo are not required.

```bash
curl -fsSL https://github.com/SioYooo/RepoGrammar/releases/latest/download/install.sh -o install.sh
bash install.sh --install-cli-only --yes
repogrammar version
```

Already have Node? The same release is published to npm, so you can download and
run it in one step — no separate install:

```bash
npx --yes --package @sioyooo/repogrammar repogrammar version
```

The npm package is a thin launcher: it downloads and verifies the matching
prebuilt macOS/Linux binary (the same release artifact `install.sh` installs),
so Rust and Cargo are still not required. See the
[full quickstart](https://github.com/SioYooo/RepoGrammar/blob/main/docs/quickstart.md)
for version-pinned `npx` commands and CI usage.

### 2. Set up your coding agent and first repository

Run this once inside your first repository. It configures a detected Codex or
Claude Code integration, initializes that repository, and starts its autosync
daemon.

```bash
cd /path/to/your/repo
repogrammar setup --target auto
```

### 3. Initialize every other repository

Each repository needs its own local index. Run `init` once inside every other
repository where you want RepoGrammar available:

```bash
cd /path/to/another/repo
repogrammar init
```

After `setup` or `init`, RepoGrammar automatically keeps that repository's
index synchronized as code changes. There is no global repository scanner, so
new repositories must be initialized once. See the
[full quickstart](https://github.com/SioYooo/RepoGrammar/blob/main/docs/quickstart.md)
for advanced installation, CI, manual sync, and cleanup options.

On receipt-aware current source, `repogrammar uninstall --dry-run` previews a
full managed-machine removal and `repogrammar uninstall --yes` authorizes it.
Use `repogrammar disconnect --target all --yes` when you only want to remove
RepoGrammar-owned coding-agent integrations. Repository indexes are deliberately
separate; remove one with `repogrammar uninit --project /path/to/repo --yes`.

The immutable public `v0.4.1` release includes the `disconnect` rename and the
receipted full self-uninstall contract. Follow the help shipped with the
installed binary for the exact lifecycle commands supported by that version.

## What you get

- **Pattern families** — repeated, compatible implementations with support,
  variation, exception, and counterexample context.
- **Metadata-first evidence** — repo-relative paths, content hashes, bounded
  ranges, provenance, and unresolved obligations; source spans are opt-in.
- **Prioritized read plans** — the smallest source-backed set the agent should
  inspect before making a change.
- **Static alignment** — a conservative check of whether a target matches an
  evidenced family without upgrading static similarity into runtime proof.
- **Typed recovery** — stale, ambiguous, dynamic, unsupported, and
  insufficient cases become `UNKNOWN` or `PARTIAL_CONTEXT` with an explicit
  next action.

## Latest in `0.4.1`

| Area | Current behavior |
| --- | --- |
| Onboarding | One `setup` flow composes safe agent integration, repository initialization, indexing, MCP self-test, and repo-local autosync. |
| Queries | Exact-first resolution also understands qualified concept phrases such as `FastAPI route`; `mode` controls evidence gathering and `verbosity` controls payload density. |
| Conformance | `check` returns static-alignment certificates with explicit unresolved obligations and never claims runtime equivalence. |
| Freshness | Query-time hashes reject stale evidence; explicit `sync` is authoritative, while default repo-local autosync is a best-effort convenience. |
| Efficiency | Dependency-aware incremental sync and Python interface hashes avoid unnecessary rebuild work while full/incremental equivalence gates protect results. |
| Metrics | Source-free query-outcome accounting reports estimated potential read displacement across outcomes; it is not measured token savings or a causal result. |

See the [changelog](https://github.com/SioYooo/RepoGrammar/blob/main/CHANGELOG.md)
for the complete version history and the
[CLI specification](https://github.com/SioYooo/RepoGrammar/blob/main/docs/specifications/cli.md)
for the exact command contract.

## How it stays trustworthy

1. **Discover locally.** Language adapters and bounded semantic workers extract
   structural facts without executing target-repository application code.
2. **Qualify conservatively.** Tree-sitter proposes candidates; syntax
   similarity alone cannot prove family membership.
3. **Return metadata first.** Results preserve hashes, bounded locations,
   provenance, evidence strength, and remaining read obligations.
4. **Enforce freshness.** Each repository owns its `.repogrammar/` SQLite
   generations and optional daemon; there is no global repository scanner.
5. **Abstain by type.** Unsupported confidence becomes an actionable typed
   result, not a hidden fallback.

The Rust implementation follows a dependency-inverted
`core → ports → application → adapters → interfaces` architecture. Read the
[architecture overview](https://github.com/SioYooo/RepoGrammar/blob/main/docs/architecture/overview.md)
and [MCP contract](https://github.com/SioYooo/RepoGrammar/blob/main/docs/specifications/mcp-api.md)
for the deeper design.

## Language and framework boundary

| Language | Current evidence boundary |
| --- | --- |
| **Python** — FastAPI, pytest, Pydantic, SQLAlchemy | Official Python-first scope with bounded framework-family context |
| **TypeScript / JavaScript** — Express, Jest/Vitest, Mocha/`node:test`, Next.js, Fastify, Prisma, Drizzle, Zod, NestJS, Hono | Conservative exact-anchor preview; React and React Native remain unsupported |
| **Rust** — internal patterns plus bounded serde, thiserror, Tokio, clap, and axum anchors | Structural preview; no macro expansion, trait-resolution, or runtime claim |
| **Java/Spring, C#, C/C++** | Conservative structural preview; no runtime or build-system equivalence claim |
| **Go, PHP, Ruby, Swift** | File discovery only; not analyzed or supported |

RepoGrammar is pre-1.0. Its MCP API and preview analyzers remain experimental,
and it is not a sound whole-program static analyzer or a runtime-equivalence
oracle. Stable artifacts target macOS arm64/x86_64 and glibc Linux
arm64/x86_64 at the documented minimum versions; Windows and musl are not
public release targets.

RepoGrammar itself does not call an LLM, embeddings API, vector database, or
cloud model. Python 3.10 or newer is required for the bounded Python analyzer;
Rust/Cargo is not required for the verified release path. The exact safety and
platform boundaries live in [limitations](https://github.com/SioYooo/RepoGrammar/blob/main/docs/limitations.md).

## Built with AI, directed by a human

RepoGrammar was developed through a human-directed GPT-5.6 workflow. ChatGPT
helped plan and review the work; Codex implemented and tested scoped changes;
the human maintainer owns the product insight, architecture, evidence policy,
scope, review, merge authority, and public approvals.

OpenAI Build Week is a launch milestone, not the product boundary. Competition
recording and submission material stays in the
[demo runbook](https://github.com/SioYooo/RepoGrammar/blob/main/docs/demo/build-week-demo.md)
and [launch kit](https://github.com/SioYooo/RepoGrammar/blob/main/docs/promotion/launch-kit.md),
leaving this README focused on the developer tool.

## Community

- Start with the [general quickstart](https://github.com/SioYooo/RepoGrammar/blob/main/docs/quickstart.md),
  [Codex guide](https://github.com/SioYooo/RepoGrammar/blob/main/docs/quickstart-codex.md),
  or [Claude Code guide](https://github.com/SioYooo/RepoGrammar/blob/main/docs/quickstart-claude.md).
- Browse the [documentation map](https://github.com/SioYooo/RepoGrammar/blob/main/docs/README.md)
  and [known limitations](https://github.com/SioYooo/RepoGrammar/blob/main/docs/limitations.md).
- Report bugs or propose improvements with the repository's
  [issue templates](https://github.com/SioYooo/RepoGrammar/issues/new/choose).
- Review [CONTRIBUTING](https://github.com/SioYooo/RepoGrammar/blob/main/CONTRIBUTING.md),
  [SECURITY](https://github.com/SioYooo/RepoGrammar/blob/main/SECURITY.md), and the
  [Code of Conduct](https://github.com/SioYooo/RepoGrammar/blob/main/CODE_OF_CONDUCT.md)
  before contributing.

RepoGrammar is licensed under the [MIT License](https://github.com/SioYooo/RepoGrammar/blob/main/LICENSE).
