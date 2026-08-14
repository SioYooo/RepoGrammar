# MATLAB language completion review

- Language/rank: MATLAB, frozen Top-20 rank 18
- Status: Incomplete — `structural_substrate`
- Authority: ADR-0020, ADR-0030, ADR-0037, ADR-0046
- Branch/base: `feat/top20-matlab-assembly-scratch` from `9d3a0ba`
- Last updated: 2026-08-15
- Top-20 counted: no

## Capability record

| Dimension | Current evidence |
|---|---|
| Dialect/version | Package metadata is bounded to `resources/mpackage.json`, introduced in R2024b; `.m` discovery selects no MATLAB release. |
| Provider/frontend | No source frontend. In-process JSON metadata reader `bounded_mpackage_dependency_inventory_v1`; semantic provider `PROVIDER_UNAVAILABLE`. |
| Provider version | RepoGrammar crate version for the static reader; no MATLAB/Octave/Code Analyzer version. |
| Discovery/config | Normalized lowercase `.m` metadata discovery and exact root/nested `resources/mpackage.json`. |
| Manifest/lockfile | Direct `matlab_add_on` `name@uuid` declarations with optional compatible-version requirement; no lockfile/resolution/install proof. |
| Owned IR | One `project_config` unit/IR node per parsed package definition, plus owned units and IR for the ADR-0046 anchor: a module unit per decoded `.m`, a test-class unit, and a test-method unit. |
| External symbols | None. Imports, packages, class/function binding, Java/MEX, path precedence, and dynamic dispatch are unresolved. |
| Library contracts | None. Package presence proves no toolbox or runtime behavior. |
| Exact-anchor family | One: `framework:matlab_unittest.test_method` over `matlab_unittest.TestMethod`, gated at support three. Function-based and script-based tests are not implemented. |
| Fixtures/tests | Inline real-shape manifest, malformed/duplicate/conflict/unsafe-value/resource tests, discovery, routing, persistence/incremental removal, zero-family, and public leakage tests under `src/rust/`. |
| Typed UNKNOWN | `matlab_dependency_inventory` covers malformed identity/schema/container, forward schema, partial/conflicting declarations, and resource limits. Source semantics have no frontend and remain unsupported rather than guessed. |
| Source-free | Public index/status tests reject package names/UUID fragments and assembly source text. Provider/contact URLs are discarded. |

## Four-part review

### Correctness

The parser requires exact config placement, duplicate-free bounded JSON, a
valid root package identity, MATLAB-identifier subset, UUID dependency identity,
and bounded non-path version text. It preserves only declared directness and
does not populate a resolved version. Deterministic persistence, replacement,
and removal tests pass. The ASCII identifier subset is deliberately narrower
than every MATLAB Unicode identifier accepted by a licensed release.

### Security

No MATLAB, Octave, project startup/shutdown task, toolbox installer, package
manager, Java/MEX code, subprocess, path mutation, repository code, or network
operation runs. JSON depth/member/key/input/dependency bounds and URL/path-text
rejection are tested. Project XML, `.prj`, `.mlproj`, `.mltbx`, `.mlx`, `.mlapp`,
P-code, MEX, and Simulink are not parsed.

### Completeness

The static package inventory is real auxiliary evidence but the language is not
complete. `.m` is extension-only metadata and can be dialect-ambiguous. There
is no authoritative source frontend, release-aware parse degradation, source
unit/IR, symbol resolution, external package semantics, family, adversarial
product fixture corpus, or final completion audit.

### Performance

Parser inputs are capped at 1 MiB; JSON depth 128, members 8,192, decoded key
bytes 256, dependency rows 2,000, names 128 bytes, and version text 256 bytes.
Targeted tests cover exact 2,000 dependency entries and +1 abstention. No
cross-machine timing claim is made; targeted module tests complete in well under
one second after compilation on the recorded development host.

## ADR-0020 nine-gate checklist

- [ ] 1. Discovery/configuration — partial: deterministic discovery and one
  safe package format exist, but `.m` dialect/generated/build selection and
  broader project/toolbox metadata do not.
- [ ] 2. Authoritative frontend/format parser — package JSON is authoritative
  only for its bounded manifest fields, not MATLAB source.
- [x] 3. Owned code units/IR — ADR-0046 emits a module unit per decoded `.m`,
  one unit for an admitted `matlab.unittest.TestCase` class, and one per
  admitted test method, each projected into the shared IR.
- [x] 4. Typed UNKNOWN — a `Test` methods block in a class that does not derive
  from `matlab.unittest.TestCase` yields `UnresolvedImport` under
  `matlab_unittest_class_binding`, and it blocks family membership.
- [x] 5. Exact-anchor family — `framework:matlab_unittest.test_method` with
  support at least three.
- [ ] 6. Fixture proof — positive, lookalike, and low-support fixtures exist;
  parse-degraded and resolved/unresolved do not, because a scanner has no parse
  failure and there is no MATLAB provider to resolve against.
- [ ] 7. Source-free readiness — tested for this inventory path, not all
  required MATLAB readiness/unknown surfaces.
- [x] 8. Four-part review — this record.
- [ ] 9. Atomic completion audit — this branch commit is a prerequisite slice,
  not a final audit; obtain its exact SHA from file history after commit.

## Completion verdict and exact non-claims

`PARTIAL_AUDITED_PROGRESS`; strict gate count `4/9`; Top-20 counted `no`.
Beyond the bounded static R2024b+ package declarations, RepoGrammar now proves
one exact class-based `matlab.unittest` declaration shape under ADR-0046. It
still cannot prove a MATLAB release, toolbox installation, dependency
resolution, external symbols, runtime behavior, or any Simulink fact. No
`LICENSE_BLOCKED` claim is made because no licensed provider was probed.

Two limitations are stated rather than left to inference. This frontend is a
scanner, so malformed MATLAB does not fail — it yields fewer admitted
declarations, which is indistinguishable from a file with fewer declarations.
And it never interprets a statement: MATLAB's command syntax cannot be
distinguished from an expression without the workspace (`a -1` is a subtraction
or the call `a('-1')` depending on runtime binding), and that ambiguity is left
outside the claim surface rather than resolved. It does not reach the scanner,
because command arguments are unquoted and so never change what is a comment or
a string.

## Evidence paths and risks

- `src/rust/adapters/languages/matlab.rs`
- `src/rust/adapters/parsing/matlab.rs`
- `src/rust/application/indexing.rs`
- `src/rust/bin/repogrammar.rs`
- `docs/decisions/ADR-0037-matlab-package-inventory-preflight.md`

Highest risk: `.m` path identity is not an authoritative MATLAB dialect test.
The next highest-value action is a no-execution, release-pinned frontend
qualification with explicit license and redistribution evidence.
