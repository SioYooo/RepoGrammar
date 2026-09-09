# Fortran language completion review

- Status: Incomplete — `structural_substrate`
- Authority: ADR-0020, ADR-0030, ADR-0034, and ADR-0051
- Dependency prerequisite: `5e8fda053122fb0cfd093b28767b9479bd7ddc80`
- Last updated: 2026-09-05

## ADR-0020 gate

- [x] Conservative discovery/config and dependency inventory — frozen lowercase
  non-preprocessed fixed/free forms, exact `fpm.toml`, bounded root dependency
  strings, typed unsupported/conflict/malformed/resource UNKNOWN, persistence,
  incremental replacement/removal, and no execution (ADR-0034, unchanged by
  this lane; `fpm.toml` inventory scope is untouched).
- [x] Authoritative frontend for the declared scope — ADR-0051's bounded
  hand-written free-form lexer and recursive structural parser reads only
  `.f90`/`.f95`/`.f03`/`.f08` paths. ADR-0051 D1 supplies the invariance
  argument in the ADR-0040 shape: the admitted constructs (`!` comments,
  quoted literals with doubling, `&` continuation with token gluing, `;`
  separators, case-insensitive keywords, the program-unit skeleton of
  module/program/external procedures) lex and nest identically under every
  free-form Fortran 2008+ compiler. What would move the parse — preprocessor
  lines, `INCLUDE` text, fixed-form column semantics, submodules, labeled DO,
  nested modules — is refused whole-file with a named typed `UNKNOWN` rather
  than read. ADR-0034 contains no evidence-ladder prohibition on text
  matching; its restrictions name executing Fortran tooling, and nothing runs.
  Flang remains `NO_GO`. Dispatch is wired at the parser port
  (`Language::Fortran` source arms in `src/rust/adapters/parsing/mod.rs`) and
  `application/indexing.rs` now admits Fortran source instead of classifying
  it inventory-only, so production indexing feeds the frontend.
- [x] RepoGrammar-owned Fortran code units and IR — one module unit per
  admitted file, one `fortran_test_drive_subroutine` unit per admitted anchor
  with a source range over the whole subroutine construct, content hashes, IR
  nodes projected as functions (the Go-test-function precedent), and
  module→subroutine containment edges through the shared
  `ir_nodes_for_units`/`ir_edges_for_units` helpers.
- [x] Complete source-semantic obligation registry and provider fallback —
  `FORTRAN_OBLIGATION_REGISTRY` in
  `src/rust/adapters/parsing/fortran/testdrive.rs` is the single table that
  declares every typed `UNKNOWN` the test-drive frontend can emit, and all
  emission paths route through it, so an unregistered unknown is a typed error
  rather than a possible output. Ten entries cover: the blocking identity
  obligation `testdrive_use_not_proven` (the ADR-0051 D2.4 in-file
  `use testdrive` proof, mirroring the r lane's
  DESCRIPTION-declares-testthat fact), the parse-boundary obligations
  (`preprocessor_directive`, `include_statement`, `fixed_form_signature`,
  `unadmitted_fortran_construct`, `parser_depth_limit`, `source_byte_limit`,
  `parser_resource_limit`), and the two standing residuals the bounded parse
  can never discharge — `testdrive_module_binding` (which module file
  satisfies `use testdrive`) and `test_registration_unproven` (whether a
  collect subroutine registers the test with `new_unittest`) — which ride on
  every admitted anchor as non-blocking `UNKNOWN`s. Registry tests pin that
  every emitted kind is declared exactly once and that only the identity
  obligation may record a blocking impact. The blocking column is enforced at
  the frontend rather than mirrored in `application/family.rs`, whose
  `FamilyUnknownDomain` still names no Fortran arm: a test-shaped subroutine
  without a proven `use testdrive` emits the obligation and no anchor unit at
  all, so nothing reaches the family gate to be blocked. That is fail-closed
  and verified on `testdrive_missing_use`, but it means the family layer
  carries no independent Fortran claim-impact rule; a superseding slice that
  wants one must add the domain arm.
- [x] One exact Fortran family with support at least three — the fixed
  `testdrive.test_subroutine` target, the
  `framework:testdrive.test_subroutine` role in
  `src/rust/adapters/frameworks/fortran.rs` (registered in the shared
  detector and in `application/query_terms.rs` as the `testdrive` alias,
  known token, and test concept), and six positive anchor instances across
  two committed fixture files. Family formation is landed too: the
  `application/family.rs` compatibility and derived-support arms
  (`repogrammar-fortran-derived` / `bounded_fortran_testdrive_v1`), the
  family-eligible kind, and the sqlite repo-shape allowlists.
  `min_family_support` pins **3** for `fortran` explicitly rather than
  inheriting the shared default of two, which the low-support fixture's two
  anchors now pin. Measured through the product CLI today:
  `testdrive_exact_tests` forms
  `family:fortran:fortran_test_drive_subroutine:framework_testdrive_test_subroutine`
  with support **6**, and `testdrive_lookalikes`, `testdrive_low_support`,
  `testdrive_parse_degraded`, and `testdrive_missing_use` each index to
  `status == "complete"` and form none.
- [x] Positive, lookalike, low-support, and parse-degraded family fixtures —
  `src/fixtures/fortran/release/v0_2/` carries
  `testdrive_exact_tests/` (module-wrapped canonical suite with a plain test,
  a multi-dummy test, and an attribute-order variant; plus three top-level
  external tests with their own `use` statements),
  `testdrive_lookalikes/` (wrong first-dummy type, `function test_x`,
  `class(error_type)`, extra `optional` attribute, `only:` list without
  `error_type`, renamed binding, non-`test_` name, internal procedures,
  type-bound sections, interface bodies), `testdrive_missing_use/` (the
  blocking identity unknown), `testdrive_low_support/` (two admitted anchors),
  and `testdrive_parse_degraded/` (fixed-form column-6 content, preprocessor
  directives, `INCLUDE` statement — each a whole-file typed refusal with a
  degraded diagnostic). Module tests read every fixture file and pin its
  outcome; preprocessed-suffix inputs never reach the parser at all because
  discovery defers them (ADR-0034). Unresolved/resolved fixture pairs do not
  exist: there is no Fortran provider to resolve against.
- [x] Source-free readiness and leakage review for claim-bearing analysis —
  the frontend-level guarantees are tested (no subroutine, module, or dummy
  name reaches any fact target, note, assumption, or code-unit id; units are
  identified by kind and byte range; all registry text is fixed source-free
  strings), and product-level `init`/`index`/`families` tests over every
  Fortran fixture now live in `src/rust/bin/repogrammar.rs`. One qualifier
  belongs on the record: the shared CLI matrix helper
  `assert_scanner_lane_readiness_is_source_free` (`status`, `doctor`,
  `stats`, `unknowns`, `families`, `files`, `inspect_readiness`) is still
  called only for the delphi, ada, matlab, visual-basic, and r lanes.
  Fortran's `stats` and `unknowns` output was re-inspected empirically on
  2026-09-05 and exposed only bounded tokens, counts, role names, and reason
  codes; that check is not yet pinned by a committed assertion.
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings; open findings remain blockers.
- [ ] Linked atomic prerequisite commits and final completion audit — open.
  The maintainer ruled on 2026-09-05 that this gate requires the frontend/IR
  slice and the `UNKNOWN`/provider slice to land as separate atomic commits.
  This lane arrived as one integrated slice whose frontend emits its typed
  `UNKNOWN`s inline, so satisfying the ruling would mean re-authoring
  intermediate states that never existed rather than re-slicing existing
  ones. No external artifact blocks it.

## Current evidence and blocker

The framework backlog decision ADR-0051 D0 records is the one Wave F2 said
Fortran needed: stdlib/test-drive (github.com/fortran-lang/test-drive, module
`testdrive`) is selected as the first Fortran family — pure Fortran,
fpm-native, MIT OR Apache-2.0, with the exact `test_interface` signature
(`type(error_type)`, `intent(out)`, optionally `allocatable`) verified from
the framework's own sources. pFUnit was recorded as not-first because its
xUnit directives are preprocessor products inside a lane that refuses
preprocessing; Vegetables/tUnit are named follow-ups.

Exactly one anchor shape is admitted: a module-scope or top-level
`subroutine test_<name>` whose first dummy is declared `type(error_type)`
with `intent(out)` (plus optional `allocatable`), with the `error_type`
accessibility parse-proven by a `use testdrive` statement in the candidate's
own scope or the enclosing module's. Plain uses and `only:` lists containing
a bare `error_type` prove; rename lists, `only:` lists without `error_type`,
and `use, intrinsic ::` do not. A test-shaped subroutine without proof
records the blocking `testdrive_use_not_proven` unknown. Module
accessibility deliberately does not refuse anchors because test-drive's own
README keeps tests module-private behind a public collector. Functions,
interface bodies, internal procedures, and type-bound sections never anchor.

No integration blocker remains. `application/indexing.rs` admits Fortran
source, `application/family.rs` carries the support-compatibility and
derived-support arms and the family-eligible kind, the role is registered in
the shared detector and in `application/query_terms.rs`, the sqlite
repo-shape allowlists count the language, and the family forms in product
output at the three-member bar. Each of the six anchors in the exact-test
corpus carries its two standing residual `UNKNOWN`s, as the registry
declares.

What is open is gate 9's commit shape, and one known limitation:
`readiness_language_scopes()` in `application/query.rs` — the list deciding
which languages the public `stats` surface reports with a
`language_scope`/`preview_status` — still contains only python,
typescript/javascript, rust, java, csharp, and c/cpp. It was deliberately not
extended, because that list is a product support-scope claim requiring its
own decision rather than internal counting. Fortran's repo-shape counts
therefore exist in storage and are not surfaced through the public
per-language stats rows.

Three defects found this session applied to this lane and are recorded rather
than left implied. `family_eligible_kind` listed
`fortran_testdrive_subroutine` while the frontend emits
`fortran_test_drive_subroutine`, so the Fortran family could never form at
all; the kind now matches. `min_family_support` fell through to the shared
default of two, so a pair of anchors would have formed a family this review
says must not form. And the sqlite repo-shape stats allowlists
(`REPO_SHAPE_LANGUAGE_SCOPES` and all four `repo_shape_*_where` predicates)
omitted Fortran, so the language silently counted zero; the pre-existing
four-predicate invariant test now actually covers it.

## Completion verdict

Not complete. Fortran has a bounded free-form frontend over one path class
and one signature shape, owned units and IR, a complete claim-scoped
source-semantic obligation registry with provider-fallback policy, committed
positive/lookalike/low-support/parse-degraded fixtures, a role registry
feeding the shared framework detector, and a family that forms in product
output at the three-member bar. It has no provider and no final audit. Strict
gate count is `8/9` with gate 9 open on the commit-shape ruling above;
Fortran is `structural_substrate` and must not be counted as supported.

Two limitations are worth stating rather than leaving to inference.

The parser recognizes only its declared subset, so a file using a construct
outside it — a labeled DO, a named construct (`outer: do … end do outer`), an
`enum` block, an old-style intent-statement spelling of the interface, or a
statement this frontend cannot structurally close — abstains for the whole
file rather than anchoring what it recognized first. That is deliberate; the
degraded diagnostic and the `fortran_test_parse` `UNKNOWN` say when it
happened, not how many tests were missed. The fixed-form detector is likewise
conservative: its column-6 rule can refuse a free-form labeled statement
indented by exactly five spaces, and its column-1 `*` rule cannot see
`C`-comment lines that also parse as free-form statements — those fall
through to the generic unadmitted-construct refusal when they do not parse.

The anchor is a claim about a declaration's shape and an in-file import, not
about registration or module resolution. Whether a collect subroutine
registers the test, and which module file satisfies `use testdrive`, are the
standing `test_registration_unproven` and `testdrive_module_binding`
residuals on every anchor. The registry records them under
`no_fortran_provider_irreducible`: ADR-0034 forbids executing Fortran
tooling, no provider slot exists or is registered, and the
zero-external-dependency constraint admits none, so the obligation cannot be
discharged in practice. The product's `unknowns` surface answers a different
question and reports them on its own axis — on `testdrive_exact_tests` it
emits `recoverable_unknowns: 12`, `irreducible_unknowns: 0`, with
`not_implemented_in_current_version` recovery for the six
`framework_semantic_provider` residuals — that generic mechanism token is
claimed by a registered but unintegrated provider slot — and
`manual_review_required` for the six `import_resolution_provider` residuals,
which no slot claims. Both statements are true of the same facts.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | Fortran / 19 |
| Dialect/version | Bounded free-form Fortran 2008+ subset per ADR-0051 D1; no compiler, standard-edition, source-form-flag, preprocessing, include, target, or build-profile selection is made or claimed. Fixed-form suffixes (`.f`/`.for`/`.ftn`) and preprocessed forms stay inventory (ADR-0034) or are refused whole-file when they appear inside admitted suffixes. |
| Provider/frontend/version | Bounded in-process `repogrammar-fortran-testdrive-parser` / `bounded_fortran_testdrive_v1`, a hand-written free-form lexer plus recursive structural parser. No external Fortran parser, grammar, compiler/runtime, or provider. Flang remains `NO_GO` per ADR-0034 D3. |
| Discovery/config | `.f`/`.for`/`.ftn`/`.f90`/`.f95`/`.f03`/`.f08` plus exact `fpm.toml` (ADR-0034, unchanged). |
| Manifest/lockfile | Bounded root `[dependencies]`/`[dev-dependencies]` literal strings from `fpm.toml`; no graph, lock, git/path, target table, or resolved version. |
| Owned source IR / external symbols | Owned units exist for the ADR-0051 anchor only; the IR projects them as functions with module containment. External symbols stay absent; module graphs, interfaces, generics, submodules, coarrays, and generated code are unresolved. |
| Library Contracts | Registry exists, production packs = 0; manifest rows and anchors create no behavior contract. |
| Exact family / fixtures | One exact family, `fortran.testdrive.test_subroutine` under role `framework:testdrive.test_subroutine` over the fixed target `testdrive.test_subroutine`, family id `family:fortran:fortran_test_drive_subroutine:framework_testdrive_test_subroutine`, support bar three pinned explicitly for `fortran`. Measured: six positive instances across two files form the family at support 6; lookalikes for every named negative class, low-support (two anchors), missing-use, and three parse-degraded refusals index complete and form none. Resolved/unresolved pairs do not exist because there is no Fortran provider. |
| Primary UNKNOWN cases | `fortran_testdrive_identity` (`testdrive_use_not_proven` — blocking; `testdrive_module_binding` — standing), `fortran_testdrive_registration` (`test_registration_unproven` — standing), `fortran_test_parse` (preprocessor directive, include statement, fixed-form signature, unadmitted construct, byte/depth/unit limits), plus the inventory lane's `fortran_dependency_inventory` tokens. Standing residuals share one fallback, `no_fortran_provider_irreducible`: no Fortran provider slot exists or is registered, ADR-0034 forbids executing Fortran tooling, and the dependency constraint admits none, so the obligation cannot be discharged in practice. The `unknowns` surface classifies the same residuals on its own axis — measured on `testdrive_exact_tests`, `recoverable_unknowns: 12`, `irreducible_unknowns: 0`, `by_recovery_code` = `manual_review_required` 6 (`import_resolution_provider`), `not_implemented_in_current_version` 6 (`framework_semantic_provider`). |
| Source-free / security | Only free-form Fortran bytes are read; fixed-form and preprocessed files are refused or never discovered. No source text, identifier, literal, or absolute path reaches any fact, note, assumption, or unit id; input is bounded by bytes, block depth, module nesting, and unit counts; no fpm, gfortran, flang, preprocessor, include processor, repository code, child process, or network runs. |
| Completion state / counted | `structural_substrate`; strict gate count `8/9` with gate 9 open on the commit-shape ruling; Top-20 complete = no. |

Four-part review: correctness anchors only on the parse-proven
`use testdrive` import and the exact first-dummy interface, treats module
accessibility as claim-irrelevant with the framework's own README as
evidence, and routes every typed `UNKNOWN` through one obligation registry
whose only blocking entry is the identity obligation, which fails closed by
emitting no anchor at all rather than by a family-layer rule; security bounds
input
by bytes, depth, and unit counts, executes nothing, and keeps every registry
note, claim, and kind a fixed source-free string; completeness lacks the
committed CLI-surface readiness assertions, the public per-language `stats`
rows, a family-layer Fortran claim-impact arm, and the final audit;
performance is a single linear pass over
file bytes with bounded nesting, with no large scientific-project benchmark.
Evidence: `src/rust/adapters/parsing/fortran/free_form.rs`,
`src/rust/adapters/parsing/fortran/testdrive.rs` (the
`FORTRAN_OBLIGATION_REGISTRY` gate 4 table and its emission + registry
tests), `src/rust/adapters/parsing/fortran.rs`,
`src/rust/adapters/frameworks/fortran.rs`,
`src/rust/adapters/languages/fortran.rs`, `src/rust/application/family.rs`
and `src/rust/application/indexing.rs`, `src/rust/bin/repogrammar.rs` (the
product-CLI family tests over every Fortran fixture),
`src/fixtures/fortran/release/v0_2/`, ADR-0034, ADR-0051, and frontend
tests. Exact non-claims: the anchor proves a declaration's shape and
an in-file import, not that the test is registered, runs, or passes; the fpm
inventory proves neither compilation nor module linkage; and no Fortran
dialect beyond the declared free-form subset is claimed.
