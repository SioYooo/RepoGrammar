# Source-free, security, correctness, completeness, and performance review

Date: 2026-08-01. Branch: `feat/top-20-language-library-support`.
Verdict: `PARTIAL_AUDITED_PROGRESS`.

## Source-free evidence

The program separates internal evidence from public output. Internally,
dependency rows may retain bounded package text and repository-relative evidence
paths. Public indexing/status/readiness/family surfaces must not echo source,
absolute paths, credentials, manifest bodies, remote URLs, archive names,
symbols, or package identities unless a separately reviewed contract explicitly
permits them.

Evidence exercised by committed tests includes:

- binary or sentinel-marked source for Go, PHP, Ruby, Swift, VB.NET, Object
  Pascal, Ada, Fortran, SQL, R, and MATLAB does not enter a forbidden source
  parser;
- Composer remote URLs, R remote-source credentials, vcpkg/Conan unsupported
  values, MATLAB package names/UUIDs, Maven coordinates, VB/Delphi package names,
  and Assembly labels/include text are absent from ordinary JSON/status output;
- malformed, duplicate, conflicting, resource-exhausted, archive, and provider
  failures use bounded semantic classes rather than raw input text;
- family/read-plan/MCP surfaces use evidence ranges, hashes, ids, counts, and
  low-cardinality tokens instead of source snippets by default;
- the Scratch preflight debug result exposes aggregate counts and error enums,
  not `project.json` opcode strings or archive entry names.

Source-free is complete only for the implemented slices. It is not a blanket
proof for future compiler diagnostics, package-manager output, archives,
catalogs, generated code, or third-party provider SDKs.

## Security review

### Positive boundaries

- Repository paths are normalized, repository-relative, symlink-aware, hash
  checked, size bounded, and subject to aggregate discovery budgets.
- JSON readers reject duplicate members and bound depth/member/key counts. XML
  readers reject DTD/external/custom entities and bound depth/token/field counts.
- Static package/config readers consume supplied bytes only. They do not run
  package managers, build systems, plugins, generators, compilers, tests,
  repository code, dependency code, child processes, or network resolution.
- Optional process providers enforce request/output/time/version/provenance
  contracts where implemented. Provider failure never upgrades a structural
  candidate.
- Scratch archive work is disconnected from product discovery. The prerequisite
  bounds archive/entry/uncompressed sizes, compression ratio, paths, duplicates,
  symlinks, encryption, JSON structure, targets, blocks, opcodes, and extensions;
  common deflated `project.json` is rejected rather than decompressed unsafely.
- Contract registration is bounded to 4,096 contracts and 64 exact versions per
  contract, rejects duplicate ids and overlapping package/version/capability
  claims, and refuses manifest-only or versionless lookup.

### Residual risks

| Severity | Risk | Current disposition |
|---|---|---|
| High | Native/compiler providers could read filesystem/cache/credentials, spawn descendants, allocate unbounded memory, or execute plugins/project code | no new provider is admitted until language-specific OS sandbox, artifact, version, license, timeout, output, and provenance gates pass |
| High | Scratch deflate/archive integration could introduce bombs, traversal, links, or parser differentials | product integration is `NO_GO`; binary port and maintained ZIP/deflate qualification required first |
| High | Executable manifests/build DSLs can run arbitrary code | never executed for discovery; dynamic portions remain inventory-only/UNKNOWN |
| Medium | Package/version text may contain private identity | internal only under current storage/CLI contract; public projection remains incomplete and must be separately reviewed |
| Medium | Tree-sitter error recovery can create plausible partial trees | parse-degraded and blocking UNKNOWN policy; Tree-sitter alone cannot prove semantic support |
| Medium | Cross-provider disagreement could select a false identity | provider-specific conflict policy is incomplete; no behavior promotion is allowed |
| Low | Contract-registry construction is quadratic at its bounded maximum | 4,096-entry hard cap; registry is empty in production and built only from reviewed inputs; optimize before large pack admission |

## Correctness review

The strongest correctness property is evidence separation:

`manifest_declared < lockfile_resolved < provider_resolved`, with reviewed
contracts outside that dependency ladder. Directness is a three-state value.
Unknown ecosystems are rejected. Manifest presence never proves installed
state, an import spelling never proves package ownership, and a contract match
would still require exact source and symbol anchors.

Observed limitations are intentionally preserved:

- no Maven effective model, Gradle, C# project/NuGet lane, npm-family lockfile,
  Cargo lock, Go workspace graph, Ruby resolved-spec join, Swift manifest model,
  SQL extension model, Assembly native dependency model, or Scratch product
  inventory;
- only TypeScript has an integrated provider slot, and its default/fallback
  behavior is not a complete pinned project model; Python and rust-analyzer
  slots remain `not_integrated`;
- MATLAB and all N1/N2/N3 source lanes except Assembly remain source-free
  inventory rather than source semantics;
- Assembly facts are deliberately structural candidates with an unconditional
  target-profile UNKNOWN;
- the status JSON field named `dependency_records` currently counts the
  existing `derived_record_dependencies` status source rather than the
  authoritative active dependency read model. Product tests therefore verify
  MATLAB dependency count through `list_active_dependencies`; this pre-existing
  naming/count issue was not broadened into the language slice and remains a
  follow-up correctness risk.

## Completeness review

- 20 ranked language records plus TypeScript extra exist and each links a
  completion review.
- Strict completion is `0/20`; TypeScript extra is incomplete.
- Ecosystem tokens = 20; tokens with at least one generic dependency consumer =
  17. `sql_extension`, `scratch_extension`, and `native_system` have none.
- Production reviewed library contract packs = 0.
- No language satisfies all nine ADR-0020 gates. No row may be reported as
  `bounded_preview` or `provider_backed` under the strict Top-20 program merely
  because a narrower historical product label uses “preview” or “supported.”

## Performance and resource review

The implemented work favors deterministic bounded passes:

- discovery and source/config reads have per-file and aggregate limits;
- JSON/XML/TOML/DCF/line parsers cap bytes, depth, members/tokens, fields, lines,
  dependencies, facts, and generated units;
- Assembly caps source bytes, lines, line bytes, labels, facts, and reserved
  UNKNOWN capacity;
- Scratch caps archive bytes, entries, total/per-entry uncompressed bytes,
  compression ratio, path bytes, JSON bytes/nodes/depth/members, targets,
  blocks, opcodes, and extensions;
- dependency snapshots sort deterministically and reject duplicates; registry
  overlap checking is bounded even though it is quadratic in contract count;
- incremental indexing copies only unchanged evidence-bound rows and removes or
  replaces changed/removed path records.

Targeted parser tests and full workspace tests demonstrate termination under
the committed synthetic exact/+1 cases. They are not cross-machine benchmarks.
There is no representative compiler/provider memory/latency benchmark, large
multi-project package graph benchmark, archive deflate benchmark, database
catalog benchmark, or production corpus percentile report. Those missing
measurements block provider and language completion claims.

## Review conclusion

The changes are safe and useful as a bounded inventory/structural foundation.
They are not complete language support and not complete arbitrary-library
behavior analysis. The only acceptable strong label for this branch is
`PARTIAL_AUDITED_PROGRESS`.
