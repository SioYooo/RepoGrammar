# Declarative language-pack spike (C#)

Status: **QUALIFIED**, with one recorded oracle-definition deviation (see
[Oracle definition defect](#oracle-definition-defect)). Base:
`main@0104073a0360ce03ff06891b7c3370a24c1d702f`, branch
`feat/declarative-language-pack-spike`. Local only; no push or merge. The
preregistration below is unchanged from commit
`63aef3443ba51a8878026f09057a2b0058427498`; results follow it.

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

## Results

One round was needed; no failure class required a second round.

| Commit | Purpose |
| --- | --- |
| `63aef3443ba51a8878026f09057a2b0058427498` | Preregistration and capture harness |
| `305a61426547043ba1f6babfb1a80550f69f54d8` | Pack type, C# pack, node query, ADR-0057; legacy kept under `cfg(test)` with differentials |
| `0e9fbd604da3c762bd35f9eb9dce72be4a4346c1` | Legacy removal; differentials become pinning and snapshot tests |

Candidate products: `cand1` was built from the `305a614` tree before
formatting (formatting only), `cand2` from the `0e9fbd6` tree before one
three-line doc-comment edit, and `cand3` from `0e9fbd6` itself. Debug builds
are not byte-reproducible, so the final evidence is `cand3`.

### a. Byte equivalence

| Product | Capture rows (179 roots) | Mismatches vs baseline | `sync-equivalence --all` | `product-eval`, preregistered exclusions | Same, plus `summary.latency_ms_p50` |
| --- | ---: | ---: | --- | --- | --- |
| `cand1` (`16e643049a5c`) | 4,681 | 0 | 14/14 pass | **not equal** | equal |
| `cand2` (`eae5cdbec336`) | 4,681 | 0 | 14/14 pass | **not equal** | equal |
| `cand3` (`a9492e55bba8`) | 4,681 | 0 | 14/14 pass | **not equal** | equal |

Two baseline captures agreed on all 4,681 rows before any candidate run. Every
row covers byte-identical units, facts, families, members, evidence, UNKNOWN
inventory, deep family/member reads, `find`/`check`/`explain`, MCP payloads
and the full 14-table generation fingerprint. That includes all non-C# roots,
so the result also covers criterion c.

The in-process differentials passed before the legacy code was deleted:

- Extraction: identical `ParseReport`s (units, IR, facts, diagnostics) for
  all 11 committed `.cs` fixture files and a 15-source branch corpus. The
  branch corpus covers identical-range class/module nodes, file-scoped and
  nested namespaces, nested classes, `#if` regions, minimal-API builder chains
  and nested invocations, convention routing, partial/`dynamic`/source
  generator markers, attribute recognition for every test framework, EF Core,
  FluentValidation and a parse-degraded source. Full-report equality was
  established once, before deletion. The retained
  `node_query_scanner_branch_corpus_snapshots` test pins only unit kinds and
  UNKNOWN claims; detail such as `MemberData` resolution, template shapes and
  `test_data_shape` stays covered by the existing C# unit tests.
- External corpus: 4,818 of 4,820 local Unity `.cs` files (read-only; manifest
  SHA-256 `87f9d286a814702cfbd05d8f7d4d096d224d0097e05a6aa247c18e1b857b407a`,
  path/content-hash lines sorted by path) were identical. Two non-UTF-8 files
  were skipped before parsing:
  `.../com.unity.render-pipelines.high-definition/Editor/RenderPipeline/VirtualTexturingSettingsUI.cs`
  and `.../Samples~/Environment Samples/Scripts/CelestialBodiesManager.cs`.
  The corpus is not committed, and the skip says nothing about product
  discovery of non-UTF-8 files. The opt-in test that ran it was removed with
  the legacy traversal, so this result cannot be re-run from the final tree.
- Family policy: an exhaustive comparison of all 13 `UnknownReasonCode`s, 21
  claims and 13 roles; identical variation and characteristic tables and
  minimum support; per-prefix pair compatibility (more than 10,000 pairs,
  including blocker variants); identical feature extraction; and identical
  support-fact safety across every C# target and origin/assumption variants.

### Oracle definition defect

The preregistered `product-eval` exclusion list was built from the fields that
differed between two baseline runs. `summary.latency_ms_p50` is
`percentile(&all_latencies, 50)` in `repo_guard.rs`, the same kind of timing
aggregate as the excluded `summary.latency_ms_p95`. It happened to be 40 in
both baseline runs, so it was missed. A third run of the same baseline binary
gave 44 and also fails the strict comparison against the first baseline.
Under the preregistered list, every candidate differs only in that field.
With it also excluded, every candidate is equal to the baseline, and so are
both other baseline runs.

| Run | Binary | `summary.latency_ms_p50` | `summary.latency_ms_p95` | Strict vs baseline run 1 |
| --- | --- | ---: | ---: | --- |
| baseline run 1 | baseline | 40 | 69 | equal |
| baseline run 2 | baseline | 40 | 67 | equal |
| baseline run 3 | baseline | 44 | 73 | **not equal** |
| cand1 | candidate | 57 | 124 | **not equal** |
| cand2 | candidate | 48 | 76 | **not equal** |
| cand3 | candidate | 52 | 81 | **not equal** |

The preregistration text is not edited. Latency fields differ between runs,
the runs were not isolated, and no performance claim is made in either
direction.

### b. Family layer

`application/family.rs` (non-test) no longer contains any C#-dedicated
function, constant or `FamilyUnknownDomain` variant. The only remaining
`csharp` mentions outside tests are the framework-module import list and the
registry entry:

```text
7:    ada, cpp, csharp, delphi, fortran, go, java, matlab, php, r, ruby, sql, swift, tsjs,
915:static LANGUAGE_FAMILY_PACKS: &[&LanguageFamilyPack] = &[&csharp::FAMILY_PACK];
```

C#-specific non-test mentions dropped from 90 to 2. The 266 non-blank lines of
C#-dedicated family functions are replaced by 142 lines of generic pack code
that any future pack reuses. Other languages' functions were left in place, so
the generic `pack_context_variation_slots` body still duplicates five
per-language variation-slot functions. Non-test `family.rs` went from 5,132 to 5,010
non-blank lines.

### c. Other languages

Covered by the capture rows above, plus `sync-equivalence --all` passing all
14 scenarios for each candidate, as at baseline.

### d. Repository checks (final tree)

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace --all-features` | PASS (2,291 + 126 + 185 + 1 passed; 2 pre-existing ignored) |
| `repo-guard check` (clean local clone) | PASS |
| `repo-guard check` (this checkout) | FAIL, pre-existing: 2,508 `SourceOutsideSrc` paths, all under the orphaned `.claude/worktrees/` copy |
| `repo-guard check-diff --base main --head HEAD` | PASS |
| `repo-guard sync-equivalence --all` | PASS (14/14, `cand3`) |
| `repo-guard product-eval` | Equal apart from timing fields; see the oracle defect |
| `python3 src/workers/python/worker.test.py` (Python 3.14.6) | PASS |
| `node src/workers/typescript/worker.test.js` | PASS |
| `python3 src/experiments/language_pack/capture.test.py` | PASS |
| `git diff --check`, `cmp -s AGENTS.md CLAUDE.md` | PASS |

### Line split (headline)

Of the 1,815 non-blank, non-test lines of the C# extractor at `main`:

| Part | Lines | Now |
| --- | ---: | --- |
| Anchor tables (namespaces, attributes, minimal-API, convention, lookalike) | 52 | Data in `adapters/frameworks/csharp.rs`; two duplicated target lists removed |
| Node-kind dispatch and recursive traversal, including binding collection | 64 | A 19-line `nodes.scm` plus 129 lines of query glue (capture collection, ordering, ancestor context, binding record) |
| Procedural anchor semantics | 1,699 (93.6%) | Unchanged Rust |

The 1,699 lines are the remainder after removing the tables and traversal
listed above. Small shape-classifier matches, such as the visibility, class,
return and route-template shape functions, count as procedural. So 93.6% is
not an upper bound on what could ever be tabulated.

The extractor is now 1,832 non-blank lines. **The spike did not shrink
extraction code.** The query glue is language-neutral and should become a
shared `adapters/parsing` helper when a second lane migrates; only then is it
amortized. Per-language cost is dominated by anchor semantics: scoped `using`
binding, role precedence, receiver chains, `MemberData` scopes, preprocessor
regions, template and signature shapes, and unit/fact construction. Syntax
selection and tables are a small share.

The family side does shrink: the C# family policy is about 160 lines of pack
data, and the 67-line `LanguageFamilyPack` type is shared.

### Adding a language after this change

For a packed Tree-sitter lane, compared with the twelve-item template in
`docs/plans/multi-language-expansion-plan.md`:

- Unchanged: grammar dependency, core-model kinds, discovery, framework
  registry roles, derived-support derivation in `indexing.rs`, query/read
  model, persistence whitelists, fixtures, unknown-reduction pairs, docs.
- New per lane: a `nodes.scm`, the anchor handlers, anchor tables and a
  `LanguageFamilyPack` instance next to the framework registry.
- Removed per lane: every language-specific function in the family builder.
  The family builder gains one `LANGUAGE_FAMILY_PACKS` entry.

### Overlap with `refactor/architecture-preserving-simplification`

That unmerged branch (4 commits, 21 behind `main`) also edits
`application/family.rs` (it merges the blocking and non-blocking UNKNOWN
collectors into `family_unknowns_by_unit`), `adapters/parsing/mod.rs` and
`application/indexing.rs`. It does not touch the C# functions, the pack or the
C# extractor. The two branches can be combined, but expect textual conflicts
in those three files.

### Remaining UNKNOWNs and risks

- Only C# is packed. Java, C/C++, Rust, Python, TS/JS and the bounded
  frontends keep their item-7 functions. C/C++ shares one policy across two
  language tokens, and Rust has repository-level blocking unknowns; both still
  need design.
- `role_family_pack` relies on disjoint role namespaces, which an existing test
  enforces across languages.
- The node-query scanner calls `Node::parent()` for ordering ties and context
  lookup. Its cost on very large files was not measured.
- ADR-0057 was Proposed when this spike closed; the maintainer accepted it on
  2026-10-20.
