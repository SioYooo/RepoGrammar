# UNKNOWN Regression Benchmark

- Status: Active regression protocol
- Scope: persisted semantic `UNKNOWN` inventory for release fixtures
- Canonical taxonomy: `../specifications/unknowns.md`

## Purpose

The UNKNOWN regression benchmark tracks whether analyzer changes reduce,
reclassify, or accidentally hide typed `UNKNOWN` evidence. It is a diagnostic
benchmark for analyzer work, not a product-quality score by itself.

The benchmark exists because reducing `UNKNOWN` counts is only good when the
replacement evidence is source-backed and false certainty is controlled.
Analyzer slices must not pass the benchmark by collapsing uncertain facts into
families, dropping source-backed `UNKNOWN` facts, or returning source text in
inventory output.

## Current Product Test

The product regression test is:

```text
cargo test --workspace product_runtime_unknown_regression_benchmark_tracks_mechanisms_without_false_certainty
```

It copies committed release fixtures into temporary repositories, then runs the
real product flow:

```text
repogrammar init --json
repogrammar resync --json --progress never
repogrammar unknowns --json
repogrammar families --json
```

The test verifies:

- `unknowns --json` remains source-free and path-safe;
- `inventory_scope` is `persisted_semantic_unknowns`;
- language, reason-code, and required-mechanism buckets match the checked-in
  baseline;
- negative fixtures still return `families --json` status `UNKNOWN` with no
  family rows, preventing false-certainty regressions.

## Baseline Fixtures

| Case | Fixture | Purpose |
|---|---|---|
| Python dynamic unknowns | `src/fixtures/python/release/v0_1/dynamic-unknown` | Dynamic imports, monkey patching, FastAPI dependency uncertainty, pytest fixture ambiguity, and framework magic. |
| TS/JS framework negatives | `src/fixtures/typescript/release/v0_2/framework_adapter_negative_cases` | Fastify receiver gaps, Prisma/Drizzle dynamic or raw cases, path/import conflicts, and exact-anchor abstention. |
| Rust macro/cfg unknowns | `src/fixtures/rust/release/v0_2/macro_cfg_unknowns` | Cargo cfg/build-variant ambiguity, macro boundaries, and Rust self-dogfood false-certainty guards. |

The current TS/JS negative baseline expects the remaining import-resolution
fixture UNKNOWN to land in `typescript_module_resolver`. More specific buckets
such as `typescript_paths_resolver`, `typescript_rootdirs_model`,
`typescript_package_entry_model`, `typescript_commonjs_alias_model`, and
`typescript_export_graph` are covered by focused unit tests and should be added
to product fixture baselines only when a committed fixture exercises that
public product path.

## Updating The Baseline

When a later analyzer slice intentionally reduces or reclassifies an UNKNOWN:

1. Update the analyzer and its focused positive/negative tests first.
2. Prove the replacement fact is fresh, repo-relative, hash-backed, and
   compatible with the relevant family support gate.
   For TS/JS worker reductions, the replacement must also match the same
   path/hash/code-unit/range and requested operation provenance; static fallback
   facts with `provider_resolved=false` are context only.
3. Keep dynamic, ambiguous, stale, external, or unsupported behavior as typed
   `UNKNOWN`.
4. Re-run `repogrammar unknowns --json` over the benchmark fixture copy and
   update the exact expected buckets in the product test.
5. Confirm `families --json` does not become `ok` for a negative fixture unless
   that fixture is intentionally reclassified and has dedicated positive and
   false-positive coverage.
6. Update this document and `../specifications/unknowns.md` if a new mechanism,
   reason-code mapping, or recovery-code meaning is introduced.

Unknown-rate reductions must not be reported as quality improvements unless
false certainty is also measured or controlled.
