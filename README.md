<h1 align="center">RepoGrammar</h1>

<p align="center">
  <strong>Give coding agents your repository's conventions—not another pile of search results.</strong>
</p>

<p align="center">
  Local-first, source-backed pattern context with bounded read plans and honest abstention.
</p>

<p align="center">
  <a href="https://github.com/SioYooo/RepoGrammar/releases/tag/v0.4.3"><img alt="Stable version 0.4.3" src="https://img.shields.io/badge/stable-0.4.3-7c3aed?style=flat-square"></a>
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

RepoGrammar supports macOS and glibc-based Linux. Windows and musl Linux are
not currently supported installation targets.

### Prerequisites

- Python 3.10 or later;
- Bash;
- `curl`, `tar`, and `gzip`; and
- network access to GitHub Releases during installation.

Verify the required tools before installing:

```bash
python3 --version
bash --version
curl --version
tar --version
gzip --version
```

The Python version must be 3.10 or later. Rust, Cargo, Node.js, Docker, an LLM,
and API keys are not required for the binary installation path.

### 1. Download and install the CLI

Use a temporary directory so the installer files do not remain in a project:

```bash
mkdir -p /tmp/repogrammar-install
cd /tmp/repogrammar-install

curl -fsSLO \
  https://github.com/SioYooo/RepoGrammar/releases/download/v0.4.3/install.sh
curl -fsSLO \
  https://github.com/SioYooo/RepoGrammar/releases/download/v0.4.3/install.sh.sha256
```

Verify the installer itself on macOS:

```bash
shasum -a 256 -c install.sh.sha256
```

On Linux, use `sha256sum -c install.sh.sha256` instead. Then install the CLI:

```bash
bash install.sh --version v0.4.3 --install-cli-only --yes

export PATH="$HOME/.local/bin:$PATH"
repogrammar version
```

The expected output is `repogrammar 0.4.3`. The installer downloads and
checksum-verifies the matching native archive and bundled Python worker,
installs the managed command under `$HOME/.local/bin`, and records the product
receipt. It does not configure a coding agent or create repository-local
`.repogrammar/` state.

To make the PATH change persistent, add this line once to `~/.zshrc` for Zsh or
`~/.bashrc` for Bash, then start a new shell:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### 2. Optionally connect a coding agent

Skip this step when only the CLI is needed. To detect an installed Codex or
Claude Code client and configure the read-only RepoGrammar MCP server globally:

```bash
repogrammar install --target auto --scope global --yes --no-telemetry
```

Restart an already-running coding-agent session after this command completes.
Agent installation does not initialize a repository and does not edit global
instruction files by default.

### 3. Initialize each repository

Every repository needs its own local RepoGrammar state and index:

```bash
cd /path/to/repository

repogrammar init --project "$PWD" --yes

repogrammar status \
  --project "$PWD"
```

`init` creates `.repogrammar/`, builds the active index, and starts that
repository's optional autosync daemon by default. Do not manually edit
`.repogrammar/`. For CI or a deterministic one-shot index, use:

```bash
repogrammar init \
  --project "$PWD" \
  --yes \
  --no-autosync \
  --progress never
```

Run `init` once in every additional repository. The CLI and coding-agent
integration are machine-level installations and do not need to be repeated.
There is no global repository scanner.

For machine-readable status and recovery guidance:

```bash
repogrammar status --project "$PWD" --json
repogrammar doctor --project "$PWD" --json
```

Follow the reported recovery action instead of manually modifying
`.repogrammar/`. If the shell reports `repogrammar: command not found`, run
`export PATH="$HOME/.local/bin:$PATH"` and verify that
`$HOME/.local/bin/repogrammar` exists.

Already have Node.js? The same immutable version is also available through the
thin npm launcher:

```bash
npx --yes --package @sioyooo/repogrammar@0.4.3 repogrammar version
```

See the [full quickstart](https://github.com/SioYooo/RepoGrammar/blob/main/docs/quickstart.md)
for advanced installation, CI, explicit instruction synchronization, manual
sync, and cleanup. Use `repogrammar uninstall --dry-run` to preview complete
managed-machine removal, `repogrammar disconnect --target all --yes` to remove
only agent integrations, and
`repogrammar uninit --project /path/to/repository --yes` to remove one
repository's local index.

## Five-minute judge testing path

After cloning this repository and completing the installation above, run the
following commands from the RepoGrammar repository root. This path exercises
the installed release binary, repository initialization, storage readiness,
family inventory, and one source-backed pattern lookup without requiring Rust
or Cargo:

```bash
repogrammar version

repogrammar init \
  --project "$PWD" \
  --yes \
  --no-autosync \
  --progress never

repogrammar status --project "$PWD"
repogrammar families --project "$PWD"

repogrammar find \
  "src/fixtures/python/release/v0_1/positive-strong-evidence/routes.py:7" \
  --project "$PWD" \
  --mode compact \
  --verbosity minimal
```

The expected evidence is:

- `repogrammar version` reports `0.4.3`;
- status reports an initialized repository, available storage, and an active
  generation;
- `families` reports ready implementation pattern groups; and
- `find` reports `pattern family found`, identifies
  `Python · FastAPI Route`, and returns a bounded source span to read. It still
  states that dynamic or runtime behavior remains unproven.

If coding-agent integration was enabled, verify the native MCP registration as
an additional machine-level check:

```bash
codex mcp get repogrammar --json
# or: claude mcp get repogrammar
```

For the full recorded judge journey—including a target-repository patch,
runtime test, stale-evidence rejection, explicit sync, and cleanup—follow the
[Build Week demo runbook](https://github.com/SioYooo/RepoGrammar/blob/main/docs/demo/build-week-demo.md).

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

## Latest in `0.4.3`

| Area | Current behavior |
| --- | --- |
| Onboarding | `install.sh` installs the binary, optional agent MCP wiring reuses the validated product-receipt command path even with Conda/venv PATH prefixes, and `init` owns each repository's index and autosync. |
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
