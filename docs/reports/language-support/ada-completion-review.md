# Ada language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020, ADR-0030, and ADR-0033
- Dependency prerequisite: `5e8fda053122fb0cfd093b28767b9479bd7ddc80`
- Last updated: 2026-08-01

## ADR-0020 gate

- [x] Conservative discovery/config and dependency inventory — GNAT-default
  `.ads`/`.adb`, exact GPR/Alire config, bounded unconditional Alire manifest
  declarations, typed conditional/pin/lock/malformed/conflict/resource UNKNOWN,
  persistence, incremental replacement/removal, and no execution.
- [ ] Selected project model and authoritative source frontend.
- [ ] RepoGrammar-owned Ada code units and IR.
- [ ] Complete Ada semantic-obligation/claim-impact registry and provider fallback.
- [ ] One exact Ada family with support at least three.
- [ ] Positive, lookalike, low-support, parse-degraded, conditional, generic,
  overload, dispatch, and resolved/unresolved fixtures.
- [ ] Source-free readiness and leakage review for claim-bearing analysis.
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

Not complete. Ada remains `discovered_only`; dependency presence is not source,
library, framework, family, or readiness support. No supported-language count or
completion percentage may include Ada until every open checkbox has linked
current-branch evidence.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | Ada / 16 |
| Dialect/version | GNAT-default lowercase `.ads`/`.adb` inventory only; no Ada edition, GNAT profile, target, scenario variable, GPR project, or alternative naming selection. |
| Provider/frontend/version | None. Libadalang/GNAT project-provider admission is `NO_GO` for this supplied-bytes lane; no version is active. |
| Manifest/lockfile | Bounded unconditional direct Alire string requirements; pins/conditional tables/internal lock schema do not resolve dependencies. |
| Owned source IR / external symbols | Both absent; config units are not Ada source IR and package/generic/overload identities are unresolved. |
| Library Contracts | Registry exists, production packs = 0; Alire declarations cannot establish behavior. |
| Exact family / fixtures | No family. Strong static manifest, adversarial, resource, leakage, and incremental tests; no semantic family corpus. |
| Primary UNKNOWN cases | GPR selection, naming, edition/toolchain/target, conditional Alire data, pins, generic instantiation, overload/dispatch, generated code, and provider availability. |
| Source-free / security | Source/GPR is zero-read; config parsing is bounded; no GNAT, Libadalang, gprbuild, alr, child, repository/dependency code, or network runs. |
| Completion state / counted | `discovered_only`; strict gate count `2/9`; Top-20 complete = no. |

Four-part review: correctness covers only unconditional literal declarations;
security preserves the zero-read/non-execution boundary; completeness lacks a
selected project/frontend/IR/family; performance is bounded for static Alire
metadata but has no project-provider or large-codebase measurement. Evidence:
`src/rust/adapters/languages/ada.rs`, `src/rust/adapters/parsing/ada.rs`,
ADR-0033, product/incremental tests, and
`5e8fda053122fb0cfd093b28767b9479bd7ddc80`. Exact non-claim: Alire
inventory is not compilation, package selection, legality, symbol identity, or
Ada support.
