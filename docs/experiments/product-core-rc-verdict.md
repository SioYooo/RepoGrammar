# Product Core Release-Candidate Verdict

- Date: 2026-07-19
- Branch: `feat/product-core`
- Verdict HEAD: the commit carrying this document (parent `30fd268`)
- Baseline: `33715e4` (published v0.2.2)
- Scope: 27 commits of product-core work, phases 0 through 8

## Verdict

**PRODUCT_CORE_RELEASE_CANDIDATE_READY**

All eight program phases completed their gates; every adversarial-review
finding that survived skeptic verification has been fixed and integrated or
recorded below as a documented, claim-safe deferral. No published v0.2.2
release artifact was modified. Nothing in this verdict authorizes a push,
merge, tag, or publication.

## Gate evidence (as re-run on the verdict HEAD)

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test --workspace --all-features` | 1444 + 95 + 113 + 1 pass, 0 fail |
| `repo-guard check` | passed |
| `repo-guard product-eval` (79-query corpus) | false_family 0/46, selected_on_abstention_gold 0, correct_abstention 25/25, context 12/12, hit@1 21/42, candidate_recall 13/14, mrr 0.500, unsupported_rejection 4/4, ambiguity_precision 6/6 |
| `repo-guard sync-equivalence --all` | 14/14 scenarios, incremental outcomes byte-equal to full rebuilds |
| `repo-guard payload-measure` | deterministic two-run byte-identical table; standard/full 44/44 rows byte-stable vs pre-precision; minimal −16,351 B (−14.2%) |
| Packaged-artifact release smoke | executed end-to-end against a CI-identical tarball with the static-alignment assertions |
| `AGENTS.md` ≡ `CLAUDE.md` | byte-identical |

## Phase-8 hardening disposition

Adversarial review of the full delta (8 dimensions, skeptic-verified): 12
confirmed findings, 7 refuted. A follow-up review of the compatibility lane
confirmed 6 more (0 refuted). All 18 are closed:

- Equal-axis strict-subset fabrication of `STATIC_DEVIATION` under blocking
  unknowns (invariant violation) — fixed; abstaining certificates now carry no
  family or computation by construction.
- Stale `CONTEXT_ONLY` release-smoke assertion that would have failed the RC
  gate — fixed and executed against a packaged artifact.
- Python context-budget escape headroom made a provable 6x bound; oracle
  scenario 14 added and adversarially shown to catch the old coefficient.
- Five claim calibrations (candidate recall, corpus size, abstention-gold
  count, ad-hoc timing figure, byte-stability anchor wording) aligned to
  committed artifacts; repository-wide legacy-contract sweep completed with
  recorded transcripts annotated rather than rewritten.
- Legacy preview-era lock handling, best-effort daemon step-down with
  downgrade reclaim, and future-schema fail-closed behavior hardened with
  tests; best-effort mechanisms are documented as advisory, never as
  guarantees.

## Compatibility posture

- v0.2.2 (schema v7) state opened by this line: typed
  `SchemaVersionOutdated` and a mutable-store rebuild via `resync`; the
  mutable database holds only derived, regenerable data.
- Future-schema databases: fail-closed (invalid state; never deleted).
- Preview-era host-less locks: distinct legacy classification with an
  operator-gated, warned removal path; field-verified against the failure
  observed on a real installation on 2026-07-18.
- `product-schemas.v1` remains purely additive across the whole delta;
  `standard`/`full` verbosity tiers are byte-stable against this line's
  pre-precision shape (the inline member cap of 20 is the one declared
  default-tier behavior change relative to v0.2.2).

## Documented deferrals and open risks (none undermine a shipped claim)

1. Phase 7 full agent-study grid not run (authorization for the pilot only;
   pilot proved harness mechanics at N=2, no effect claims). The pilot's one
   substantive signal — MCP adoption 0/4 in the tool-enabled arm — is an open
   product question about instruction steering, not a correctness defect.
2. v2 retrieval ablation matrix and the RQ3 non-member mutation matrix remain
   deferred with documentation.
3. Precision-return slices S1/S3/S4 (member-serializer sub-batch), S9, S12,
   S13 are not implemented; S11 (the single breaking `product-schemas.v2` cutover that
   flips the default tier) is future work explicitly gated on its own
   compatibility review.
4. The R6 check-abstention echo saving (~600 B) is unrealizable by design:
   ambiguous resolutions must keep their candidate handles.
5. S4 reference views (no-op sync 25 s → 1-3 s target) deferred; current
   no-op sync is ~25 s at real scale.
6. Retrieval quality is honestly mid: hit@1 21/42 on retrieval-intent
   natural-language queries, with zero false-family selections; abstention
   remains the failure mode.
7. Cross-version daemon step-down and legacy-lock removal are best-effort by
   design and documented as such; index-write mutual exclusion rests on the
   index lock.

## Authorization boundary

This verdict does not publish, push, tag, merge, or modify any released
artifact. Any release built from this line must re-run the full gate table
above on the release commit.
