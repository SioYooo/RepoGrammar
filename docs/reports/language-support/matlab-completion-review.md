# MATLAB language completion review

- Language/rank: MATLAB, frozen Top-20 rank 18
- Status: Incomplete — `structural_substrate`
- Authority: ADR-0020, ADR-0030, ADR-0037, ADR-0046
- Branch/base: `feat/top20-matlab-assembly-scratch` from `9d3a0ba`
- Last updated: 2026-09-04
- Top-20 counted: no

## Capability record

| Dimension | Current evidence |
|---|---|
| Dialect/version | Package metadata is bounded to `resources/mpackage.json`, introduced in R2024b; `.m` discovery selects no MATLAB release, and none is needed for the admitted subset. ADR-0046 D4c declares the MATLAB/Octave lexical-invariance set: a construct the two read differently makes the file abstain, so the dialect cannot change what is claimed. |
| Provider/frontend | Bounded in-process recursive-descent parser `repogrammar-matlab-unittest-parser` / `bounded_matlab_unittest_class_v2` over the ADR-0046 D2a subset, plus the JSON metadata reader `bounded_mpackage_dependency_inventory_v1`. No MATLAB, Octave, grammar, toolchain, or external artifact; semantic provider `PROVIDER_UNAVAILABLE`. |
| Provider version | RepoGrammar crate version for the parser and the static reader; no MATLAB/Octave/Code Analyzer version. |
| Discovery/config | Deterministic selection: exact lowercase `.m` source and exact root/nested `resources/mpackage.json` are the only candidates, and any candidate below an exact `codegen`, `slprj`, or `sccprj` component — the MathWorks code-generation output folders named by the standard MATLAB ignore template — is excluded as generated output with the shared `language_specific_exclusion` skip token, MATLAB-only, exact and case-sensitive. Generic `build`/`dist`/`generated`/`target` output is covered by the shared default exclusion table. Invalid bytes hash without decoding, oversized files skip with `too_large`, and symlinks are never followed (escape skip), all per the shared pipeline spec. |
| Manifest/lockfile | Direct `matlab_add_on` `name@uuid` declarations with optional compatible-version requirement; no lockfile/resolution/install proof. |
| Owned IR | One `project_config` unit/IR node per parsed package definition, plus owned units and IR for the ADR-0046 anchor: a module unit per decoded `.m`, a test-class unit, and a test-method unit. |
| External symbols | None. Imports, packages, class/function binding, Java/MEX, path precedence, and dynamic dispatch are unresolved. |
| Library contracts | None. Package presence proves no toolbox or runtime behavior. |
| Exact-anchor family | One: `framework:matlab_unittest.test_method` over `matlab_unittest.TestMethod`, gated at support three, with positive, lookalike, low-support, parse-degraded, and command-syntax-abstention fixtures. Function-based and script-based tests are not implemented. |
| Fixtures/tests | Inline real-shape manifest, malformed/duplicate/conflict/unsafe-value/resource tests, discovery, routing, persistence/incremental removal, zero-family, and public leakage tests under `src/rust/`. Parser exclusions are tested before admissions: command-syntax divergence, Octave-only lexemes, unadmitted `classdef` members, abstract and valued `Test` attributes, contextual keywords used as variables, non-classdef-first files, and adversarial nesting/Unicode input. |
| Typed UNKNOWN | `matlab_dependency_inventory` covers malformed identity/schema/container, forward schema, partial/conflicting declarations, and resource limits. Source claims add `matlab_unittest_class_binding` (unbound `Test` block), the blocking `matlab_block_structure` and `matlab_dialect_invariance` abstentions, and the non-blocking `matlab_classdef_body_shape`, `matlab_abstract_test_methods`, and `matlab_test_attribute_value` observations. Every one names a bounded class; no divergent source text reaches a fact. |
| Source-free | Public index/status tests reject package names/UUID fragments and assembly source text. Provider/contact URLs are discarded. |

## Four-part review

### Correctness

The config parser requires exact config placement, duplicate-free bounded JSON,
a valid root package identity, MATLAB-identifier subset, UUID dependency
identity, and bounded non-path version text. It preserves only declared
directness and does not populate a resolved version. Deterministic persistence,
replacement, and removal tests pass. The ASCII identifier subset is deliberately
narrower than every MATLAB Unicode identifier accepted by a licensed release.

The source frontend's correctness rests on two invariance arguments rather than
on a MATLAB release. Command syntax cannot be told from an expression without
the workspace, so ADR-0046 D4b admits a possibly-command statement only when
both readings consume the same span and neither opens or closes a block, and
abstains for the whole file otherwise. MATLAB and Octave share `.m`, so D4c
admits only constructs both lex the same way. Two defect classes the earlier
line scanner carried are closed by the grammar itself: `methods`, `properties`,
and `arguments` are contextual, so `methods = getMethods(x);` no longer opens a
phantom block, and an unrecognised `classdef` member abstains instead of
mis-nesting around a keyword a later release may add.

### Security

No MATLAB, Octave, project startup/shutdown task, toolbox installer, package
manager, Java/MEX code, subprocess, path mutation, repository code, or network
operation runs. JSON depth/member/key/input/dependency bounds and URL/path-text
rejection are tested. Project XML, `.prj`, `.mlproj`, `.mltbx`, `.mlx`, `.mlapp`,
P-code, MEX, and Simulink are not parsed. Source input is treated as untrusted:
descent is bounded at 256 blocks so pathological nesting degrades instead of
overflowing the stack, non-ASCII bytes are handled without panicking, and every
abstention names a bounded class rather than quoting the construct that caused
it.

### Completeness

The bounded source frontend and the static package inventory are both real
evidence, and the language is still not complete. `.m` is extension-only
metadata. The frontend is release-agnostic by construction rather than
release-qualified: no MathWorks parser, grammar, or Code Analyzer is consulted,
identifiers are the ASCII subset, and the admitted subset is one class-based
test shape. There is no symbol resolution, external package semantics, or final
completion audit.

### Performance

Parser inputs are capped at 1 MiB; the source parser is bounded at 4,096 units
and 256 nested blocks; JSON depth 128, members 8,192, decoded key bytes 256,
dependency rows 2,000, names 128 bytes, and version text 256 bytes. Lexing is a
single left-to-right pass and the descent visits each token a bounded number of
times. Targeted tests cover exact 2,000 dependency entries and +1 abstention,
and 4,000 nested blocks degrading rather than recursing. No cross-machine timing
claim is made; targeted module tests complete in well under one second after
compilation on the recorded development host.

## ADR-0020 nine-gate checklist

- [x] 1. Discovery/configuration — deterministic selection is implemented and
  tested: exact lowercase `.m` and exact root/nested `resources/mpackage.json`
  are the only candidates (lookalikes `.M`, `.mlx`, `toolbox.mltbx`,
  `mpackage.json` outside `resources/` are refused); generated/build output is
  excluded per a documented rule — MATLAB candidates below an exact
  `codegen`/`slprj`/`sccprj` component are skipped as
  `language_specific_exclusion` without hiding other languages below those
  components, and generic build/dependency output is covered by the shared
  default exclusion table; project metadata inventory is bounded to
  `resources/mpackage.json` exactly as ADR-0037 decides it (project XML,
  `.prj`, `.mlproj`, and toolbox archives are refused inputs, not missing
  work); and invalid/oversized/symlink behavior is the shared pipeline
  contract, pinned for this lane by tests (binary `.m` bytes hashed without
  decoding, symlink escape refused). Dialect is not selected by discovery:
  `.m` admits no MATLAB-versus-Octave claim, and ADR-0046 D4c bounds the
  question by lexical invariance with a typed file-level abstention.
- [x] 2. Authoritative frontend/format parser for the declared scope —
  ADR-0046's bounded in-process recursive-descent parser is the primary syntax
  evidence for MATLAB source, on the terms ADR-0040 established for SQL. Its
  fidelity boundary is a declared grammar (D2a), not a heuristic: it accepts or
  refuses each construct, never recovers, and never resynchronises. Where the
  parse would depend on something undeterminable it abstains and says so — on
  the command/expression ambiguity (D4b) and on MATLAB/Octave lexical
  divergence (D4c) — and its parse-degraded behaviour is explicit for an
  unclosed `%{`, an unterminated character array, a block structure that does
  not close, and a descent that exceeds its bound. No repository runtime code,
  build script, macro, generator, or package script executes, and no MATLAB,
  Octave, grammar, or external artifact is involved. Package JSON remains
  authoritative only for its bounded manifest fields. This is not a
  release-qualified MATLAB frontend and the ADR forbids widening the subset
  without a superseding decision.
- [x] 3. Owned code units/IR — ADR-0046 emits a module unit per decoded `.m`,
  one unit for an admitted `matlab.unittest.TestCase` class, and one per
  admitted test method, each projected into the shared IR.
- [x] 4. Typed UNKNOWN — a `Test` methods block in a class that does not derive
  from `matlab.unittest.TestCase` yields `UnresolvedImport` under
  `matlab_unittest_class_binding`, and it blocks family membership. An
  undecidable block extent yields `ConflictingFacts` under
  `matlab_block_structure` or `matlab_dialect_invariance` and blocks too;
  leaving the declared subset yields `InsufficientSupport` under
  `matlab_classdef_body_shape`, `matlab_abstract_test_methods`, or
  `matlab_test_attribute_value` and does not block, because it understates what
  a file declares without unproving what was parsed. The application-side
  classifier is asserted to reach the same verdicts from the fact alone.
- [x] 5. Exact-anchor family — `framework:matlab_unittest.test_method` with
  support at least three.
- [x] 6. Fixture proof — positive, lookalike, low-support, parse-degraded, and
  command-syntax-abstention fixtures exist under
  `src/fixtures/matlab/release/v0_2/`. An unbalanced block or an unclosed `%{`
  is a decidable well-formedness violation and reports a degraded parse. The
  abstention fixture is the positive fixture plus one `format end` statement, so
  it proves the family that would otherwise form at support three is withheld
  rather than never formed. Resolved/unresolved fixtures do not exist: there is
  no MATLAB provider to resolve against.
- [x] 7. Source-free readiness — MATLAB is registered in the repo-shape
  language scopes, so its units and families are counted rather than silently
  reported as zero; `status`, `doctor`, `stats`, `unknowns`, `families`,
  `files`, and the MCP `inspect_readiness` and `find_analogues` payloads are
  each asserted over both an indexed `TestCase` workspace and an indexed
  unbound-`Test`-block one to expose no identifier, literal, or source text and
  no absolute path. The assertions are non-vacuous: every command must exit zero
  and parse, the positive workspace must report
  `framework:matlab_unittest.test_method`, and the unbound one must report the
  lane's typed `UNKNOWN` by bounded language token `matlab` and count.
- [x] 8. Four-part review — this record.
- [ ] 9. Atomic completion audit — this branch commit is a prerequisite slice,
  not a final audit; obtain its exact SHA from file history after commit.

## Completion verdict and exact non-claims

`PARTIAL_AUDITED_PROGRESS`; strict gate count `8/9`; Top-20 counted `no`.
Beyond the bounded static R2024b+ package declarations, RepoGrammar now proves
one exact class-based `matlab.unittest` declaration shape under ADR-0046. It
still cannot prove a MATLAB release, toolbox installation, dependency
resolution, external symbols, runtime behavior, or any Simulink fact. No
`LICENSE_BLOCKED` claim is made because no licensed provider was probed.

Gate 1 moved from unchecked to checked on 2026-09-04 by closing the selection
rule, and the scope of what closed is stated exactly. The generated/build
exclusion set is the three MathWorks code-generation output folders
(`codegen` for MATLAB Coder output, `slprj` and `sccprj` for Simulink
simulation/code-generation targets) named by the standard MATLAB ignore
template; nothing beyond that set is excluded, because no lane authority names
another MATLAB generated-location convention and inventing one would convert a
heuristic into certainty. Discovery still selects no MATLAB release and no
MATLAB-versus-Octave dialect: `.m` is admitted as a shared-extension candidate
only, and the dialect question is bounded downstream by ADR-0046 D4c lexical
invariance, never guessed. Broader project/toolbox metadata inventory remains
out of scope by ADR-0037 decision — the project XML format is documented as
unstable and executing a project is forbidden — so its absence is a non-claim,
not an open obligation of this gate.

Gate 2 moved from unchecked to checked, and the reason it was unchecked is
retracted rather than quietly dropped. The earlier record argued the ambiguity
never reached the frontend "because command arguments are unquoted and so never
change what is a comment or a string." That is true and it answers the wrong
question. A line scanner only had to know which bytes were comment or string; a
parser has to decide where each block closes, and an unquoted command argument
can be the word `end` or an unbalanced bracket, which is exactly that decision.
The ambiguity does reach a parser. ADR-0046 D4b bounds it: where the command and
expression readings agree about block structure the file is parsed, and where
they disagree the whole file abstains with a typed `UNKNOWN` and no anchor.

Four limitations are stated rather than left to inference. The frontend still
never interprets a statement — expressions are consumed as opaque token runs, so
`a -1` is neither read as a subtraction nor as `a('-1')`, and the ambiguity is
bounded rather than resolved. Some abstained cases are in principle decidable
(`x end` has no well-formed expression reading, so the command reading is
forced), and deciding them would require an expression grammar that ADR-0046
deliberately does not build. Support is understated by design: a file carrying a
command-syntax divergence, an Octave-only lexeme, or an unadmitted `classdef`
member produces no anchor at all, and that absence is not evidence the file
declares no tests. And Octave's `do … until` is not detected, because `do` and
`until` are ordinary MATLAB identifiers and refusing them would abstain on legal
MATLAB.

## Evidence paths and risks

- `src/rust/adapters/languages/matlab.rs`
- `src/rust/adapters/filesystem/discovery.rs`
- `src/rust/adapters/parsing/matlab.rs`
- `src/rust/adapters/parsing/matlab/unittest.rs`
- `src/rust/application/family.rs`
- `src/rust/application/indexing.rs`
- `src/rust/bin/repogrammar.rs`
- `src/fixtures/matlab/release/v0_2/`
- `docs/decisions/ADR-0037-matlab-package-inventory-preflight.md`
- `docs/decisions/ADR-0046-bounded-matlab-unittest-frontend.md`

Highest risk: `.m` path identity is not an authoritative MATLAB dialect test,
and the frontend is release-agnostic rather than release-qualified — it is
correct for the declared subset under either dialect, and it proves nothing
about the release a repository targets. The next highest-value action is a
no-execution, release-pinned frontend qualification with explicit license and
redistribution evidence.
