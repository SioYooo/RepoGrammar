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
- Syntax-origin framework-role facts are semantic fact records, but their
  `FRAMEWORK_HEURISTIC` certainty remains insufficient support for family-claim
  input.
- The Rust domain now has stable typed UNKNOWN class/reason tokens, and the
  query application layer uses them for internal semantic-fact claim-input
  readiness. Stale or missing source blocks with `StaleEvidence`, weak certainty
  and `UNKNOWN` fact kind block with `InsufficientSupport`, and conflicting
  certainty blocks with `ConflictingFacts`.
- A fresh supported semantic fact kind is only eligible input for future claim
  builders. It is not a pattern-family classification or conformance result.
- Tests for new analyzers should include uncertain, conflicting, stale,
  unsupported, and dynamic cases.
- Python dynamic Pydantic model factories such as `pydantic.create_model(...)`
  remain typed `FrameworkMagic` `UNKNOWN` for framework identity unless a later
  provider-backed design proves a narrower claim.
- Python provider agreement, provider disagreement, and runtime observation are
  not current certainty tokens. Until the Rust domain, protocol, storage, CLI,
  MCP, and schemas change together, cross-check and observed-runtime details
  remain provenance/assumptions and conflicts still surface as
  `ConflictingFacts` or typed `UNKNOWN`.

## Revalidation conditions

Update after classification, compatibility, freshness, semantic-worker,
UNKNOWN-token, or family-mining behavior changes.
