# UNKNOWN Governance

- Status: Active
- Last updated: 2026-06-25
- Scope: Durable reminders for uncertainty handling.
- Evidence: `docs/specifications/unknowns.md`
- Related canonical docs: `docs/specifications/domain-model.md`, `docs/specifications/semantic-workers.md`
- Supersedes: None
- Superseded by: None

## Durable knowledge

- `UNKNOWN` is a first-class typed result, not a temporary error string.
- Structural evidence can generate candidates but cannot prove semantic family
  membership.
- Compiler-native semantic facts outrank structural and framework-heuristic
  facts.
- Conflicting, stale, unavailable, unsupported, low-confidence, or dynamic facts
  should become `UNKNOWN` or abstention for affected claims.
- Syntax-only code units are structural candidates only; they are not semantic
  facts, framework-equivalence claims, or family evidence.
- Tests for new analyzers should include uncertain, conflicting, stale,
  unsupported, and dynamic cases.

## Revalidation conditions

Update after classification, compatibility, freshness, semantic-worker, or
family-mining behavior changes.
