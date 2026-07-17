# RepoGrammar Launch Kit

This is the reusable public copy for RepoGrammar's developer launch. The root
README remains the product homepage; this file supplies synchronized text for
GitHub metadata, Build Week/Devpost, video descriptions, and community posts.
Do not add a publication, platform, performance, or savings claim here until
the linked evidence exists.

## Product identity

**Name:** RepoGrammar

**One-line value proposition:**

> Local-first, source-backed repository context for coding agents—with a typed
> `UNKNOWN` when the codebase has not proved the answer.

**GitHub description:**

> Local-first repository context for coding agents: source-backed patterns,
> bounded read plans, and typed UNKNOWN.

**Suggested GitHub topics:**

```text
coding-agents
mcp
rust
static-analysis
developer-tools
code-analysis
local-first
```

**Primary audience:** developers using Codex, Claude Code, or another MCP-aware
coding agent in established repositories with repeated implementation patterns.

## Long product description

Coding agents repeatedly read broad parts of a repository to rediscover local
conventions: how routes, tests, models, fixtures, and data access are actually
implemented here. RepoGrammar indexes those patterns locally and returns a
small, source-backed context bundle before an agent reads source broadly.

Its pattern-family-first CLI and read-only `repogrammar_context` MCP tool can
return:

- a supported implementation family and compatible members;
- source-free evidence metadata with repo-relative paths and hashes;
- variation, exception, freshness, and uncertainty information;
- a bounded read plan identifying source that still must be inspected; and
- typed `UNKNOWN` or `PARTIAL_CONTEXT` when evidence is stale, ambiguous,
  dynamic, unsupported, or insufficient.

RepoGrammar is local-first. The index lives under `.repogrammar/`. The product
does not execute target-repository code, does not call an LLM or cloud API, and
does not require an OpenAI API key. Source snippets are opt-in and remain
bounded by hash and freshness checks.

The project is pre-1.0 and its MCP API and bounded analyzers remain
experimental. Python FastAPI, pytest, Pydantic, and SQLAlchemy are the primary
bounded family path; TypeScript/JavaScript, Java/Spring, C#, C/C++, and
internal Rust dogfood have narrower preview boundaries. Go, PHP, Ruby, and
Swift are discovered-only and are not analysis-support claims.

## Short launch copy

> RepoGrammar helps coding agents stop rediscovering the same repository
> conventions. It builds a local, source-backed map of implementation families
> and returns metadata plus a bounded read plan before broad source reads. When
> evidence is weak, stale, dynamic, or ambiguous, it returns a typed `UNKNOWN`
> instead of guessing. The Rust CLI and read-only MCP server are designed for
> long-lived developer workflows, not only a hackathon demo.

## Build Week / Devpost project copy

### Inspiration and problem

Coding agents are powerful, but they often spend context rereading many files
just to learn a repository's conventions. A retrieval system can reduce that
search, yet a plausible-looking match is dangerous if it silently turns weak
structural similarity into a confident engineering claim.

RepoGrammar asks a narrower question: can a local developer tool provide
compact, auditable implementation-pattern context while preserving an honest
abstention path?

### What it does

RepoGrammar builds a repository-local index and groups compatible,
source-backed implementation facts into pattern families. A developer or
coding agent can start from the path, symbol, member, framework role, or
pattern question they already have. RepoGrammar discovers bounded candidates
and returns family context, provenance metadata, variation/exception coverage,
unknowns, and a hash-checked read plan.

`repogrammar check` deliberately returns `CONTEXT_ONLY` and keeps runtime
equivalence advisory `UNKNOWN` when it is not proved. Unresolvable targets
return a typed reason such as `InsufficientSupport` and recommend ordinary
source fallback.

### How it is built

The product has a Rust core with dependency-inverted boundaries, SQLite
repository-local storage, conservative language/framework adapters, and
bounded Python and TypeScript semantic-worker processes. Tree-sitter generates
candidates but is not accepted as the sole semantic oracle. The default MCP
surface is one read-only tool, `repogrammar_context`, with explicit operations
for finding analogues, showing a family, explaining deviations, and checking
conformance.

Setup composes safe agent integration, repository initialization and indexing,
auto-sync, and a product MCP self-test while preserving foreign or malformed
agent configuration. Telemetry is off by default and setup does not enable it.

### How GPT-5.6 and Codex were used

The project used an AI-assisted, human-directed workflow. ChatGPT on GPT-5.6
helped turn the maintainer's product ideas into execution plans and reviewed
completed slices. Codex on GPT-5.6 implemented much of the Rust code, adapters,
tests, release tooling, and documentation, iterating against formatting,
clippy, test, and repository-policy gates.

The human maintainer retained architecture, scope, evidence policy, review,
and merge authority. The repository's mirrored agent contract requires scoped
changes, synchronized tests and docs, no execution of target-repository code,
and typed abstention instead of overclaiming. Commits contain only the
maintainer's identity.

RepoGrammar itself does not run GPT-5.6 and does not call the OpenAI API. GPT-5.6
was the development and demo reasoning surface; RepoGrammar is the local MCP
developer tool supplying repository evidence.

### What was learned

Useful repository context is not the same as proof. The product became more
trustworthy by separating family context, runtime equivalence, source-reading
requirements, freshness, and agent-integration readiness. `UNKNOWN` is part of
the user experience: it tells the agent exactly where normal source inspection
must resume.

### Long-term direction

Build Week is RepoGrammar's first public product story, not its endpoint. The
long-term goal is a dependable local context layer for developers and coding
agents: simpler installation, reproducible real-repository dogfood, clearer
evidence, and conservative expansion only when each language/framework path
passes its qualification gates.

## Evidence and media links

- Repository: <https://github.com/SioYooo/RepoGrammar>
- README demo visual:
  <https://github.com/SioYooo/RepoGrammar/blob/main/docs/assets/repogrammar-demo.svg>
- Verified CLI transcript:
  <https://github.com/SioYooo/RepoGrammar/blob/main/docs/demo/verified-cli-transcript.md>
- Recording runbook:
  <https://github.com/SioYooo/RepoGrammar/blob/main/docs/demo/build-week-demo.md>
- Limitations:
  <https://github.com/SioYooo/RepoGrammar/blob/main/docs/limitations.md>
- Stable-channel `0.2.2` GitHub release:
  <https://github.com/SioYooo/RepoGrammar/releases/tag/v0.2.2>
- Stable-channel `0.2.2` npm package:
  <https://www.npmjs.com/package/@sioyooo/repogrammar/v/0.2.2>
- Final public verification: Actions run
  [`29591027524`](https://github.com/SioYooo/RepoGrammar/actions/runs/29591027524)
  emitted `STABLE_RELEASE_READY`
- Public YouTube demo: **pending recording, audio, upload, and signed-out access
  verification**
- Codex `/feedback` Session ID: **pending verified submission receipt**

## Submission fields to verify manually

- [ ] Repository link points at the public repository and the submitted commit
      is reachable.
- [x] A no-build evaluator path is live through the pinned npm package and
      immutable GitHub release.
- [ ] The public video is under three minutes, contains spoken audio, and works
      while signed out.
- [ ] The project description explains the GPT-5.6 planning/review and Codex
      implementation roles without claiming RepoGrammar calls an OpenAI model.
- [ ] The accepted `/feedback` Session ID is copied from the submission receipt,
      not guessed from a local task identifier.
- [ ] Release, npm, platform, and support statements match the verified state on
      submission day.
- [ ] Devpost text is refreshed from this file after the final product change;
      Devpost must not become a competing product specification.

## Claim guardrails

Use these phrases:

- “source-backed implementation-family context”
- “metadata-first, with source spans opt-in”
- “estimated potential read displacement”
- “typed `UNKNOWN` with source fallback”
- “local product MCP self-test passed” only when that exact fact was verified

Do not use these phrases without new evidence:

- “measured token savings”
- “reduces tokens by X%”
- “prevents hallucinations”
- “proves conformance” or “proves runtime equivalence”
- “supports Windows”
- “published on npm” or “available from GitHub Releases”
- “all coding agents are configured” when only the product binary self-test
  passed

The current CLI may emit `estimated_potential_token_savings`, but that field
ships with `measurement_kind: ESTIMATED` and a caveat stating that it is not
measured token savings. A measured claim requires the paired, comparable,
correctness-gated study defined by the repository dogfood protocol.

## Current external blockers

Repository copy and the audited static terminal visual are ready. The following
steps cannot be completed by a documentation commit:

1. record the live 90–100 second demo with spoken audio;
2. edit, caption, and upload the video to YouTube;
3. verify the video from a signed-out session; and
4. submit Devpost fields and capture the accepted Codex `/feedback` Session ID.

Until those steps are evidenced, keep every placeholder and pending label in
this file.
