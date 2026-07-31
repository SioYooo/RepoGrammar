# Fortran language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020, ADR-0030, and ADR-0034
- Dependency prerequisite: `5e8fda053122fb0cfd093b28767b9479bd7ddc80`
- Last updated: 2026-08-01

## ADR-0020 gate

- [x] Conservative discovery/config and dependency inventory — frozen lowercase
  non-preprocessed fixed/free forms, exact `fpm.toml`, bounded root dependency
  strings, typed unsupported/conflict/malformed/resource UNKNOWN, persistence,
  incremental replacement/removal, and no execution.
- [ ] Selected dialect/source-form/project model and authoritative frontend.
- [ ] RepoGrammar-owned Fortran code units and IR.
- [ ] Complete Fortran semantic-obligation/claim-impact registry and provider fallback.
- [ ] One exact Fortran family with support at least three.
- [ ] Positive, lookalike, low-support, parse-degraded, preprocessed, include,
  module/submodule, interface, generic, and resolved/unresolved fixtures.
- [ ] Source-free readiness and leakage review for claim-bearing analysis.
- [x] Four-part review record — this report records correctness, security,
  completeness, and performance findings; open findings remain blockers.
- [ ] Linked atomic prerequisite commits and final completion audit.

## Current evidence and blocker

Discovery and indexing now inventory only the documented lowercase
non-preprocessed GNU suffix set. Source bytes never cross SourceStore, including
non-UTF-8 inputs. Exact supplied `fpm.toml` bytes may yield direct runtime or
development `fpm` rows from simple root-table strings. Dotted namespace,
inline git/path, target-specific, conflicting, malformed, and resource-bounded
shapes remain fixed source-free `fortran_dependency_inventory` UNKNOWN. Product
and incremental tests prove CLI mode, zero source reads, dependency copy-
forward/replacement/removal, path/URL non-leakage, and zero family output.

This is dependency infrastructure, not Fortran support. Uppercase/preprocessed
forms, `.fpp`, `.fi`, `.fii`, compiler flags, includes, dialects, source-form
selection, source IR, obligation registry, exact family, readiness promotion,
and semantic fixtures remain absent.

Flang's Round-4 result is `NO_GO` for production admission: its documented
prescanner expands includes and performs preprocessing, while no pinned
artifact/dependency/sandbox/differential matrix was qualified. No Flang/f18,
fpm, compiler, or preprocessor dependency/process was introduced. A future
proposal must requalify an isolated supplied-bytes frontend from first principles.

## Completion verdict

Not complete. Fortran remains `discovered_only`; dependency presence is not
source, library, framework, family, or readiness support. No supported-language
count or completion percentage may include Fortran until every open checkbox
has linked current-branch evidence.

## Final program audit fields

| Required field | Audited result |
|---|---|
| Language / rank | Fortran / 19 |
| Dialect/version | Frozen lowercase non-preprocessed GNU-style suffix inventory; no standard edition, compiler, fixed/free override, preprocessing, include, target, or build profile. |
| Provider/frontend/version | None. Flang/f18 is `NO_GO` for this zero-execution lane; no provider version is active. |
| Manifest/lockfile | Bounded root `[dependencies]`/`[dev-dependencies]` literal strings from `fpm.toml`; no graph, lock, git/path, target table, or resolved version. |
| Owned source IR / external symbols | Both absent; module/submodule/interface/generic identities are unresolved. |
| Library Contracts | Registry exists, production packs = 0; manifest rows cannot establish behavior. |
| Exact family / fixtures | No family. Strong static manifest/resource/leakage/incremental tests; no source-family matrix. |
| Primary UNKNOWN cases | Source form/standard/compiler, preprocessing/includes, module graph, conditional targets, generics/interfaces, ABI, native/system dependencies, and provider availability. |
| Source-free / security | Source is zero-read and metadata bounded; no Flang, fpm, compiler, preprocessor, child, repository/dependency code, or network runs. |
| Completion state / counted | `discovered_only`; strict gate count `2/9`; Top-20 complete = no. |

Four-part review: correctness retains only literal root-table declarations;
security avoids preprocessing/compiler execution; completeness lacks source
frontend/IR/obligations/family; performance is bounded for metadata but has no
frontend or large scientific-project benchmark. Evidence:
`src/rust/adapters/languages/fortran.rs`,
`src/rust/adapters/parsing/fortran.rs`, ADR-0034, product/incremental tests, and
`5e8fda053122fb0cfd093b28767b9479bd7ddc80`. Exact non-claim: fpm
inventory does not prove compilation, module linkage, numerical semantics,
dependency resolution, or Fortran support.
