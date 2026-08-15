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
- [ ] Selected project model and authoritative source frontend. ADR-0045's
  scanner is bounded to one call shape and selects no project model, no GPR
  profile, and no Ada edition, so this gate stays open.
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
- [x] Positive, lookalike, low-support, and parse-degraded fixtures exist for
  that family. An unterminated string literal is a decidable well-formedness
  violation and reports a degraded parse. Conditional, generic, overload,
  dispatch, and resolved/unresolved fixtures do not exist: there is no Ada
  provider to resolve against.
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

This is dependency infrastructure, not Ada support. `.ada` and GPR-selected
alternative source names remain outside discovery; the product has no Ada
syntax/semantic IR, project selection, frontend, obligation registry, exact
family, readiness promotion, or semantic fixture corpus.

Libadalang's Round-4 result is `NO_GO` for production admission: its documented
GNAT coupling, project-provider file access, and incomplete legality coverage do
not satisfy the current supplied-bytes-only default boundary. No Libadalang,
GNAT, gprbuild, or alr dependency/process was introduced. A future proposal must
requalify a pinned isolated frontend from first principles.

## Completion verdict

Not complete. Ada has a bounded scanner over one call shape, owned units and
IR, a typed registration-binding `UNKNOWN`, and one exact family with support
three under ADR-0045. Alire dependency presence remains auxiliary and is not
source, library, framework, family, or readiness support. Strict gate count is
`7/9`; it must not be counted as a supported language.

Two limitations are stated rather than left to inference. This frontend is a
scanner, so malformed Ada does not fail the way a parsed language does; it
reports a degraded parse when its own well-formedness invariant is violated, and
outside that invariant fewer admitted calls remain indistinguishable from a body
with fewer calls. And the AUnit `with` clause must be in the same file: a
package body inherits its spec's context clause, so a body that uses AUnit
without naming it is a real shape this frontend does not admit. That false
negative is chosen over asserting an import it cannot see.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | Ada / 16 |
| Dialect/version | GNAT-default lowercase `.ads`/`.adb` inventory only; no Ada edition, GNAT profile, target, scenario variable, GPR project, or alternative naming selection. |
| Provider/frontend/version | None. Libadalang/GNAT project-provider admission is `NO_GO` for this supplied-bytes lane; no version is active. |
| Manifest/lockfile | Bounded unconditional direct Alire string requirements; pins/conditional tables/internal lock schema do not resolve dependencies. |
| Owned source IR / external symbols | Owned units and IR exist for the ADR-0045 anchor only; external symbols stay absent, and package, generic, overload, and dispatch identities are unresolved. The `'Access` prefix is read as syntax and never resolved to a routine. |
| Library Contracts | Registry exists, production packs = 0; Alire declarations cannot establish behavior. |
| Exact family / fixtures | One exact family, `framework:aunit.test_registration` over `aunit.Register_Routine`, gated at support three. Positive, lookalike, and low-support fixtures exist; parse-degraded, generic, overload, dispatch, and resolved/unresolved do not, because a scanner has no parse failure and there is no Ada provider. |
| Primary UNKNOWN cases | GPR selection, naming, edition/toolchain/target, conditional Alire data, pins, generic instantiation, overload/dispatch, generated code, and provider availability. |
| Source-free / security | Source/GPR is zero-read; config parsing is bounded; no GNAT, Libadalang, gprbuild, alr, child, repository/dependency code, or network runs. |
| Completion state / counted | `structural_substrate`; strict gate count `7/9`; Top-20 complete = no. |

Four-part review: correctness covers only unconditional literal declarations;
security preserves the zero-read/non-execution boundary; completeness lacks a
selected project/frontend/IR/family; performance is bounded for static Alire
metadata but has no project-provider or large-codebase measurement. Evidence:
`src/rust/adapters/languages/ada.rs`, `src/rust/adapters/parsing/ada.rs`,
ADR-0033, product/incremental tests, and
`5e8fda053122fb0cfd093b28767b9479bd7ddc80`. Exact non-claim: Alire
inventory is not compilation, package selection, legality, symbol identity, or
Ada support.
