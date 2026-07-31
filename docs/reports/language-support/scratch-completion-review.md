# Scratch language completion review

- Language/rank: Scratch, frozen Top-20 rank 12
- Status: Incomplete — `NO_GO` / `prerequisite_only`
- Authority: ADR-0020, ADR-0030, ADR-0039
- Branch/base: `feat/top20-matlab-assembly-scratch` from `9d3a0ba`
- Last updated: 2026-08-01
- Top-20 counted: no

## Capability record

| Dimension | Current evidence |
|---|---|
| Dialect/version | Scratch 3 `.sb3` target; no product format parser integrated. |
| Provider/frontend | Pure ZIP/JSON security preflight only. No Scratch VM/parser/provider. |
| Provider version | RepoGrammar crate version; Scratch VM repository is a format-shape reference only. |
| Discovery/config | None. `.sb3` remains `unsupported_extension` intentionally. |
| Manifest/lockfile | None. Declared extension count is inspectable only in the non-integrated stored-JSON preflight. |
| Owned IR | None. Security result is bounded aggregate metadata, not a Scratch code model. |
| External symbols | None. |
| Library contracts | None; extension presence proves no opcode/runtime behavior. |
| Exact-anchor family | None; event-hat/opcode-stack and broadcast-handler families absent. |
| Fixtures/tests | Programmatic stored-ZIP real-shape project, traversal/absolute/drive/backslash, duplicate, Unix symlink, ratio bomb, size, deflate, duplicate/deep/unsupported JSON, malformed/missing project tests. |
| Typed failures | Explicit archive/JSON error enum including unsupported compression; not yet persisted as product UNKNOWN. |
| Source-free | Preflight returns only counts; tests reject opcode and asset names in debug output. |

## Four-part review

### Correctness

The central-directory preflight validates classic single-disk non-ZIP64 records,
local header/name/method coherence for stored `project.json`, exact single root
project JSON, and bounded target/block/opcode shapes. It is not a general ZIP
reader: method-8 deflate is rejected, CRC/data descriptors are not qualified as
a full extraction contract, and no Scratch execution semantics are inferred.

### Security

The prerequisite enforces 64-MiB archive, 4,096 entry, 64-MiB per-entry,
256-MiB aggregate-uncompressed, 200:1 ratio, 512-byte path, 8-MiB project JSON,
depth 128, 200,000 member/node, 4,096 target, 100,000 block, and 256 extension
limits. It rejects encryption, unsafe paths, duplicates, symlinks, ZIP64/multi-
disk, unsupported methods, invalid UTF-8/JSON, and malformed shapes. It never
extracts to disk, runs VM/extensions, or downloads assets.

### Completeness

The product port is UTF-8 text-only and has no reviewed deflate dependency, so
the safe result is NO-GO. There is no `.sb3` discovery, binary source port,
deflate/CRC qualification, asset index, sprite/target/block/broadcast owned IR,
extension/opcode identity, family, persistence, CLI/MCP readiness, or support.

### Performance

Archive parsing is bounded by the explicit ceilings above and uses one central-
directory pass plus an iterative JSON-node count. No decompression occurs. No
cross-machine timing/peak-memory claim is made; targeted tests complete in well
under one second after compilation on the recorded host.

## ADR-0020 nine-gate checklist

- [ ] 1. Discovery/configuration — deliberately absent due binary-port blocker.
- [ ] 2. Authoritative format parser — security preflight is not a full `.sb3` parser.
- [ ] 3. Owned code units/IR — absent.
- [ ] 4. Typed UNKNOWN — local error enum only, not product claim-scoped UNKNOWN.
- [ ] 5. Exact-anchor family — absent.
- [ ] 6. Fixture proof — adversarial unit inputs exist, no product/family fixtures.
- [ ] 7. Source-free readiness — preflight debug result is source-free, no product path.
- [x] 8. Four-part review — this record.
- [ ] 9. Atomic completion audit — prerequisite/NO-GO slice only; obtain exact
  SHA from file history after commit.

## Completion verdict and exact non-claims

`NO_GO` for current product integration and `PARTIAL_AUDITED_PROGRESS` for the
security prerequisite; strict gate count `1/9`; Top-20 counted `no`.
RepoGrammar does not discover, parse, index, execute, or support Scratch. A
stored-project JSON test is not evidence that common deflated `.sb3` archives
work, and aggregate opcode counts are not behavior semantics.

## Evidence paths and risks

- `src/rust/adapters/languages/scratch.rs`
- `src/rust/adapters/filesystem/discovery.rs` (negative `.sb3` assertion)
- `docs/decisions/ADR-0039-scratch-sb3-archive-no-go.md`

Highest risk: an unsafe future integration could bypass the binary-port and
deflate qualification. The single next action is a bounded binary-document port
plus a maintained ZIP/deflate security qualification before discovery changes.
