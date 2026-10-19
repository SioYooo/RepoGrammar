# Declarative language-pack spike (C#)

Status: **PREREGISTERED**. Base: `main@0104073a0360ce03ff06891b7c3370a24c1d702f`,
branch `feat/declarative-language-pack-spike`. Local only; no push or merge.

## Question

Can one existing Tree-sitter language lane express its extraction node
selection and its family policy as a declarative language pack (a Tree-sitter
`.scm` query, anchor tables and an UNKNOWN rule table) while every product
output stays byte-identical, and can the language's dedicated functions in
`application/family.rs` be removed in favor of one table-driven entry?

This is an architecture spike. It adds no language, framework, family kind,
provider, dependency or public schema, and it changes no support/score gate,
UNKNOWN semantics, gold or fixture expectation.

## Target language

C#. Of the four Tree-sitter lanes (C#, Java, C/C++, Rust), C# has the smallest
family-layer footprint: nine roles and one language token. C/C++ shares one
policy across two language tokens and includes preprocessor/include evidence
gates. Rust has repository-level blocking unknowns and a self-dogfood fallback
role. Java has more roles and test-data resolution. C# has dedicated release
fixtures, `unknown_reduction` fixtures and the `csharp_edit` incremental
scenario.

## Pack shape (draft)

- A language-neutral `LanguageFamilyPack`/`RoleFamilyPolicy` data type in
  `core::model`. It uses only std slices, function pointers and the existing
  core `UnknownReasonCode`, so the spike adds no application-to-adapter
  coupling.
- The C# instance in `src/rust/adapters/frameworks/csharp.rs`, the existing C#
  framework registry. It holds the anchor engine/method and derived-support
  identity, the minimum support, the assumption-to-feature table, and per-role
  variation slots plus equal/required-equal compatibility prefixes. The
  characteristic prefixes are their concatenation. The UNKNOWN rule table lists
  the blocking reasons and claims and the non-blocking reasons and claims, plus
  the default affected claim and the recovery scope.
- One generic entry in `family.rs` for each current dispatch point: variation
  slots, variation prefixes, unknown-blocker exclusion, feature extraction,
  pair compatibility, characteristic prefixes, minimum support, support-fact
  safety and UNKNOWN classification. `FamilyUnknownDomain::CSharp` becomes
  `FamilyUnknownDomain::Pack(&'static LanguageFamilyPack)`. Other languages
  keep their current paths.
- `application/indexing.rs` keeps its support-derivation path but reads the C#
  anchor identity, derived-support identity and role compatibility from the
  pack instead of the `family.rs` forwarders.
- Extraction: `src/rust/adapters/parsing/csharp/nodes.scm`, embedded at compile
  time, selects scope, class-like, method, constructor, property, invocation
  and variable-declarator nodes. All variable declarators are consumed before
  any other capture, preserving the current two-pass order (receiver bindings
  first). The remaining captures are visited in syntax-tree pre-order, start
  ascending and end descending, and each node inherits the context of its
  nearest captured ancestor. The namespace, attribute, minimal-API,
  convention-routing and lookalike tables move into the C# pack, and the role
  target lists derive from them. Procedural resolution stays Rust code: scoped
  `using` chains, receiver bindings, `MemberData` scopes, preprocessor regions
  and template shapes.

## Primary evidence (all required for `QUALIFIED`)

a. Byte equivalence for every committed fixture root (179 roots), measured
   with `src/experiments/language_pack/capture.py`. Captured surfaces: CLI
   `init`, `resync`, `files`, `units`, `families` and `unknowns`; deep `family`
   for every family; deep `member` for every unit; `find`, `check` and
   `explain` for every indexed path; MCP `show_family` for every family and
   `find_analogues` for every indexed path; and the full 14-table
   owned-generation fingerprint from `compare_generations.py`.
   Only `active_generation`/`generation_id` keys are dropped. Two baseline runs
   must first agree with zero mismatches; the candidate must then have zero
   mismatches. `product-eval` results must be equal after excluding exactly
   the fields that differed between two baseline runs (`started_at`,
   `finished_at`, `fixtures[].resync_latency_ms`,
   `results[].latency_ms_all_reps`, `results[].latency_ms_p50`,
   `summary.latency_ms_p95`) and the provenance field `repogrammar_commit`.
   Before the old scanner is removed, a temporary in-process differential
   test must also find identical `ParseReport`s from the old and new scanners.
   It covers every committed `.cs` fixture file plus an external read-only
   corpus of 4,820 local Unity `.cs` files, which are not committed and are
   recorded by count and hash. Before the old family functions are removed,
   an exhaustive differential test compares old and new classification over
   every `UnknownReasonCode`, every C# claim literal plus `family:`-prefixed and
   foreign claims, and every C# role plus unknown and empty roles. It also
   checks every role's variation, characteristic and compatibility tables.
b. `family.rs` no longer contains `csharp_context_variation_slots`,
   `csharp_variation_feature_prefixes`, `add_csharp_family_features`,
   `csharp_characteristic_prefixes`, `csharp_evidence_pair_is_compatible`,
   `csharp_unknown_*`, `csharp_support_fact_*`,
   `csharp_support_target_is_role_compatible`, `csharp_framework_role_is_known`,
   `CSHARP_DERIVED_SUPPORT_*`, `CSHARP_MIN_FAMILY_SUPPORT` or a
   `FamilyUnknownDomain::CSharp` variant; C# reaches each dispatch point through
   the single pack lookup.
c. All non-C# fixture rows are unchanged (covered by a), and
   `repo-guard sync-equivalence --all` passes every scenario as at baseline.
d. `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
   --all-features -- -D warnings`, `cargo test --workspace --all-features` and
   `cargo run --quiet --bin repo-guard -- check` pass.

Auxiliary evidence, headline first: the split of the C# extractor
(`adapters/parsing/csharp.rs`, excluding tests) into lines that became data or
query (pack tables and `.scm`) and lines that remain procedural Rust, with the
procedural parts named. Then the net line change for the C# lane and the family
layer, and the list of files a future language would add or edit.

## Labels, failure classes and stop rules

- `QUALIFIED`: a-d all hold.
- `PARTIAL`: extraction equivalence holds but some family policy cannot be
  tabulated without a language-specific branch, or the reverse.
- `NO_GO`: the declarative form cannot express existing semantics without
  output loss; the exact constructs are listed.
- `INCONCLUSIVE`: evidence cannot be produced (environment failure).

Failure classes: query expressiveness (with constructs), preprocessor/macro
semantics loss, untabulable family branch, equivalence diff (with sample),
environment failure. At most three rounds; each round makes one conservative
change aimed at the observed failure class and adds one negative test. Stop on
`QUALIFIED`, a definite `NO_GO`, an architecture blocker or an environment
failure that cannot be fixed safely.

## Non-claims

No new language support, Top-20 progress, performance improvement or
semantic-precision claim. Tree-sitter remains candidate generation; family
membership still requires exact anchors and the support threshold.

## Baseline environment notes

- The checkout's `target/` held artifacts built from a former checkout path, so
  debug binaries embedded a nonexistent source worker path and every Python
  resync failed with "python project frontend session failed". After
  `cargo clean -p repogrammar` and a rebuild, `repo-guard sync-equivalence --all`
  passes and `product-eval` reproduces retrieval 34/47 and abstention 36/36.
- `python3` on the default PATH is 3.9; the harnesses use Homebrew Python 3.14.6.
- `repo-guard check` in this checkout reports 2,508 `SourceOutsideSrc` paths, all
  under the Git-ignored `.claude/worktrees/agent-a648fb0bb00250b25/`, an orphaned
  agent worktree copy whose registered path is a former checkout location. It
  is left untouched. `check` is therefore run in a clean local clone of the
  branch, and `check-diff --base main --head HEAD` runs in this checkout.
- Baseline capture (harness SHA-256
  `73bac80c6eea9f6b7ca46b6089be8bff57b5bc0078e48228b46a02b422220e71`): 179
  roots, 4,681 rows; two baseline runs agree with zero mismatches. Baseline
  `sync-equivalence --all` passes all 14 scenarios; baseline `product-eval`
  reports retrieval 34/47, abstention 36/36 and context 27/27. Raw outputs stay
  outside the repository.
- Frozen baseline binaries (SHA-256): `repogrammar`
  `3211c5960506b644f18b8c9262b23ecfec9b20eae0d9edcef0e71d4022cd832c`,
  `repo-guard` `476517edc5a022c7f30654e613fdbcc366c8702c232247415365d4c3b91ab3f0`.

## Reproduction

```text
python3 src/experiments/language_pack/capture.test.py
python3 src/experiments/language_pack/capture.py --repo <checkout> --bin <product> --python <python3.10+> --scratch <outside-checkout-dir> --out <capture.json> [--compare <baseline-capture.json>]
<matching-repo-guard> sync-equivalence --fixture src/fixtures/incremental_equivalence/v1 --all --bin <product> --out <dir>
<matching-repo-guard> product-eval --corpus src/fixtures/evaluation/query-corpus-v1.json --bin <product> --out <dir>
```
