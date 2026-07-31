# Fortran language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020, ADR-0030, and ADR-0034
- Reviewed integration baseline: `abda602fe38db8549e4f318bd4638ddb94df7282`
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
- [ ] Correctness, security, completeness, and performance review.
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
