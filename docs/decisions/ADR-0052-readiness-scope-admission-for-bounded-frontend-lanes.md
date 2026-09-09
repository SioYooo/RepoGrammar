# ADR-0052: Readiness-scope admission for bounded-frontend lanes

- Status: Proposed — requires explicit maintainer direction, not an
  implementing agent's self-grant
- Date: 2026-09-05
- Scope: whether a language whose bounded frontend has landed under ADR-0020
  may appear in the public per-language readiness and stats surface
- Refines: ADR-0019 (which admitted C# and C/C++ to `bounded_v0_2_preview` by
  naming them individually, and carries no blanket clause for later languages),
  ADR-0011 (unchanged: the official v0.1 target stays Python-first)
- Related: ADR-0020, ADR-0040, ADR-0042 through ADR-0051,
  `docs/reports/language-support/top-20-program-summary.json`

## Context

`readiness_language_scopes()` in `src/rust/application/query.rs` is the list
that decides which languages the public `stats` surface reports as a
per-language row, each carrying a `language_scope` and a `preview_status`. It
contains six entries: `python` at `official_v0_1`, and `typescript/javascript`,
`rust`, `java`, `csharp`, and `c/cpp` at `bounded_v0_2_preview`. Those five
preview entries exist because ADR-0019 D1 named them.

Eleven further languages have since landed bounded frontends and exact-anchor
families under ADR-0020: SQL, R, Visual Basic .NET, Delphi/Object Pascal, Ada,
and MATLAB, then PHP, Swift, Ruby, Go, and Fortran. Each stands at eight of
nine ADR-0020 gates. None appears in `readiness_language_scopes()`.

The counting substrate for them is present and correct. `REPO_SHAPE_LANGUAGE_SCOPES`
and the four `repo_shape_*_where` predicates in
`src/rust/adapters/persistence/sqlite.rs` cover all of them, pinned by
`every_repo_shape_scope_has_all_four_predicates`. So their repo-shape counts are
computed and stored; they are simply never projected into a public row.

This ADR exists because that gap is not an oversight to be closed by an
implementing agent. `readiness_language_scopes()` is the one list in the
codebase where adding a language asserts a product support scope. The mirrored
agent contract states that the official v0.1 scope is Python-first and that
existing TypeScript/JavaScript substrate "must not be described as the official
v0.1 target unless a later ADR changes scope." Extending the list is exactly
such a change, and ADR-0020's own D2 warns that a parser, a family, or a
fixture is never by itself evidence of language support.

## The question this decision must answer

Does a language that has landed a bounded frontend, owns its code units and IR,
declares its typed `UNKNOWN` set, forms an exact-anchor family through the
product, and carries a source-free readiness check — that is, a language at
eight of nine ADR-0020 gates — qualify for the `bounded_v0_2_preview` readiness
scope that ADR-0019 granted C# and C/C++?

The honest argument on each side:

**For admission.** The five languages ADR-0019 admitted were admitted on
*structural* evidence — Tree-sitter structural candidates plus exact anchors.
The eleven ADR-0020 lanes clear a strictly higher bar: a real parse of a
declared subset with whole-file abstain-or-admit semantics, an explicit
invariance argument, and a family proven end to end through the product CLI.
Withholding a stats row from a lane with stronger evidence than the lanes that
already have one is inconsistent. A stats row reports counts and a preview
label; it is not a support claim in the ADR-0020 sense.

**Against admission.** A `preview_status` in public output is read by users as
"this language is supported to a preview standard," whatever the field's
internal meaning. None of the eleven is complete; all eleven are open at
gate 9; and ADR-0020's stated purpose is to stop exactly this kind of partial
evidence from being promoted into a coverage claim. The counts being absent
from a public row costs nothing that matters — the families are already
reachable through `find`, `families`, and the MCP surface, which is where the
product's value is delivered.

## Decision (proposed, not in force)

If the maintainer accepts this ADR, the following would apply. Until then,
`readiness_language_scopes()` stays as it is.

### D1. Admission bar

A language may be added to `readiness_language_scopes()` at
`bounded_v0_2_preview` only when all of the following hold, each verifiable
from committed evidence:

1. its bounded frontend is dispatched in the parser registry and is not a
   scanner or lexical-candidate substrate;
2. it owns code units, source ranges, and IR nodes and edges, with no
   parser-native object crossing the port boundary;
3. its typed `UNKNOWN` set is declared claim-scoped with a recorded
   provider-fallback position for each entry;
4. at least one exact-anchor family forms through the product CLI at the
   language's pinned support bar, proven by a committed product-level test over
   a committed fixture, with negative, low-support, and parse-degraded fixtures
   that form nothing;
5. its `stats`, `unknowns`, and `doctor` output is source-free, pinned by a
   committed assertion rather than a one-off manual check; and
6. its four `repo_shape_*_where` predicates exist, so the row it gains reports
   real counts rather than a silent zero.

A language that meets D1 but is open at ADR-0020 gate 9 is admitted to the
readiness scope and is still not complete. The two states are independent and
must not be conflated in any surface or document.

### D2. Languages that would qualify today

Against D1 as written, SQL, R, Visual Basic .NET, Delphi/Object Pascal, Ada,
MATLAB, PHP, Swift, Ruby, Go, and Fortran meet items 1 through 4 and item 6.

Item 5 is met only in part. Six lanes carry a committed CLI-level readiness
assertion over `status`, `doctor`, `stats`, `unknowns`, `families`, and
`files`: Delphi, Ada, MATLAB, Visual Basic, and R through the shared
`assert_scanner_lane_readiness_is_source_free` helper, and SQL through its own
`sql_readiness_surfaces_stay_source_free_and_low_cardinality`, which walks the
same six surfaces.

The five lanes landed on 2026-09-05 — PHP, Swift, Ruby, Go, and Fortran — do
not. They carry frontend-level source-free tests and product-level
init/index/families tests, and their `stats` and `unknowns` output was checked
empirically on that date and exposed only bounded tokens, counts, role names,
and reason codes. That check is not pinned by a committed assertion, so it
protects nothing against regression. Extending the shared helper to those five
is a prerequisite to their admission under D1, not a consequence of it.

### D3. Non-claims

- Admission grants a counting row and a preview label. It grants no claim about
  arbitrary third-party library behavior, runtime equivalence, or completeness.
- The official v0.1 scope stays Python-first. `official_v0_1` is not granted to
  any language by this decision.
- No language's ADR-0020 gate count changes by being admitted, and no admitted
  language may be described as supported, complete, or covered.

## Consequences if accepted

`readiness_language_scopes()` grows from six entries to seventeen, and every
consumer that iterates it — the `stats` per-language rows and the
`by_language_detail` block of `unknowns` — reports eleven more rows. The rows
would read as mostly-zero for a repository that contains none of those
languages, which is already true of the existing six.

The prerequisite work is extending
`assert_scanner_lane_readiness_is_source_free` to PHP, Swift, Ruby, Go, and
Fortran. That needs no external artifact and is worth doing on its own merits,
because it converts a one-off manual check into a committed gate-7 assertion
whether or not this ADR is accepted.

## Consequences if rejected

The eleven lanes keep their families reachable through `find`, `families`, and
MCP, and keep their repo-shape counts in storage without a public row. Each
affected completion review already records this as a known limitation, so the
rejection needs no further documentation. This ADR should then be marked
Rejected rather than deleted, so the question is not silently reopened by a
later session reading the same gap as an oversight.
