# RepoGrammar

**Local-first, source-backed repository context for coding agents.**

Coding agents repeatedly read the same files to rediscover how a repository
implements routes, fixtures, models, and data access. RepoGrammar builds a
compact map of those implementation-pattern families before an agent reads
source broadly. It returns metadata and a hash-checked read plan when evidence
is strong, and a typed `UNKNOWN` instead of guessing when it is not.

[![RepoGrammar terminal demo: setup, find, check, and typed UNKNOWN](https://raw.githubusercontent.com/SioYooo/RepoGrammar/main/docs/assets/repogrammar-demo.svg)](https://github.com/SioYooo/RepoGrammar/blob/main/docs/demo/verified-cli-transcript.md)

This is an audited excerpt from a real `0.2.0-preview.0` CLI run against
committed Python fixtures. Paths were normalized for display; the
[commands and transcript](https://github.com/SioYooo/RepoGrammar/blob/main/docs/demo/verified-cli-transcript.md) are
reproducible from this checkout.

## Install

### Public-preview path — verify the exact version

Availability is decided by the exact `0.2.0-preview.0` publication, not by the
contents of this README. Verify both registries before using the no-build path:

```bash
npm view @sioyooo/repogrammar@0.2.0-preview.0 version
curl -fsSI https://github.com/SioYooo/RepoGrammar/releases/download/v0.2.0-preview.0/install.sh.sha256
```

Only when both commands succeed, run the pinned preview:

```bash
npx @sioyooo/repogrammar@0.2.0-preview.0 setup --project /path/to/your/repo --target auto
```

If either check fails, use the contributor/dogfood path, which builds once from
source. The complete publication gate is in the
[release checklist](https://github.com/SioYooo/RepoGrammar/blob/main/docs/release/public-preview-release-checklist.md).

```bash
git clone https://github.com/SioYooo/RepoGrammar.git
cd RepoGrammar
cargo build --release
bash src/install/repogrammar-install.sh --install-cli-only --from-source --yes
repogrammar version
```

The installed command needs Python 3.10 or newer (`python3`) for the current Python preview. It does
not need Node.js, npm, Docker, a local model, an OpenAI API key, or a cloud API.

## From setup to trustworthy context

Run setup inside the repository you want to analyze. It reviews one plan,
initializes and indexes the repository, wires a detected Codex or Claude Code
integration when ownership is safe, starts auto-sync by default, and runs a
read-only product MCP self-test:

```bash
cd /path/to/your/repo
repogrammar setup --target auto

# Ask for a source-backed family and a bounded read plan.
repogrammar find --project . --token-budget 8000 app/routes.py

# Conformance remains advisory when runtime equivalence is unproven.
repogrammar check --project . --token-budget 8000 app/routes.py
```

The captured demo also asks for a target that static evidence cannot resolve:

```bash
repogrammar find --project . --token-budget 8000 registered_router
```

It returns `UNKNOWN`, identifies `InsufficientSupport`, and recommends source
fallback. That is a successful safety decision, not a failed query. Use the
[fixture-backed walkthrough](https://github.com/SioYooo/RepoGrammar/blob/main/docs/demo/verified-cli-transcript.md) to reproduce
the exact `find → check → UNKNOWN` path.

## How it works

RepoGrammar ships a pattern-family-first CLI and one read-only MCP tool,
`repogrammar_context`.

1. **Discover candidates locally.** Language adapters and bounded semantic
   workers extract source-backed structural facts without executing the target
   repository.
2. **Require compatible support.** Tree-sitter proposes candidates; it is not
   treated as a semantic oracle. Family claims require compatible exact-anchor
   evidence.
3. **Return metadata first.** Results include repo-relative evidence, hashes,
   byte and line ranges, variation/exception coverage, and a minimal read plan.
   Source snippets are opt-in.
4. **Abstain by type.** Stale, ambiguous, dynamic, unsupported, or insufficient
   evidence becomes `UNKNOWN` or `PARTIAL_CONTEXT`, with a recovery action.
5. **Stay local and fresh.** The active SQLite index lives under
   `.repogrammar/`; explicit sync and auto-sync keep repository evidence
   current.

The Rust implementation follows a dependency-inverted
`core → ports → application → adapters → interfaces` architecture. See the
[architecture overview](https://github.com/SioYooo/RepoGrammar/blob/main/docs/architecture/overview.md) and
[MCP contract](https://github.com/SioYooo/RepoGrammar/blob/main/docs/specifications/mcp-api.md).

## Support and limitations

| Language                                                                                     | Current evidence boundary                                      |
| -------------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| **Python** — FastAPI, pytest, Pydantic, SQLAlchemy                                    | Bounded framework-family context, not full Python semantics    |
| **TypeScript / JavaScript** — Express, Jest/Vitest, Next.js, Fastify, Prisma, Drizzle | Conservative exact-anchor preview                              |
| **Java/Spring, C#, C/C++**                                                             | Structural preview; no runtime/build-system equivalence claim  |
| **Rust**                                                                               | Internal self-dogfood; no general Rust semantic-analysis claim |
| **Go, PHP, Ruby, Swift**                                                               | File discovery only; not analyzed or supported yet             |

RepoGrammar is pre-alpha. It is not a sound static analyzer and does not replace
source inspection. `estimated_potential_token_savings` is an **estimated** local
read-displacement diagnostic—not measured savings or a causal claim. Measured
savings require a controlled before/after study; the current
[limitations](https://github.com/SioYooo/RepoGrammar/blob/main/docs/limitations.md) keep that boundary explicit.

macOS and Linux are the current supported platforms. Windows is not fully
supported because its local index lifecycle still needs platform proof; no
Windows release support is claimed.

## Codex and GPT 5.6 Usage

RepoGrammar asks whether coding agents can read less repository source without
becoming more confident than the evidence permits. [OpenAI Build Week](https://openai.devpost.com/)
is a launch milestone, not the product boundary: RepoGrammar is being built as
an ongoing local developer tool for coding-agent workflows.

The implementation used a human-directed GPT-5.6 workflow:

- **ChatGPT (GPT-5.6)** helped turn the maintainer's product direction into
  plans and reviewed each completed slice.
- **Codex (GPT-5.6)** implemented and tested Rust modules, language adapters,
  CLI behavior, release tooling, and documentation against repository gates.
- **The human maintainer** owns architecture, scope, evidence policy, review,
  and every merge. Commits use only the maintainer's identity.

Repository guardrails keep that collaboration auditable: the mirrored
`AGENTS.md` / `CLAUDE.md` contract requires scoped changes, tests and docs in
the same commit, no target-repository code execution, and typed `UNKNOWN`
instead of unsupported claims. The reusable
[demo script](https://github.com/SioYooo/RepoGrammar/blob/main/docs/demo/build-week-demo.md) and
[launch kit](https://github.com/SioYooo/RepoGrammar/blob/main/docs/promotion/launch-kit.md) contain the Build Week submission
copy without turning the README into a competition-only landing page.

## License

RepoGrammar is licensed under the [MIT License](https://github.com/SioYooo/RepoGrammar/blob/main/LICENSE).
