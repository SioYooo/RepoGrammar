---
name: repogrammar-domain
description: Use for core model, pattern-family, evidence, provenance, compatibility, freshness, abstention, or mining-pipeline changes.
---

# Purpose

Preserve RepoGrammar's conservative pattern-family methodology.

# Trigger conditions

Use when editing `src/rust/core/`, parser-to-IR conversion, semantic facts,
family classification, evidence handling, provenance, freshness, compatibility,
or mining algorithms.

# Required reading

- `docs/specifications/product.md`
- `docs/specifications/domain-model.md`
- `docs/specifications/indexing-pipeline.md`
- `docs/decisions/ADR-0003-pattern-family-model.md`

# Preconditions

- The change must not turn heuristics into certainty.
- The change must preserve source evidence and `UNKNOWN` outcomes.
- Tree-sitter facts are structural and candidate-generating, not final semantic
  proof.

# Step-by-step procedure

1. Identify the domain claim being changed.
2. Define evidence required for the claim.
3. Preserve `DOMINANT_PATTERN`, `VARIATION`, `EXCEPTION`, and `UNKNOWN`.
4. Distinguish structural, semantic, framework-heuristic, conflicting, and
   unknown facts.
5. Add compatibility and freshness considerations where relevant.
6. Add tests for uncertain, low-confidence, or competing-family behavior.
7. Update the domain specification.

# Required verification

Run the Rust quality gates and repository guard.

# Documentation updates

Update domain model, indexing pipeline, product spec, roadmap, or ADRs when the
methodology changes.

# Commit requirements

Keep domain type changes, tests, and spec updates atomic.

# Completion report

Report any remaining `UNKNOWN` cases and unsupported assumptions.

# Failure and rollback handling

If evidence is insufficient, abstain. Do not claim dominance from one example or
from weak similarity. Do not use Tree-sitter-only evidence to prove semantic
family membership.
