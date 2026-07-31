# Ada language completion review

- Status: Incomplete — `discovered_only`
- Authority: ADR-0020, ADR-0030, and ADR-0033
- Reviewed integration baseline: `abda602fe38db8549e4f318bd4638ddb94df7282`
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
- [ ] Correctness, security, completeness, and performance review.
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
