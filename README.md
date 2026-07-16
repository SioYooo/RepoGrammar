# RepoGrammar

**Metadata-first context reduction for coding agents.**

Coding agents waste tokens re-discovering conventions a repository already
contains — reading dozens of files just to learn *"how does this repo write a
FastAPI route / a pytest fixture / a SQLAlchemy repository?"* RepoGrammar gives
the agent a compact, source-backed map of those implementation patterns **before**
it edits code — and, crucially, returns a typed `UNKNOWN` instead of confidently
guessing when evidence is weak.

---

## Objective

RepoGrammar was built to answer one question: *can we make coding agents spend*

*fewer tokens and make fewer confident-but-wrong guesses about a codebase?*

- **The problem** — agents re-read broad swaths of a repo to relearn its
  conventions, and RAG/LLM approaches happily hallucinate.
- **Our take** — a small, auditable "repo grammar" of implementation families,
  delivered before the agent reads source, that **abstains honestly** when unsure.
- **Design values** — metadata by default, source opt-in, never execute
  target-repo code, and never overclaim.

---

## What it does

Before an agent reads source broadly, RepoGrammar returns a small, auditable map:

- **`family`** — an implementation family with enough repeated, compatible support.
- **`variation` / `exception`** — accepted differences and intentionally
  unsupported cases.
- **`evidence`** — repo-relative paths, hashes, byte ranges, support counts
  (no source snippets by default).
- **`read_plan`** — hash-checked spans to read before editing.
- **`UNKNOWN`** — a *typed abstention* for stale, ambiguous, dynamic, or
  out-of-scope cases.

It ships as a **pattern-family-first CLI** and a **read-only MCP tool
(`repogrammar_context`)** that wires into Claude Code and Codex.

## How it's built

A **Rust core** with a layered, dependency-inverted architecture
(`core → ports → application → adapters → interfaces`). Key choices:

- **Tree-sitter as candidate generation, not a semantic oracle** — a family
  needs several compatible exact-anchor facts before it is claimed.
- **Never execute target-repo code** — the Python worker parses `setup.py` but
  never runs it; the TS worker won't load the repo's own `typescript` by default.
- **Local-first** — the index is a SQLite database under the project's
  `.repogrammar/`, with explicit `init` / `sync` / `autosync`.

## Quick start

```bash
git clone https://github.com/SioYooo/RepoGrammar.git
cd RepoGrammar
cargo build --release
bash src/install/repogrammar-install.sh --install-cli-only --from-source --yes
repogrammar version

# then, inside a repo you want to analyze:
repogrammar init
repogrammar find --project . --token-budget 8000 <path-or-symbol>
repogrammar check --project . --token-budget 8000 <path-or-symbol>
```

Wire it into a coding agent (Codex / Claude Code):

```bash
bash src/install/repogrammar-install.sh --install-and-configure --from-source --yes --target all
```

## 🤖 Built with AI (GPT-5.6)

RepoGrammar was developed with an AI-assisted, human-directed workflow. Both
roles ran on GPT-5.6:

- **Planning, review, and direction — ChatGPT (GPT-5.6).** It shaped the
  maintainer's ideas into an overall plan, reviewed the code produced at each
  step, and set the direction for the next piece of work — the architectural
  brain that decided *what* to build next and checked that each step held up.
- **Coding — Codex (GPT-5.6).** It did the bulk of the actual coding: turning
  each planned step into Rust modules, language adapters, and tests, then
  iterating against `cargo fmt`, `clippy -D warnings`, `cargo test`, and the
  repo's `repo-guard` checks.

Guardrails kept the loop honest: the repo encodes a strict agent contract
(`AGENTS.md` / `CLAUDE.md`) — smallest coherent change, tests + docs in the same
commit, no target-repo code execution, and typed `UNKNOWN` instead of
overclaiming. The human maintainer owns every architecture decision, scope
boundary, and merge; commits carry only the maintainer's identity, never
AI/agent attribution.

## What it can do

| Language                                                                   | What RepoGrammar can do                               |
| -------------------------------------------------------------------------- | ----------------------------------------------------- |
| **Python** — FastAPI, pytest, Pydantic, SQLAlchemy                  | Bounded framework-family context (not full semantics) |
| **TS/JS** — Express, Jest/Vitest, Next.js, Fastify, Prisma, Drizzle | Exact-anchor family context                           |
| **Java/Spring, C#, C/C++**                                           | Structural family context                             |
| **Rust**                                                             | Self-dogfood plus general framework context           |
| **Go, PHP, Ruby, Swift**                                             | File discovery only (not yet analyzed)                |

RepoGrammar is pre-alpha. It is not a sound static analyzer, does not replace
direct source inspection, and does not claim measured token savings by default.

**Platforms:** macOS and Linux are supported. Windows is not fully supported yet
— file-locking behavior on Windows currently makes the local index lifecycle
unreliable — so it is temporarily out of scope.

## License

MIT
