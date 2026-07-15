# ADR-0020: Top-20 language expansion completion gate

- Status: Accepted (explicit maintainer direction, 2026-07-15)
- Date: 2026-07-15
- Refines: ADR-0004 (language-native worker boundary), ADR-0015
  (provider-backed analyzer execution), ADR-0016 (typed semantic obligations),
  ADR-0017 (provider capability registry), and ADR-0019 (bounded structural
  expansion)
- Related: `docs/plans/top-20-language-expansion-plan.md`,
  `docs/plans/multi-language-expansion-plan.md`,
  `docs/reports/unknown-resolution-sota-analysis.md`,
  `docs/specifications/unknowns.md`, and `docs/roadmap.md`

## Context

RepoGrammar has bounded source-language paths for seven languages in the
maintainer-selected Top-20 snapshot: Python, C, C++, Java, C#, JavaScript, and
Rust. TypeScript is also a current language, but it is outside the ranked 20.
Those paths are not equivalent in maturity: Python is the official v0.1 focus,
while the other paths are bounded previews or transitional substrate. In
particular, extension recognition and a Tree-sitter parse alone do not prove
that a language is supported.

The project needs a durable expansion target and one falsifiable definition of
completion. Without that gate, a language could be counted after adding only a
file extension, or a structural preview could be mistaken for provider-backed
semantic coverage. The gate also has to preserve RepoGrammar's existing
sound-by-abstention rule: recoverable `UNKNOWN`s should be replaced only by
source-backed facts, while irreducible uncertainty remains typed `UNKNOWN`.

## Decision

Adopt the official [TIOBE Index](https://www.tiobe.com/tiobe-index/) July 2026
ranking as a dated planning snapshot. This ADR freezes the following ordered
list for this expansion program; changes to the live index do not silently
change scope:

| Rank | Language |
|---:|---|
| 1 | Python |
| 2 | C |
| 3 | C++ |
| 4 | Java |
| 5 | C# |
| 6 | JavaScript |
| 7 | Visual Basic |
| 8 | SQL |
| 9 | R |
| 10 | Rust |
| 11 | Delphi/Object Pascal |
| 12 | Scratch |
| 13 | Go |
| 14 | PHP |
| 15 | Swift |
| 16 | Ada |
| 17 | Assembly |
| 18 | MATLAB |
| 19 | Fortran |
| 20 | Ruby |

This is a planning authority snapshot, not a claim about language quality,
repository prevalence, or current RepoGrammar support. TypeScript remains an
explicit extra language and is not counted in the 20.

### D1. Scope sets are disjoint

The program has three non-overlapping sets:

- Current Top-20 convergence: Python, C, C++, Java, C#, JavaScript, and Rust.
  Their existing discovery and structural slices are starting points, not
  automatic proof that this ADR's completion gate is satisfied.
- Extra-language convergence: TypeScript only. It follows the same provider and
  `UNKNOWN` convergence discipline but never contributes to the Top-20 count.
- New Top-20 expansion: Visual Basic, SQL, R, Delphi/Object Pascal, Scratch,
  Go, PHP, Swift, Ada, Assembly, MATLAB, Fortran, and Ruby.

No implementation wave may assign one language to more than one of these sets.
Dialect families such as Visual Basic, SQL, Delphi/Object Pascal, and Assembly
must declare their initial bounded dialect or format scope; a bounded dialect
must never be reported as universal coverage of the whole label.

### D2. Per-language completion evidence

A language is complete for this program only when an auditable chain of atomic
submodule commits supplies all of the following evidence:

1. **Discovery and configuration:** deterministic source discovery, guarded
   extensions or format recognition, generated/dependency/build exclusions,
   project-configuration inventory where relevant, and invalid/oversized/
   symlink behavior. Extension-only recognition is discovered-only state, not
   support.
2. **Authoritative frontend or format parser:** a language-native compiler,
   parser, LSP/analyzer frontend, or authoritative format parser produces the
   primary syntax/semantic evidence. A maintained grammar may generate
   candidates only when its fidelity boundary and parse-degraded behavior are
   explicit. Tree-sitter alone is never the semantic oracle. The integration
   must not execute repository runtime code, build scripts, macros, generators,
   or package scripts.
3. **Code units and RepoGrammar-owned IR:** stable language tokens, structural
   code units, source ranges, hashes, IR nodes/edges, and deterministic
   serialization/storage behavior are present. Parser-native objects do not
   leak across the port boundary.
4. **Typed `UNKNOWN`:** claim-scoped unknowns cover missing configuration or
   dependencies, ambiguity, generated/dynamic/runtime behavior, stale or
   conflicting evidence, parse degradation, and insufficient support. The
   language records which unknowns are recoverable by an authorized provider
   and which are irreducible. Counts may fall only when source-backed
   replacement facts discharge the same obligation.
5. **Family-first exact-anchor slice:** at least one recurring family reaches
   the existing conservative support and compatibility gates from exact,
   source-visible anchors. If a framework family is not meaningful for the
   language or format, the completion review may mark the framework choice
   not-applicable only by substituting a language-internal recurring-pattern
   family with an exact anchor, compatibility features, and explicit non-
   claims. SQL statement/migration shapes, Scratch event-script stacks, and
   Assembly dialect-scoped labeled procedures are examples of eligible shapes;
   raw structural similarity is not evidence.
6. **Fixture proof:** committed positive, negative/lookalike, low-support, and
   parse-degraded fixtures exercise product paths. Add dynamic, build-variant,
   stale/conflicting, and unresolved-to-resolved fixtures whenever those claims
   exist. Positive support must meet the minimum family-support threshold;
   negative and degraded fixtures must not form a confident family.
7. **Source-free readiness:** `status`, `doctor`, `stats`, `unknowns`, CLI, and
   MCP inventory/readiness surfaces expose only bounded tokens, states, counts,
   provenance, and recovery mechanisms by default. Tests must reject source
   text, absolute paths, raw diagnostics, and high-cardinality identifiers.
8. **Four-part review record:**
   `docs/reports/language-support/<language>-completion-review.md` records
   correctness/bug findings, security and untrusted-input handling,
   implementation completeness against this gate, and performance/resource
   bounds. Open issues remain explicit risks or typed `UNKNOWN`; the record is
   evidence, not a replacement for tests.
9. **Atomic delivery and completion audit:** discovery/configuration,
   frontend/IR, `UNKNOWN`/provider, family/fixtures, and review/documentation
   are independently coherent submodules. Each completed submodule lands as an
   atomic Conventional Commit with its corresponding tests and documentation
   record. The final language completion audit/review commit links every exact
   prerequisite SHA, reruns the full required gates, updates the final support
   state, and verifies that the linked chain satisfies all nine items.

An inventory label may distinguish `discovered_only`, `structural_substrate`,
`bounded_preview`, and a stronger provider-backed state. Only
`bounded_preview` or stronger may be described as supported, and only after all
nine items above are satisfied. A structural substrate already present at the
date of this ADR must still be audited against the full gate.

### D3. Provider and dependency authority

ADR-0015 authorizes staged analyzer execution, but it does not preselect every
frontend or dependency. Before adding a production dependency or executing a
new provider, each language slice must document:

- the dialect/version and repository inputs it supports;
- acquisition, consent, isolation, timeout, cache, and failure behavior;
- license/supply-chain and version-pinning decisions;
- which semantic obligations the provider can discharge;
- the typed fallback when the provider is absent, fails, or sees degraded
  syntax; and
- a controlled unresolved-to-resolved fixture pair proving a source-backed
  upgrade without false family formation.

No provider failure may silently downgrade to a confident structural claim.
No production dependency is authorized by this ADR alone.

### D4. Delivery program

The active wave assignment and handoff gates live in
`docs/plans/top-20-language-expansion-plan.md`. Each new language receives a
dedicated major-feature branch and exclusive file ownership while active.
Shared registries are integrated sequentially. A wave is a coordination unit,
not permission to mix unrelated languages in a commit or to collapse a whole
language into one mega commit.

Completion is reported per language, not by wave percentage. The Top-20 program
is complete only when all 20 ranked languages independently satisfy D2 and the
TypeScript-extra lane has been reported separately. Until then, README and
product documentation must say which languages are discovered-only,
structural substrate, bounded preview, or provider-backed; they must not say
"Top-20 support" without qualification.

### D5. This acceptance changes no runtime behavior

This ADR and its initial plan establish a normative gate only. They do not add
language tokens, discovery extensions, parsers, providers, family claims,
dependencies, or runtime behavior. Existing v0.1 Python-first and public-
preview labels remain unchanged until later atomic submodule commits and a
final language completion audit satisfy their applicable gates.

## Alternatives considered

- Count every recognized extension as supported: rejected because discovery
  proves neither parsing fidelity nor family evidence.
- Count every Tree-sitter adapter as complete: rejected because Tree-sitter is
  a syntax/candidate layer and cannot discharge semantic obligations alone.
- Use the live TIOBE page as continuously moving scope: rejected because plans
  and completion evidence would become non-reproducible. A successor ADR may
  adopt a later dated snapshot.
- Ship all 13 new languages in one branch or commit, or ship one language as a
  mega commit: rejected because failures, submodule tests, reviews,
  dependencies, and rollback could not be attributed to small coherent
  boundaries.
- Remove irreducible `UNKNOWN`s to meet a coverage target: rejected because a
  lower count without a source-backed replacement fact is false certainty.

## Consequences

- The July 2026 Top-20 list becomes durable planning scope while product
  support remains evidence-based and language-specific.
- Existing C, C++, C#, Java, JavaScript, Python, and Rust paths need a
  convergence audit; their presence does not grandfather them through D2.
- TypeScript continues as a first-class extra lane without distorting the
  ranked-language denominator.
- Thirteen new languages are sequenced in disjoint waves, but every language
  retains its own frontend decision, fixtures, review record, quality gates,
  atomic submodule commits, and final completion-audit commit.
- Dialect-sensitive labels require honest bounded scopes and explicit non-
  claims.
- Repo guard requires this ADR and its active plan so the normative gate cannot
  disappear silently.

## Follow-up work

- Execute the current-language and TypeScript convergence audits before
  claiming D2 completion for existing paths.
- Execute new-language waves in the active plan; write a provider/dependency
  decision before each production integration that needs one.
- Update `docs/reports/unknown-resolution-sota-analysis.md` when a language
  provider or new `UNKNOWN` recovery mechanism lands.
- Keep `docs/reports/language-support/` completion reviews and the README
  support matrix synchronized with implemented evidence.
- Write a superseding ADR if the ranked snapshot, completion gate, or no-
  execution boundary changes.
