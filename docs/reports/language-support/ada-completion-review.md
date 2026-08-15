# Ada language completion review

- Status: Incomplete — `structural_substrate`
- Authority: ADR-0020, ADR-0030, ADR-0033, and ADR-0045
- Dependency prerequisite: `5e8fda053122fb0cfd093b28767b9479bd7ddc80`
- Last updated: 2026-08-15

## ADR-0020 gate

- [x] Conservative discovery/config and dependency inventory — GNAT-default
  `.ads`/`.adb`, exact GPR/Alire config, bounded unconditional Alire manifest
  declarations, typed conditional/pin/lock/malformed/conflict/resource UNKNOWN,
  persistence, incremental replacement/removal, and no execution.
- [x] Authoritative frontend for the declared scope — ADR-0045's hand-written
  lexer and recursive-descent parser consume the whole compilation unit against
  the D4a subset or refuse the file; there is no error recovery and no partial
  tree, so a grammar gap costs recall and can never invent a registration. The
  fidelity boundary is the declared subset and the refusal vocabulary is a fixed
  enumeration; parse-degraded behaviour is explicit and tiered. The scope was
  built so that no project model, GPR profile, or Ada edition selection is
  needed for the admitted parse: D4b proves the parse invariant across
  {Ada 95, 2005, 2012, 2022} and refuses every construct that would make it
  edition-, preprocessor-, or naming-dependent. This is not a full Ada frontend
  and not a semantic oracle, and the ADR forbids widening the subset without a
  superseding decision.
- [x] RepoGrammar-owned Ada code units and IR — ADR-0045 emits a module unit
  per decoded `.adb` body and one unit per admitted registration call, each
  projected into the shared IR.
- [x] Complete Ada semantic-obligation/claim-impact registry and provider
  fallback — a `Register_Routine` call without an AUnit `with` clause in the
  same file yields `UnresolvedImport` under `ada_aunit_registration_binding`,
  and it blocks family membership.
- [x] One exact Ada family with support at least three —
  `framework:aunit.test_registration` over the `aunit.Register_Routine` anchor,
  gated at support three.
- [x] Positive, lookalike, and low-support fixtures exist on disk for that
  family, and parse-degraded and build-variant cases are covered by frontend
  tests rather than by workspace fixtures. An unterminated literal, a `gnatprep`
  conditional or substitution, an edition-selecting pragma, and an
  edition-sensitive reserved word each abstain and report a degraded parse;
  constructs merely outside the declared subset abstain with a typed `UNKNOWN`
  and no operator warning. Generic, overload, dispatch, and resolved/unresolved
  fixtures do not exist: there is no Ada provider to resolve against.
- [x] Complete source-free readiness and leakage matrix — Ada is registered
  in the repo-shape language scopes, so its units and families are counted
  rather than silently reported as zero; `status`, `doctor`, `stats`,
  `unknowns`, `families`, `files`, and the MCP `inspect_readiness` and
  `find_analogues` payloads are each asserted over both an indexed positive
  workspace and an indexed unbound one to expose no identifier, literal, or
  source text and no absolute path. The assertions are non-vacuous: every
  command must exit zero and parse, the positive workspace must report
  `framework:aunit.test_registration`, and the unbound workspace must report the lane's typed
  `UNKNOWN` by bounded language token `ada` and count.
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings; open findings remain blockers.
- [ ] Linked atomic prerequisite commits and final completion audit.

## Current evidence and blocker

Discovery and indexing now remain source-free for Ada source and GPR: their
bytes never cross SourceStore, including non-UTF-8 inputs. Only supplied bounded
`alire.toml`/`alire.lock` bytes are read. Exact unconditional direct string
declarations become auxiliary `alire` rows; conditional tables, pins, conflicts,
malformed/resource input, and the internal lock schema abstain through fixed
`ada_dependency_inventory` facts. Product and incremental tests prove CLI mode,
zero source/GPR reads, dependency copy-forward/replacement/removal, no raw path
or source leakage, and zero family output.

On top of that dependency infrastructure, ADR-0045 now supplies an Ada source
frontend: a hand-written lexer and recursive-descent parser over a declared
subset, owned units and IR, a typed registration-binding `UNKNOWN`, and one
exact family. `.ada` and GPR-selected alternative source names remain outside
discovery, and the product still has no project selection, no semantic
resolution, and no provider.

Libadalang's Round-4 result is `NO_GO` for production admission: its documented
GNAT coupling, project-provider file access, and incomplete legality coverage do
not satisfy the current supplied-bytes-only default boundary. No Libadalang,
GNAT, gprbuild, or alr dependency/process was introduced. A future proposal must
requalify a pinned isolated frontend from first principles.

## Completion verdict

Not complete. Ada has a bounded frontend over a declared subset, owned units and
IR, a typed registration-binding `UNKNOWN`, and one exact family with support
three under ADR-0045. Alire dependency presence remains auxiliary and is not
source, library, framework, family, or readiness support. Strict gate count is
`8/9`; it must not be counted as a supported language.

Three limitations are stated rather than left to inference. **Recall is low by
construction.** The declared subset covers ordinary package bodies, subprograms,
declarations, statements, and expressions, but any `.adb` using a generic
declaration, a task or protected unit, a representation clause, a subunit, or a
`gnatprep` directive abstains as a whole and contributes no anchor. That is the
intended conservative failure: the frontend records a typed `UNKNOWN` naming the
refusal class rather than reporting an empty file. **A refused file proves
nothing about its contents**, so a zero-anchor Ada file is not evidence that it
registers no routines. And **the AUnit `with` clause must be in the same file**:
a package body inherits its spec's context clause, so a body that uses AUnit
without naming it is a real shape this frontend does not admit. That false
negative is chosen over asserting an import it cannot see.

One judgement is recorded explicitly. Gate 2 is checked on the reading ADR-0040
established for SQL — "authoritative frontend for the declared scope", where the
scope is declared, the refusals are enumerated, and the parse is proved
independent of what cannot be determined. A reviewer who instead reads gate 2 as
requiring a language-native vendor frontend would keep it open for Ada, and
ADR-0033 D3's Libadalang `NO_GO` means no such frontend is available on this
supplied-bytes-only boundary.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | Ada / 16 |
| Dialect/version | GNAT-default lowercase `.ads`/`.adb`. No Ada edition, GNAT profile, target, scenario variable, GPR project, or alternative naming is selected, and ADR-0045 D4b proves the admitted parse invariant across {Ada 95, 2005, 2012, 2022} so that no selection is needed. Ada 83 is outside the set; `gnatprep` input, edition-selecting pragmas, and edition-sensitive reserved words are refused. |
| Provider/frontend/version | RepoGrammar-owned bounded Ada frontend (hand-written lexer plus recursive-descent parser over the ADR-0045 D4a subset), engine `repogrammar-ada-aunit-scanner`, method `bounded_ada_aunit_registration_v1`. No external provider: Libadalang/GNAT project-provider admission is `NO_GO` for this supplied-bytes lane. |
| Manifest/lockfile | Bounded unconditional direct Alire string requirements; pins/conditional tables/internal lock schema do not resolve dependencies. |
| Owned source IR / external symbols | Owned units and IR exist for the ADR-0045 anchor only; external symbols stay absent, and package, generic, overload, and dispatch identities are unresolved. The `'Access` prefix is read as syntax and never resolved to a routine. |
| Library Contracts | Registry exists, production packs = 0; Alire declarations cannot establish behavior. |
| Exact family / fixtures | One exact family, `framework:aunit.test_registration` over `aunit.Register_Routine`, gated at support three. Positive, lookalike, and low-support workspace fixtures exist; parse-degraded and build-variant cases are covered by frontend tests. Generic, overload, dispatch, and resolved/unresolved do not exist, because there is no Ada provider. |
| Primary UNKNOWN cases | `ada_registration_scan` (construct or character outside the declared subset, unterminated literal, obsolescent replacement character, nesting and byte ceilings), `ada_conditional_compilation` (`gnatprep` directive or substitution), `ada_language_edition` (edition-selecting pragma, edition-sensitive reserved word), `ada_aunit_registration_binding` (registration without an AUnit context clause), plus GPR selection, naming, conditional Alire data, pins, generic instantiation, overload/dispatch, generated code, and provider availability. |
| Source-free / security | GPR is zero-read and `.adb` bytes are parsed in process under byte, nesting, and unit ceilings; refusals carry a fixed vocabulary and never source text; no GNAT, Libadalang, gprbuild, alr, child, repository/dependency code, or network runs. |
| Completion state / counted | `structural_substrate`; strict gate count `8/9`; Top-20 complete = no. |

Four-part review: correctness rests on a parse that consumes the whole
compilation unit or refuses it, with the tick, edition, preprocessor, and
replacement-character hazards each bounded by an explicit rule and an exclusion
test; security preserves the non-execution boundary, parses supplied bytes under
byte, nesting, and unit ceilings, and keeps every refusal surface free of source
text; completeness still lacks a selected project model, semantic resolution,
and the linked completion audit; performance is bounded by those ceilings but
has no large-codebase measurement. Evidence:
`src/rust/adapters/languages/ada.rs`, `src/rust/adapters/parsing/ada.rs`,
`src/rust/adapters/parsing/ada/lexer.rs`,
`src/rust/adapters/parsing/ada/syntax.rs`,
`src/rust/adapters/parsing/ada/aunit.rs`, ADR-0033, ADR-0045,
product/incremental tests, and `5e8fda053122fb0cfd093b28767b9479bd7ddc80`. Exact
non-claim: a parsed registration is not execution, ordering, suite collection,
or proof that the registered routine exists, and Alire inventory is not
compilation, package selection, legality, symbol identity, or Ada support.
