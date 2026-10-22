# Pytest-variant narrowing preregistration

Date: 2026-10-22. Status: PREREGISTERED before any probe run on this branch and
before any retrieval-code change. Base: `main` at `d48008f`. This document does
not edit the 2026-10-14 capability review; it executes that review's section 9.

## Objective and non-goals

Six prospective public-repository queries retrieve the correct role prefix
(`family:python:pytest_test`) but abstain with `margin_too_close` because several
existing pytest *variant* families tie at the same score. Question: can a locus a
user could plausibly add (path, member symbol) or the existing constraint profile
**provably** narrow a real multi-family candidate set to one selectable family,
using only mechanisms already in the contract?

Non-goals and locks: score 10 / margin 1, exact-identity precedence, freshness
gate, typed UNKNOWN, variant boundaries, vocabulary v3, public schemas and the
frozen corpora are unchanged. No boost of `writing tests`, no family merge, no new
family kind, language, framework, or provider, no dependency. A new semantic
mechanism needs its own specification/admission; if narrowing needs one, the
outcome is NO_GO/DEFERRED with the missing contract named, not an implementation.

## The six tied queries (verbatim; from `preregistration.json`)

Source: `src/experiments/language_capability/preregistration.json`; observed
2026-10-14 in `/private/tmp/repogrammar-language-capability-20261014/public-candidate/product-eval-results.json`
(untracked): `unknown`, `InsufficientSupport`, `margin_too_close`, margin 0.

| Id | Corpus | Target | Tied candidate families (pytest variants; suffix after `framework_pytest_test:`) |
| --- | --- | --- | --- |
| f-writing-tests-backend | fastapi-template | How should I approach writing tests for this backend? | v83a77ba72f49, vbc0cc525ed72, v8d38a5ff5e6d, v69fe71823663, vdb45c78725a1, vc4de2b2f72f0, v584cdd1b6b8e (+ non-pytest decoys: typescript jest_vitest, fastapi_route) |
| f-writing-tests-api | fastapi-template | Show the existing style for writing tests before extending the API. | same seven pytest variants (+ same decoys) |
| r-writing-tests-sessions | requests | Where is the pattern for writing tests for sessions? | v94834eabfb5a, v88f106547f45, v9a524eb8ef7b, v94761a8b0697 |
| r-writing-tests-cookies | requests | Show examples of writing tests for cookie handling. | same four |
| r-writing-tests-auth | requests | How should I start writing tests for request authentication? | same four |
| r-writing-tests-redirects | requests | Describe the convention for writing tests around redirects. | same four |

Tie reason: all variants are `pytest_test` families that differ only by their
`fixture_context_nonbuiltin` characteristic profile (for example parametrize
expectations or `client`/`db` fixtures). The query vocabulary has no term that
distinguishes them, so their scores are equal and the margin gate correctly
abstains. Variants interleave inside files (for example six variants have members
in `backend/tests/api/routes/test_users.py`; two in `tests/test_utils.py`).

## Hypothesis (per signal; per query)

Existing, already-specified signals that could narrow a tie:

1. **Path locus** (directory or file token alongside the concept words). The
   directory/composite scope stage (`query-resolution.md`, stage 4) intersects the
   in-scope families with the positively scored families, and a path-shaped file
   target takes the exact/local-context stage. A locus narrows **only if exactly
   one variant has evidence under it**.
2. **Symbol/member locus** (`unit:` id or `family:` id). Exact-identity authority.
   This selects a family by identity; it does not narrow a fuzzy query and is
   recorded as a control only.
3. **Constraint profile.** The profile is hydrated only for an already selected
   family; it is the *definition* of each variant, not a ranking input. It cannot
   break a tie before hydration without a new ranking mechanism.

Predictions, made from the family/member inventory only (not from any narrowing
outcome). Inventory (2026-10-22, release build of this branch on the pinned
snapshots): fastapi variants have complete member lists; requests family v94834eabfb5a
(74 members) has a **truncated** member list.

- f-writing-tests-backend: `backend/tests/crud` holds only v8d38a5ff5e6d -> may narrow (P1).
- f-writing-tests-api: `backend/tests/api/routes/test_private.py` holds only
  v69fe71823663 -> may narrow (P2).
- Four requests queries: no directory or file has provably one variant (`tests/test_requests.py`
  holds v94761a8b0697 plus an unlisted share of the truncated 74-member family;
  `tests/test_utils.py` holds two). Predicted **not narrowable** by path: they
  remain ties (U-r-*).

## Frozen sample sets

Machine-readable and frozen in `src/experiments/pytest_variant_narrowing/samples.json`
(sha256 below). Targets prefixed `@id` expand to the verbatim query above plus the
rest of the string. Classes and expected outcomes:

- **positive** (P1, P2): verbatim query + provable unique locus. Expected: status
  `ok`, selected family = the unique holder (P1 v8d38a5ff5e6d, P2 v69fe71823663),
  hydrated through the existing freshness gate. Premise check: inventory complete
  and exactly one holder; otherwise the sample is void, reported, never reclassified.
- **diagnostic_positive** (P1s, P2s): short form `<locus> writing tests`; recorded,
  not in the decision rule.
- **tie** (T0-* baselines for all six queries; U-r-* four requests queries with
  `tests/test_requests.py`; T1 `backend/tests`; T2 `backend/tests/api/routes`; T3
  `tests/test_utils.py`; T4 `.../test_users.py`): expected no selected family.
- **negative** (N1 pytest-free directory, N2 absent locus, N3 `..` traversal, N4
  disjoint scopes, N5 bare `endpoint`, N6 typo `writing tsets`): expected no
  selected family. N7 (bare locus without concept) is recorded only; existing
  directory-scope behavior is not an abstention claim.
- **stale** (S1): P1 after an unindexed append to `backend/tests/crud/test_user.py`
  in a copy; expected no selected family (the freshness gate must hold).
- **control_exact** (C1 `family:` id, C2 `unit:` id): expected the named family.

## Producers and hashes

Probe: `python3 src/experiments/pytest_variant_narrowing/probe.py --bin target/release/repogrammar --sources <dir> --out <scratch>`
(python >= 3.10; copies pinned snapshots, `init --state-only` + `resync`, `find --mode compact`).
Pinned sources: fastapi-template `4d3d5e92c1ea6b3fa0fab02c41124844ec45bca8`
(tree sha256 `e940c82ce02b63a46543806c5cda6b012fe5e4491e82537ee8b3cddea3ba4082`),
requests `0e322af87745eff34caffe4df68456ebc20d9068`
(`64b28a9a015f67139e70f890047d941c0452d1beceb6791890885ade2f20ee21`), from the
2026-10-14 `public-sources-final` snapshot (untracked). Frozen gates use
`repo-guard product-eval` and `repo-guard sync-equivalence`.

| File | sha256 |
| --- | --- |
| src/experiments/pytest_variant_narrowing/samples.json | a517a9e4626fb1ba4859d8e2504335e8d95e1885d597a3acbc7dfc562f4c5fec |
| src/experiments/pytest_variant_narrowing/probe.py (see note) | 10b4b1a6a774fed32a0904283261941948f003bed3e76faa4b21aa5c1e440127 |
| src/experiments/language_capability/preregistration.json | 9854a75da31e451b78b6ba5d46905fef98cc905bb07dac1719cf1cdf7f0c2e64 |
| src/experiments/language_capability/qualification-controls.json | 55aaf48f47d58d83dc900047f3ddea3e2eb5a0c769462bf66abd7123a4716fe1 |
| src/experiments/language_capability/prepare.py | 01b517253260672bfc36e0b055229e20a39cd98628a482eccab90d937f3d3ea9 |
| src/experiments/language_capability/capture.py | f8365283869f1c3b0f8bde5e421a942e9c2bbdf6a84897b5fee3225b1a3c4062 |
| src/experiments/language_capability/recall.py | dd6f7127301822c7705a1e598a1398396e33519fb29aa1b832962b3400151d01 |
| src/fixtures/evaluation/query-corpus-v1.json | 20c74d6575b8e6677353e28036fc92154d6c29ba46c76d61c2667cdfaf9a5da4 |

Note: the probe is a harness, not product code; a harness bug fix (never a sample,
expectation, or rule change) is permitted and must be disclosed with the new hash
in the results section.

## Decision rule (frozen)

- Safety first: any selected family in a tie, negative, or stale sample, or any
  change in the frozen gates, is a SAFETY_REGRESSION and voids positives.
- **QUALIFIED**: all six tied queries have a provably unique, plausibly supplied
  locus, each selects its unique family with margin >= 1, and no negative, tie,
  or stale sample changes.
- **PARTIAL**: at least one but not all six; every non-narrowable query is
  documented as an existing-contract limit.
- **NO_GO**: no positive selects, or narrowing requires a mechanism outside the
  current contract (for example profile-aware ranking, per-variant vocabulary, or
  query-time fixture inference). Then name the missing contract.
- "Already narrowable" (no code change) is the expected finding if P1/P2 select.

## Gates that must hold after any change

`product-eval` (corpus v1): retrieval 34/47, abstention 36/36, context 27/27,
committed_precision 42/42 not regressed; `sync-equivalence --all` 14/14;
`cargo fmt --check`, clippy `-D warnings`, `cargo test --workspace --all-features`,
`repo-guard check`, `git diff --check`.

## Stop conditions

- Probe shows P1 and P2 select under existing code: stop, no code change, document how.
- A change would need new vocabulary, ranking weight, threshold, or schema: stop with
  NO_GO and name the contract.
- At most three implementation rounds; each names the previous failure class, one
  conservative change, a negative test, and gate results. No tuning of samples after
  results.

## Results

Pending (to be filled after the probe).
