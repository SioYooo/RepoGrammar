# Product-core baseline

## Purpose

This document records the Phase 0 product-core baseline: a committed,
deterministic evaluation harness plus a fixed query corpus that measure what the
current RepoGrammar product runtime actually returns for the pattern-family query
surface (`find`, `family`, `member`, `explain`, `check`). It is
test/measurement infrastructure only. It changes no production behavior; it
observes and records it.

The baseline exists so later product-core work has a falsifiable reference
point. Every recorded number here is behavior at the pinned commit, not a
target or a promise. Where the recorded behavior contradicts a product claim,
this document names the contradiction and stops there. It does not propose or
promise a fix.

## Components

- Query corpus: [`src/fixtures/evaluation/query-corpus-v1.json`](../../src/fixtures/evaluation/query-corpus-v1.json)
  (`schema_version: product-eval-corpus.v1`).
- Zero-family fixture: `src/fixtures/evaluation/zero-family-repo/` (two plain
  Python files with ordinary functions and no framework imports).
- Harness: `repo-guard product-eval` in `src/rust/bin/repo_guard.rs`.
- Machine-readable results: `product-eval-results.json`
  (`schema_version: product-eval-results.v1`).

The harness never touches the real repository. For each fixture it copies the
committed fixture root into an isolated temporary workspace (unique temp dir,
isolated `HOME`/XDG/`CODEX_HOME`, tool-only `PATH` with `git` and `python3`),
runs `init` then `resync`, applies any per-query source mutation to that copy,
and drives the product binary through the query. Temporary workspaces are
removed on success and retained (with the path printed to stderr) on a harness
error. Auto-sync is never enabled.

## Protocol

```text
cargo build --bin repogrammar --bin repo-guard
cargo run --quiet --bin repo-guard -- product-eval \
  --corpus src/fixtures/evaluation/query-corpus-v1.json \
  --out <output-dir> \
  --repetitions 3
```

- `--bin <path>` selects the product binary explicitly. When omitted the
  harness resolves the sibling `repogrammar` next to the running `repo-guard`
  executable (both build into the same target directory) and otherwise fails
  with a typed error asking for `--bin`.
- `--repetitions <n>` (default 3) runs each query `n` times to record wall
  latency; the parsed result fields are identical across repetitions.
- Exit is `0` when the run completes. Mismatches are baseline data, not
  failures. The harness exits nonzero only on genuine harness errors: a missing
  binary, a corpus that will not parse, a subprocess that will not execute, or
  query output that is not valid JSON.

## Corpus composition

The corpus has 26 queries over three fixtures: `python-v0_1`
(`src/fixtures/python/release/v0_1`), `typescript-v0_2`
(`src/fixtures/typescript/release/v0_2`), and `zero-family`. Every target was
calibrated by actually indexing the fixture and listing families; no target was
invented. Family ids in an isolated fixture index can carry cluster suffixes
that depend on peers, so member/path/role/question queries constrain a
`family_prefix` (or `family_any_of`) rather than an exact cluster id.

| Kind | Count | Gold intent |
| --- | --- | --- |
| `exact_family_id` | 3 | `family` op resolves the exact id → `ok` |
| `exact_member_id` | 2 | `member` op resolves the code-unit id → `ok` |
| `exact_path` | 4 | single-family repo-relative path → `ok` |
| `path_suffix` | 1 | non-full-path suffix → correct abstention (`unknown`) |
| `framework_role` | 2 | unambiguous role token → `ok` |
| `nl_pattern_question` | 6 | retrieval intent (5 name a resolvable family → `ok`; 1 generic → `unknown`) |
| `ambiguous` | 1 | `"handler"` → correct abstention (`unknown`) |
| `unsupported_concept` | 1 | `"How are React hooks memoized?"` → correct abstention (`unknown`) |
| `stale_family` | 1 | source mutated after indexing → `unknown` / `StaleEvidence` |
| `partial_context_path` | 1 | uniquely resolved multi-family path → `partial_context` |
| `check_conformance` | 2 | advisory `check` context success / partial context |
| `explain_deviation` | 1 | `explain` over a multi-family path → `partial_context` |
| `zero_family_repo` | 1 | resolved path in a no-family repo → `partial_context` |

Gold calibration discipline: for retrieval-intent natural-language questions the
gold is the product intent, not the current output. Five of the six NL
questions name a concept that resolves to exactly one indexed family
(`family:python:fastapi_route`, `family:python:pytest_fixture`,
`family:python:sqlalchemy_repository_method`, `family:python:pydantic_model`),
so their gold is `ok` with that family prefix even though the current product
returns `UNKNOWN`. That gap is the point of the baseline and is recorded as a
mismatch, not softened to match current behavior. The one generic question
(`"How are API routes implemented?"`) has gold `unknown` because abstention on a
genuinely generic question is correct.

## Outcome classification

The harness maps a product query `status` string onto a coarse retrieval
outcome:

| Product `status` | Recorded `outcome` |
| --- | --- |
| `ok`, `CONTEXT_ONLY` | `ok` |
| `PARTIAL_CONTEXT` | `partial_context` |
| `UNKNOWN` | `unknown` |
| (unrecognized) | `fallback` |

`CONTEXT_ONLY` is the `check` operation's context-success status: a single
family was discovered and hydrated (route `discover_hydrate_compose`), so it is
`ok` on the retrieval axis. The advisory conformance verdict stays `UNKNOWN`
by design and is a separate axis this coarse outcome does not encode; the raw
`route` is recorded on every result for disambiguation.

## Result schema field reference

`product-eval-results.json` (`schema_version: product-eval-results.v1`):

- `repogrammar_commit`, `platform {os, arch}`, `corpus_schema_version`,
  `corpus_path` (repo-relative), `repetitions`, `started_at`/`finished_at`
  (RFC3339 UTC).
- `fixtures[]`: `fixture_id`, `fixture_version` (SHA-256 over sorted
  relative-path plus content of the fixture root), `resync_latency_ms`,
  `discovered_files`, `stored_files`.
- `results[]`: `query_id`, `fixture_id`, `kind`, `operation`, `target`,
  `expected`, `actual`, `match`, `mismatch_fields[]`, `latency_ms_all_reps[]`,
  `latency_ms_p50`. `actual` carries `outcome`, `route`, `selected_family`,
  `candidate_family_count`, `candidate_families`, `unknown_reason`, and
  `active_generation`.
- `summary`: `total`, `matches`, `mismatches`, `by_kind{kind:{total,matches}}`,
  `latency_ms_p50`, `latency_ms_p95`, and `false_family_selections` (queries
  where a family was selected but the query's family/prefix constraint excludes
  it — a false-positive family selection).

Matching treats an absent expected field as unconstrained. `family_prefix` and
`family_any_of` match by prefix on the selected family id; `family` matches
exactly; `outcome`, `unknown_reason`, and `route` match exactly when present.

## Baseline results

### Harness run

Machine-dependent, recorded from a real local run at commit
`33715e4a6a23c100d96446007550fdacebb1f340`, `platform os=macos arch=aarch64`, dev
build, three repetitions:

- 26 queries, 21 match, 5 mismatch, `false_family_selections: 0`.
- Aggregate latency `p50 44 ms`, `p95 79 ms` (wall, dev build; not a
  performance claim).
- Per-fixture full `resync`: `python-v0_1` 34 files, ~3.3 s; `typescript-v0_2`
  59 files, ~1.1 s; `zero-family` 2 files, ~0.17 s.
- Fixture versions (SHA-256 prefix): `python-v0_1` `37fec96f7c7b…`,
  `typescript-v0_2` `b8d116a702d9…`, `zero-family` `242c9589f4fe…`.

By-kind matches: every kind is a full match except `nl_pattern_question`
(1/6). The five `nl_pattern_question` mismatches are `py-nl-fastapi-routes`,
`py-nl-test-fixtures`, `py-nl-db-transactions`, `py-nl-validation-models`, and
`py-nl-repository-methods`. Each has `mismatch_fields = [outcome,
family_prefix]`: the gold is `ok` with a resolvable family prefix, but the
product returns `UNKNOWN` with no selected family. The corresponding families
exist in the same index and are directly retrievable by exact family id, exact
member id, exact path, and framework role (all recorded as matches). This is
the **natural-language interface gap**: the product's `find` surface advertises
a "pattern question" input, but retrieval-intent questions over families that
demonstrably exist abstain rather than resolve. `false_family_selections: 0`
confirms the product does not compensate by mis-selecting a wrong family; it
abstains.

The `stale_family` query confirms the freshness contract on the exact-lookup
path: after appending one line to an indexed source file, `family` on the
affected family returns `outcome unknown`, `route exact_lookup_unknown`,
`unknown_reason StaleEvidence`.

### Coordinator-measured self-dogfood facts

The following were measured by the Coordinator on 2026-07-18 on macOS arm64,
single repetition, dev build. They are machine-dependent and are recorded here
as observed behavior at this checkpoint.

- All 11 existing quality gates pass at `33715e4` (fmt; clippy; workspace tests;
  TypeScript worker; npm launcher; `npm pack` dry-run; Python worker; installer
  test; `repo-guard check`; `check-diff`; `git diff --check`).
- Self-dogfood index: 344 files, 18085 units, 27074 semantic facts, 378
  families — every one classified `DOMINANT_PATTERN` (support distribution min
  2, max 337; 25 families at support 2, 109 at support 3). The classification
  is hard-coded; there is no prevalence model. This contradicts any reading of
  `DOMINANT_PATTERN` as an earned, prevalence-weighted verdict: with every
  family labelled dominant, the label carries no discriminating information.
- `families --json` performs no freshness check and returned `status ok` with
  zero freshness fields while exact hydration on the same state returned
  `StaleEvidence`. This is an **inconsistent trust surface**: the family
  listing and the exact-lookup hydration disagree about whether the same
  generation is trustworthy.
- Full `resync`: 221 s wall. No-op `sync`: 251 s wall (`sync_mode`
  incremental, 0 reparsed, 344/344 files copied forward, 378 families
  recomputed). The incremental no-op costs more than a full rebuild on the
  self-dogfood repository.
- MCP probes at stale generation `gen-000544`: `find`/`show`/`check` all
  abstain `StaleEvidence` with recovery `run repogrammar resync`. `"handler"`
  (ambiguous), `"How are React hooks memoized?"` (unsupported), and all five NL
  pattern questions abstain `UNKNOWN`/`InsufficientSupport` with recovery `use
  source fallback`. Ambiguity, unsupported concepts, and missing evidence are
  not distinguished by the reason code.
- MCP probes at fresh generation `gen-000582`: exact role
  `framework:fastapi.route` → `ok`, selected
  `family:python:fastapi_route:framework_fastapi_route` support 30; exact-path
  `find` on `src/rust/application/recovery.rs` → `ok` but selected a
  `rust_test_function` family with 123 members inlined into a 75857-byte
  response (unbounded member inlining); `"How are API routes implemented?"`
  still `UNKNOWN`/`InsufficientSupport`; `check_conformance` on a fixture path →
  `PARTIAL_CONTEXT` with advisory check (`advisory_status UNKNOWN`, "runtime
  equivalence remains unproven").
- Historical local telemetry rollup (`family-query-outcomes.v1`, 184 events):
  `found` 2, `partial_context` 18, `unknown` 145, `fallback` 19; reason
  `InsufficientSupport` 160. Abstention dominates the recorded query history.

### Recorded contradictions

Recorded here as baseline gaps, with no remediation implied by this document:

1. **Dominance without prevalence.** Every self-dogfood family is
   `DOMINANT_PATTERN` from a hard-coded classification; the label does not
   discriminate.
2. **Freshness-less family listing.** `families --json` reports `ok` with no
   freshness fields while exact hydration on the same generation reports
   `StaleEvidence`.
3. **Natural-language interface gap.** Retrieval-intent questions over families
   that exist and are exactly retrievable abstain `UNKNOWN`; the harness records
   5 such mismatches out of 6 NL questions, and the Coordinator self-dogfood and
   telemetry observations agree.

## Reproducing

Run the protocol above against any output directory and compare
`product-eval-results.json`. Latency numbers are machine-dependent; verdicts,
`by_kind` counts, `false_family_selections`, and per-query `outcome`/`route`
fields are stable for the pinned corpus and product commit. See
[`docs/development/testing.md`](../development/testing.md) for how the harness
fits the local gate.
