# Retrieval-miss diagnosis at main d981d57

Date: 2026-09-30. Status: REPRODUCED_DIAGNOSIS; no production repair implemented.
This new audit preserves the historical corpus and experiment records.

Fresh product-eval: 93/110 matches; 30/47 retrieval; 36/36 correct abstention; false-family selections0; selected-on-abstention-gold0. All17 mismatches reproduce as UNKNOWN/InsufficientSupport.15 terminate below_min_score,2 no_candidate; no hydration/freshness/support gate is reached by these misses. Exact hydration proves all expected prefix families exist and are fresh.

The frozen corpus stays unchanged. Its ok/prefix gold is product intent, not permission to weaken current selection gates.

| Query | Target | Gold prefix | Actual score / candidates / gate | Classification |
| --- | --- | --- | --- | --- |
| py-nl-db-transactions | How are database transactions handled? | sqlalchemy_repository_method | 4 / 3 / below_min_score | coarse_concept_or_variant_ambiguity |
| py-nl-validation-models | How are validation models implemented? | pydantic_model | 4 / 5 / below_min_score | coarse_concept_or_variant_ambiguity |
| py-nl-repository-methods | How are repository methods structured? | sqlalchemy_repository_method | 7 / 3 / below_min_score | coarse_concept_or_variant_ambiguity |
| py-syn-endpoint | endpoint | fastapi_route | 4 / 1 / below_min_score | gold_vs_bare_concept_contract |
| py-syn-http-handler | http handler | fastapi_route | 4 / 1 / below_min_score | qualified_phrase_gap |
| py-syn-db-model | db model | sqlalchemy_model | 4 / 5 / below_min_score | coarse_concept_or_variant_ambiguity |
| py-syn-orm-model | orm model | sqlalchemy_model | 4 / 5 / below_min_score | coarse_concept_or_variant_ambiguity |
| py-syn-schema-validation | schema validation | pydantic_model | 4 / 2 / below_min_score | coarse_concept_or_variant_ambiguity |
| py-nl-para-rest-endpoints | Where do we define REST endpoints? | fastapi_route | 4 / 1 / below_min_score | qualified_phrase_gap |
| py-nl-para-wire-routes | Show me how HTTP routes are wired up | fastapi_route | 4 / 1 / below_min_score | qualified_phrase_gap |
| py-nl-para-writing-tests | What's the pattern for writing tests here? | pytest_test | 4 / 1 / below_min_score | qualified_phrase_gap |
| py-nl-para-db-sessions | Show me how DB sessions are used | sqlalchemy_repository_method | 4 / 3 / below_min_score | coarse_concept_or_variant_ambiguity |
| py-nl-para-talk-to-db | How do we talk to the database? | sqlalchemy_repository_method | 4 / 3 / below_min_score | coarse_concept_or_variant_ambiguity |
| py-nl-para-validate-payloads | How do we validate request payloads? | pydantic_model | None / 0 / no_candidate | unrecognized_vocabulary_with_latent_ambiguity |
| py-nl-para-request-schemas | Where are request schemas validated? | pydantic_model | 4 / 2 / below_min_score | coarse_concept_or_variant_ambiguity |
| py-nl-para-repo-pattern | How is the repository pattern used? | sqlalchemy_repository_method | 4 / 3 / below_min_score | coarse_concept_or_variant_ambiguity |
| py-nl-para-data-access | Where is the data-access layer? | sqlalchemy_repository_method | None / 0 / no_candidate | unrecognized_vocabulary_with_latent_ambiguity |

## Evidence and recommended bounded follow-up

Normalization/scoring source: query_terms.rs:304 concept aliases;333 qualified phrases;526 normalize_query;558 score_family_candidates;658 adjacent phrase classifier. Selection gates: query.rs:78 score floor10;4194 run_term_retrieval. Bare endpoint is explicitly guarded by query.rs:13904 test and query_terms.rs:949 negative phrase test.

Four qualified-phrase candidates are HTTP handler, REST endpoint, HTTP route, writing test. Reuse the existing adjacent two-term qualifier, keep floor10/margin1, hydrate through the existing freshness gate, and preserve ties. No broad synonym rewrite, punctuation fuzzy matching, framework inference, or global score-threshold change. Tests must include bare endpoint, typographical lookalikes, competing route/test families, stale source, unsafe paths and full product/sync/payload oracles.

The other13 cannot honestly become thirteen safe hits through vocabulary alone:1 is a documented bare-concept abstention,10 have coarse-concept or true variant collisions,2 lack terms and would still expose variant ambiguity. Consider support/profile-aware retrieval only as a separately specified bounded milestone. Do not edit old gold to make results green; add a versioned corpus separating safe selection, useful candidates and intended unsupported/ambiguous gold.

Pydantic cluster v9e3a0ddde854 requires pydantic_model_base; vf950ceff8895 requires pydantic_settings_base. SQLAlchemy method v0a7f92ef094a requires sqlalchemy_transaction_boundary; vd741755a0d57 requires sqlalchemy_query_call. Current pydantic schema probe ties two at score10; sqlalchemy transaction ties three at score10; sqlalchemy repository method ties both method clusters at13. The margin gate correctly abstains.

Full per-row classifications and producer hashes: [retrieval-miss-audit.v1.json](data/retrieval-miss-audit.v1.json). Raw captures are retained outside the repository. Product-eval output SHA256 817a3f135b9308d295ef49498afcc16be8639885cf4e365d266c6f8d96d74e6a. No source code or historical artifacts changed.

## Reproduction and evidence limits

```text
PATH=<qualified Python-3.10+-tool-PATH> repo-guard product-eval --corpus src/fixtures/evaluation/query-corpus-v1.json --bin <pinned-d981-product> --condition d981-miss-audit --out <new-outside-repo-directory>
```

The harness indexes committed fixtures in isolated copies, then executes every
frozen query. Exact source questions and expected families are committed in
the corpus; use each JSON row's `target` for an isolated CLI reproduction. Never
initialize or refresh the checkout's own index to reproduce this audit.

The prior archived output's harness-commit field was `7351b2a`, while its actual
product binary came from later code. It is not a pure `7351b2a` binary baseline.
This new run pins both checkout metadata and producer executable/worker/corpus
hashes. This audit uses the frozen d981 executable; later instrumentation uses
separately hashed builds and does not relabel that baseline. Results are
deterministic fixture retrieval evidence, not agent
adoption or arbitrary-repository quality.

Classification separates the **observed stop gate** (15 below_min_score,2
no_candidate,0 hydration) from inferred unresolved obligations. Variant
ambiguity was independently probed with framework-qualified queries and still
abstained at margin_too_close. No missing family, stale family, worker crash or
support promotion defect was found among these17. That does not prove absence
of those defects elsewhere. The exact same17 mismatches reproduced.

## Predeclared repair gate

Before adding the four phrases, extend existing normalization and public-query
tests to fail on the pre-change queries. Require negative typo/bare endpoint,
competing route/test family, stale source, invalid target and exact-precedence
controls. Run unchanged product-eval, full sync-equivalence and payload matrix;
require no false-family increase or correct-abstention decrease. Do not lower
score10/margin1, infer framework identity, merge variant families, or rewrite
gold. The current evidence justifies a bounded repair candidate, not a measured
post-repair improvement. The four phrases are corpus-informed development
queries; a separately frozen held-out paraphrase set is necessary before a
general retrieval-quality claim.
