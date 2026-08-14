# Python language completion review

- Language: Python
- Frozen Top-20 rank: 1
- Lane: C0 current-language convergence
- Audited integration baseline: `8a4033e3eddc93787c997d13c54bba22b1df7102`
- Completion state: **Incomplete — `structural_substrate` under ADR-0020**
- Counted in Top-20 denominator: **yes**
- Counted as Top-20 complete: **no**
- Last reviewed: 2026-08-01

This review applies ADR-0020's stricter language-completion contract. It does
not revoke the narrower public Python v0.1 family claims in the product
specification, but those claims do not grandfather Python through the Top-20
gate. The current implementation is substantial, reusable evidence; it is not a
9/9 completion chain.

## Scope and dialect boundary

The implemented source scope is exact lowercase `.py` files interpreted by the
available Python 3 worker. Root `pyproject.toml`, `setup.cfg`, and `setup.py`
are the bounded project-configuration inputs. The Python language version the
host interpreter implements is provenance-significant because AST shapes differ
by Python release; this review does not claim one universal
Python-language-version model. The worker uses the CPython standard-library
`ast`/`symtable` API, but the reported syntax boundary is read from
`sys.version_info` and therefore names a language version only — the product
never asserts that the host implementation is CPython rather than another
conforming implementation. Stub files, compiled extensions, notebooks, generated
code, runtime import hooks, arbitrary packaging frontends, and
environment-specific import behavior are outside the completed scope.

The official v0.1 framework focus remains FastAPI, pytest, SQLAlchemy, and
Pydantic. Django, Flask, unittest, click/typer, Celery, and marshmallow are
separate bounded preview slices and do not widen this completion decision.

## Current implementation evidence

### Discovery, frontend, owned IR, and project model

- Discovery recognizes `.py` and the three exact root config names, applies
  global source limits and symlink containment, and skips documented Python
  cache, virtual-environment, and dependency directories.
- `src/workers/python/worker.py` uses CPython `ast`, `symtable`, `tomllib`, and
  `configparser`; `setup.py` is parsed as syntax and is never executed.
- The private parse-document contract is pinned to `protocol_version=1` and
  `contract_revision=2`. Requests, responses, facts, ranges, hashes, source
  context, output bytes, fact counts, text fields, and wall time are bounded.
- `src/rust/adapters/parsing/python.rs` validates the worker envelope and
  translates results into RepoGrammar-owned `CodeUnit`, IR, semantic fact,
  evidence, provenance, and dependency types. Python objects do not enter core
  or storage.
- The bounded project model includes safe source roots, a repo-local module
  graph, static package re-exports, literal `__all__`, pytest `conftest.py`
  hierarchy, config hashes, and a module-interface hash used by incremental
  synchronization. It is not a complete interpreter import graph.

### Package metadata and third-party-library analysis

ADR-0030 inventory exists for normalized PyPI declarations from bounded PEP
621/build-system/dependency-group arrays, `setup.cfg` requirement sections, and
literal static `setup.py` dependency fields. It records `manifest_declared`
evidence with same-generation source provenance and rejects or abstains on
dynamic, malformed, non-authoritative, over-budget, URL, and path requirement
shapes. Inventory proves neither installation nor import resolution.

Third-party behavior remains incomplete. `python_type_provider` is registered
but `not_integrated`; `src/rust/ports/python_provider.rs` and the application
planner define future Pyrefly/Pyright/RightTyper requests and cache/provenance
types but execute no provider. The port does now own the whole abstention
decision: `classify_python_provider_answer` is the single entrypoint that reads
a recorded answer and returns `absent`, `stale`, `conflicting`, or nothing, and
`PythonProviderOutput::abstained` turns each state into a typed `UNKNOWN` with
its own reason code, class, and recovery while carrying zero facts and no
provenance. Freshness is decided against the content hash each candidate had
when the provider saw it, so no caller may rederive abstention from raw
provenance, hash, or fact fields. That is a contract, not an integration: no
provider produces an answer to classify. Therefore arbitrary import spellings are not
package-qualified external symbols and no provider-backed dependency graph
exists. The exact-version contract registry from `4e4d0de` has zero production
contract packs, so it cannot establish behavior.
Under ADR-0030, package presence and framework-name text are context only and
must never support a family by themselves.

### Exact families and typed `UNKNOWN`

Python has exact-anchor family substrate for FastAPI, pytest, Pydantic, and
SQLAlchemy with support at least three, complete-link compatibility, canonical
framework targets, same-generation evidence, and blockers for claim-relevant
parser `UNKNOWN`s. Strong support is the owned
`repogrammar-python-derived`/`bounded_ast_anchor_v1` path; raw structural or
heuristic roles remain insufficient. Context such as response models,
dependency calls, pytest fixture edges, Pydantic members, and SQLAlchemy
relationships is deliberately excluded from membership support where specified.

The implementation emits claim-scoped `UNKNOWN`s for dynamic imports and calls,
`sys.path` mutation, unresolved imports, decorator factories, monkey patching,
pytest fixture ambiguity/injection, external framework bases, dynamic model
factories, stale/conflicting evidence, resource limits, missing providers, and
insufficient support. The shared governance distinguishes claim impact from
recoverability. However, the production provider lane needed to discharge
recoverable type/import obligations is absent, and the complete parse-degraded
fixture audit required by ADR-0020 has not landed.

## Four-part review

### Correctness and bug findings

The current slice has strong regression coverage for exact imports/aliases,
shadowing and reassignment, source-position visibility, module/re-export and
pytest fixture resolution, support thresholds, complete-link clustering,
incremental interface hashing, stale evidence, and context-only anchors. The
historical implementation has received multiple focused hardening commits after
its original aggregate landing.

Open correctness gaps are completion-blocking: no Pyrefly/Pyright facts are
produced in the product path; Tree-sitter fallback described by the architecture
is not implemented; cross-function call hierarchy and runtime-observed facts are
absent; arbitrary packaging-tool semantics and provider disagreement are not
closed by product fixtures; and a syntactically degraded source corpus has not
been audited end-to-end against family non-formation. These gaps must stay
`UNKNOWN`, not be filled by AST heuristics.

### Security and untrusted-input handling

Positive evidence includes repo-relative path and symlink validation, strict
content hashes/ranges, a 1 MiB input envelope, 2 MiB output envelope, 2,000-fact
limit, bounded field lengths/assumptions, a 30-second timeout, concurrent stdout
draining, sanitized low-cardinality failures, aggregate source budgets, no
`setup.py` execution, and default source-free output. The worker converts
unexpected recursion/resource failures into typed worker failure rather than a
partial confident result.

Residual security work is provider-specific. Any Pyrefly/Pyright integration
must pin acquisition and version, isolate filesystem/network/child processes,
bound CPU/memory/output/time, prevent repository plugin/config execution, hash
inputs and configuration, sanitize diagnostics, and fail closed. No such
production adapter has passed review, so this review cannot approve third-party
semantic execution.

### Implementation completeness

Discovery, authoritative CPython syntax parsing, owned units/IR, bounded project
configuration, exact family substrate, and source-free product paths are real.
Completion is nevertheless blocked by partial discovery/config qualification,
the incomplete provider/UNKNOWN closure, missing completion fixture coverage,
this report's inability to substitute for implementation/tests, and the lack of
an ADR-0020 atomic prerequisite chain and final full-gate audit.

### Performance and resource bounds

The worker uses immutable AST-range caching and source-ordered binding histories
to avoid repeated scans, and the test policy includes a 40,000-import bounded
module, large response handling, request-context omission, timeout behavior,
and incremental interface-hash checks. Generic read/write benchmarks exist but
are not a Python completion benchmark. No current committed report demonstrates
representative multi-project Pyrefly/Pyright latency, memory, cache hit rate, or
worst-case package graph behavior because those providers do not exist. Provider
performance limits therefore remain an open gate, not an inferred success.

## ADR-0020 nine-gate checklist

| Gate | Result | Audited evidence and blocker |
|---|---|---|
| 1. Discovery/config | Partial | `.py` plus bounded root Python config, skips, size/symlink tests, and project inventory exist. The syntax-version boundary is now explicit rather than implicit: because the worker runs on the host interpreter, parseable grammar is capped by that interpreter's version, and a run that degrades a Python file reports the version that bounded it, or `UNKNOWN`. Dialect selection and wider packaging-profile qualification are still not closed as one completion module. |
| 2. Authoritative frontend | Pass for the implemented syntax slice | CPython `ast`/`symtable` is authoritative and bounded; this does not supply the missing type/import provider or implemented Tree-sitter fallback. |
| 3. Owned code units/IR | Pass | Worker output is validated and translated to owned units, IR, facts, evidence, provenance, and deterministic storage records. |
| 4. Typed `UNKNOWN` | Partial | Broad claim-scoped classes and recovery codes exist. The degraded parse path is no longer silent: an error diagnostic now emits a distinct file-level `parse degraded` token stating that missing code units are not evidence of absence, proven end to end against the real CPython frontend. Provider abstention is no longer an open vocabulary either: absent, stale, and conflicting are the port's whole answer space, each mapping to a distinct reason code and class, with conflicting typed irreducible because no registered mechanism adjudicates a provider's contradiction with itself. Pyrefly/Pyright remains `not_integrated`, so no product path exercises those states yet. |
| 5. Family-first exact anchor | Pass as substrate | Multiple exact families meet support >= 3 and compatibility rules; this is reusable evidence, not completion without the other gates. |
| 6. Fixture proof | Partial | Positive, lookalike/dynamic, low-support, stale, conflict, and selected unresolved/resolved fixtures exist. The parse-degraded case is now covered end to end: an unparseable module is indexed, yields zero units, and is reported as degraded rather than as a clean empty parse. The provider-state matrix now exists at the port boundary: absent, stale by changed hash, stale by deleted candidate, conflicting by two targets for one single-valued subject, conflicting by a provider's own `CONFLICTING` certainty, the additive operations that must not read many targets as a conflict, and the stale-outranks-conflict precedence are each pinned by a fixture. Closure still needs the same matrix on a product path, which requires a provider that answers. |
| 7. Source-free readiness | Partial | CLI/MCP/query/readiness and leakage controls exist, but the language's final provider/readiness matrix has not been audited and linked as a completion submodule. |
| 8. Four-part review | Satisfied by this snapshot | This report records correctness, security, completeness, and performance findings. Open findings remain blockers or risks and are not converted into test or implementation evidence. |
| 9. Atomic delivery/audit | Fail | The main Python landing was an aggregate commit and no final audit links independently complete discovery, frontend/IR, provider/UNKNOWN, family/fixtures, and review commits with full gates. |

Result: **not 9/9; Python must not increment the ADR-0020 completed-language
count.**

## Evidence paths

- Authority: `docs/decisions/ADR-0020-top-20-language-expansion-gate.md`,
  `docs/decisions/ADR-0030-language-neutral-dependency-library-semantics.md`
- Specifications: `docs/specifications/python-analysis.md`,
  `docs/specifications/semantic-workers.md`,
  `docs/specifications/indexing-pipeline.md`,
  `docs/specifications/unknowns.md`, `docs/development/testing.md`
- Frontend/adapter: `src/workers/python/worker.py`,
  `src/workers/python/worker.test.py`,
  `src/rust/adapters/parsing/python.rs`
- Provider boundary: `src/rust/ports/python_provider.rs`,
  `src/rust/core/model/provider.rs`, `src/rust/application/providers.rs`
- Family/owned pipeline: `src/rust/application/indexing.rs`,
  `src/rust/application/family.rs`, `src/rust/adapters/frameworks/mod.rs`
- Fixtures: `src/fixtures/python/release/v0_1/`,
  `src/fixtures/python/release/v0_2/`,
  `src/fixtures/unknown_reduction/python_fastapi_unresolved/`,
  `src/fixtures/unknown_reduction/python_fastapi_resolved/`

## Evidence-bearing historical SHAs

These commits are ancestors of the audited baseline and identify current
substrate. They are **not** asserted to be the independently complete D2/G9
prerequisite chain:

- `6561987ef2a01d5b920315a9afe97be92e57853b` — indexing, query, storage,
  lifecycle, discovery, and shared substrate.
- `e275c9b2a3d9ba1984ba2e344b0ad9e826abc973` — aggregate Python v0.1 frontend,
  families, fixtures, and product-path landing.
- `9ea396b96ae11eb79ec9f80dd52b66e631c7dc1a` — language-neutral dependency
  evidence model and ADR-0030 authority.
- `5ef354249b58784b1e872aa6ac3d83c7502dd4c8` — dependency persistence.
- `afc472bc2bf23abe63c24e1a501f1bc354b5b24e` — bounded Python dependency
  inventory.

## Required next closure sequence

1. Pin and review a candidate-scoped Pyrefly provider plus selective Pyright
   cross-check contract, including acquisition, isolation, provenance, cache,
   timeout, conflict, and source-free failure behavior.
2. Add package-qualified external-symbol resolution and versioned reviewed
   library-contract inputs without promoting manifest presence.
3. Add provider absent/present/stale/conflicting and controlled
   unresolved-to-resolved product fixtures; prove negative/degraded cases stay
   family-free. Two halves of this step are done. The parse-degraded half: an
   error diagnostic now carries a distinct file-level degraded token instead of
   sharing the recoverable-diagnostic wording, with synthetic and real-frontend
   coverage. The abstention half: the port classifies absent, stale, and
   conflicting answers into typed `UNKNOWN`s and a fixture matrix pins every
   state, its precedence, and the additive operations that are not conflicts.
   What remains is the `present` leg and the product path, both of which need an
   executing provider under a separate ADR-0020 D3 review.
4. Re-audit one exact family end-to-end, including leakage and representative
   resource measurements.
5. Deliver each completed submodule as its own Conventional Commit, then run all
   required gates and link exact SHAs in a final completion-audit commit.

## Risks and non-claims

- This review does not claim sound or complete Python semantics, a whole-program
  call graph, runtime dependency injection, dynamic import resolution, or safe
  execution of target Python code.
- PyPI inventory does not prove a dependency is installed, selected, imported,
  compatible, or behaviorally understood.
- The frontend cannot parse Python grammar newer than the host interpreter,
  because the worker is a script that interpreter executes. On an older host,
  valid modern syntax such as `match`, `except*`, and PEP 695 generics is a
  plain syntax error and those files degrade to zero code units. The run now
  reports the bounding version, but no minimum version is defined or enforced,
  and Python coverage on a given machine is therefore not a fixed property of
  the release.
- Test-only injected semantic facts are not Pyrefly/Pyright support.
- Framework exact anchors are bounded source-visible evidence, not universal
  framework coverage.
- A lower `UNKNOWN` count is not progress unless the same obligation is
  discharged by fresh source-backed evidence.
- Existing product wording such as Python v0.1 `Supported` is narrower than the
  ADR-0020 program label; this report must not be used to claim Top-20 Python
  completion.

## Completion verdict

**Incomplete — `structural_substrate`; Top-20 counted = yes; Top-20 complete =
no.** Python has the strongest current language path, but it lacks the provider,
fixture, review-closeout, and atomic-audit evidence required for `bounded_preview`
under ADR-0020.
