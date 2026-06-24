# Product Specification

RepoGrammar is a local tool for helping coding agents understand recurring
implementation patterns inside a repository.

## Product goal

RepoGrammar should return pattern-family evidence rather than only call graphs
or similarity search results. A result should be able to describe:

- common implementation skeletons;
- high-support repository conventions;
- legitimate variation slots;
- exceptions and counterexamples;
- closest matching implementations;
- contrastive examples that cover key differences;
- source evidence for every conclusion;
- `UNKNOWN` when static analysis cannot support a claim.

## Intended users

- Local coding agents preparing implementation changes.
- Maintainers reviewing whether a proposed change matches repository norms.
- Developers seeking representative examples inside a large codebase.

## MVP scope

The first implementation phase targets local TypeScript and JavaScript analysis
through a Tree-sitter syntax layer plus a future TypeScript semantic worker.
Initial framework adapters are planned for Express, NestJS, React, Jest, and
Vitest.

RepoGrammar v0.1 officially supports TypeScript and JavaScript only. Python is
planned as the second official language and may appear earlier only as an
experimental adapter. Experimental Python functionality must not be documented
as production support.

Python v0.2 should prioritize FastAPI, pytest, SQLAlchemy, and Pydantic. Django
is deferred until after the focused FastAPI/pytest subset validates the
language-adapter abstraction.

## Non-goals

- No cloud service dependency.
- No local LLM, embedding model, vector database, or remote API.
- No automatic modification of user business code from pattern-family results.
- No production-readiness or token-savings claims until measured evidence
  exists.

## Result discipline

RepoGrammar must distinguish `DOMINANT_PATTERN`, `VARIATION`, `EXCEPTION`, and
`UNKNOWN`. Low confidence, competing families, incompatible targets, and dynamic
runtime behavior must lead to abstention rather than certainty.

Structural similarity may generate candidates, but it must not by itself prove
semantic family membership. Compiler-native semantic facts take precedence over
framework heuristics and syntax-only fingerprints.
