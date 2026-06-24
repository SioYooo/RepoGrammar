# ADR-0009: Experimental Python dogfooding

- Status: Accepted
- Date: 2026-06-25

## Context

ADR-0005 makes TypeScript and JavaScript the only official v0.1 language scope.
Python is planned as the second official language, but production support
requires a focused v0.2 adapter decision.

RepoGrammar needs early feedback on whether the language-adapter,
semantic-worker, provenance, provider, and `UNKNOWN` boundaries work for Python.
That feedback is useful only if it does not become a user-facing support claim.

## Decision

Allow Python work before v0.2 only as explicitly experimental dogfooding.

Experimental Python work may include fixture corpora, parser/discovery
prototypes, semantic-worker protocol experiments, and local evaluation against
FastAPI, pytest, SQLAlchemy, and Pydantic examples.

Experimental Python must be opt-in and must not be part of default v0.1
production language support. Default user-facing documentation must continue to
describe Python as planned or experimental, not supported.

Any Python implementation in this repository must obey the repository source
boundary under `src/` and route through language adapters, semantic-worker
protocols, and RepoGrammar-owned facts before entering the Rust core.

## Alternatives considered

- Wait until v0.2 before any Python code: lower support risk, but delays adapter
  boundary feedback.
- Promote Python into v0.1 support: rejected because it contradicts ADR-0005
  and would dilute the TS/JS evidence target.
- Treat syntax-only Python extraction as support: rejected because structural
  candidates cannot prove semantic family membership.

## Consequences

Python findings before v0.2 are diagnostic only. Syntax-only or partial Python
facts must not produce strong semantic or family claims. Dynamic imports,
decorators, monkey patching, pytest fixture injection, framework configuration,
runtime dependency injection, and conflicting analyzer output should produce
typed `UNKNOWN` or abstention.

Default `index`, `sync`, MCP behavior, README claims, and release notes must not
claim Python production support until a superseding ADR accepts it.

Optional Python tooling must not become a required dependency for default Cargo
checks, repository guard, or v0.1 CI unless a later decision explicitly changes
that policy.

## Follow-up work

Define the opt-in configuration shape, add experimental fixtures under `src/`,
record Python provenance and support level in manifests, and write promotion
criteria for a future v0.2 Python adapter ADR.
