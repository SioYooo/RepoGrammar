# UNKNOWN Governance

- Status: Active
- Last updated: 2026-06-26
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
- Root `Cargo.toml` `rust_build_variant` UNKNOWNs may block repository-wide Rust
  self-dogfood families, but nested fixture/package manifests must not globally
  block unrelated root Rust family claims; keep those unknowns scoped to the
  affected package/crate/claim until provider-backed package scope exists.
- A fresh supported semantic fact kind is only eligible input for future claim
  builders. It is not a pattern-family classification or conformance result.
- Tests for new analyzers should include uncertain, conflicting, stale,
  unsupported, and dynamic cases.
- Python dynamic Pydantic model factories such as `pydantic.create_model(...)`
  remain typed `FrameworkMagic` `UNKNOWN` for framework identity unless a later
  provider-backed design proves a narrower claim.
- Python unresolved bare decorators remain typed `FrameworkMagic` `UNKNOWN` for
  framework identity. Locally defined decorators and native `property`,
  `classmethod`, and `staticmethod` remain structural metadata.
- Python exact-anchor support derivation is sound-by-abstention for bounded
  framework-family claims: claim-relevant parser-origin blocking `UNKNOWN`s
  such as `DynamicImport`/`UnresolvedImport` for `python_import_resolution`,
  `FrameworkMagic`/`MonkeyPatch` for call or framework identity, and
  `PytestFixtureInjection`/`ConflictingFacts` for `pytest_fixture_binding`
  prevent the affected unit from contributing family support. FastAPI
  `fastapi_dependency_target` UNKNOWNs remain scoped to that subclaim and do
  not block route-family support.
- Supported Python family members preserve non-blocking subclaim `UNKNOWN`s in
  family detail/query metadata with concrete affected claims such as
  `<family_id>:fastapi_dependency_target`; preserving the UNKNOWN does not make
  the subclaim true.
- The family builder repeats the same claim-scoped blocking check after support
  derivation. Parser-origin context facts can split complete-link Python
  clusters or create metadata-only variation slots, but parser-origin blocking
  `UNKNOWN`s remove the affected unit from confident family support unless the
  UNKNOWN is scoped to a non-membership subclaim. pytest non-builtin fixture
  context remains a compatibility constraint rather than a free variation.
- Python provider agreement, provider disagreement, and runtime observation are
  not current certainty tokens. Until the Rust domain, protocol, storage, CLI,
  MCP, and schemas change together, cross-check and observed-runtime details
  remain provenance/assumptions and conflicts still surface as
  `ConflictingFacts` or typed `UNKNOWN`.

## Revalidation conditions

Update after classification, compatibility, freshness, semantic-worker,
UNKNOWN-token, or family-mining behavior changes.
