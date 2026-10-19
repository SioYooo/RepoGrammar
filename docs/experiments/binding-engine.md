# Language-neutral import-binding engine (Phase 2a)

Status: **PREREGISTERED**. Base: `feat/tree-sitter-frontends@6302f64`. Local
only; no push or merge. Context: ADR-0057, ADR-0058 and the
[Tree-sitter migration protocol](tree-sitter-frontend-migration.md), which
splits the anchor-rule work into a binding engine (this experiment) and a
later role-rule table (Phase 2b, preregistered separately after this
verdict).

## Question

Can one language-neutral engine replace the lane-specific code that decides
whether a written name denotes an exact framework identity? The engine covers
lexical import scopes, opened namespaces, aliases, qualified names and
lookalike detection. Each lane supplies only Tree-sitter captures and a small
binding specification. All product output must stay byte-identical.

## Consumers

- C#: `CSharpUsingContext`, `scope_using_context`, `record_exact_using`,
  `declaration_attributes`, the `attribute_*` exactness and lookalike
  functions, `base_list_types`, `base_simple_name` and `base_is_exact` in
  `adapters/parsing/csharp.rs`. Measured at 173 non-blank non-test lines; this
  is the denominator.
- Go: the `testing` import binding (`bind_import` and the
  `TestingBinding::Qualified` qualifier check in `admitted_anchor`), so the
  engine is shown not to be C#-shaped. Go keeps its lane policy (duplicate
  binding refusal, dot and blank obligations) as Go code.

## Design (draft)

- `adapters/parsing/binding.rs`: an `ImportScope` chain with opened
  namespaces and aliases; `denotes(written, namespace, names)` for exact
  resolution with the lane's separator; and `looks_like(written, names)` for
  lookalike detection. A `BindingSpec` holds the separator, accepted name
  suffixes (C#: `Attribute`), generic-argument stripping and namespace
  charset rules.
- Import directives, attributes and base types come from Tree-sitter
  captures in the lane's `.scm` query where the grammar allows. Otherwise a
  short extraction function stays in the lane and is counted as residue.

## Oracle

Same tiers as the migration protocol. Primary evidence:

1. Fixture-wide capture: 179 roots, 4,681 rows, zero mismatches against the
   migration baseline; `sync-equivalence --all` 14/14; `product-eval` equal
   apart from fields containing `latency` plus `started_at`, `finished_at` and
   `repogrammar_commit`.
2. In-process differential, with the legacy C# and Go binding code kept under
   `cfg(test)` until the verdict: identical outputs on every committed C# and
   Go fixture, the C# branch corpus and the Go branch corpus.
3. External differentials: the 4,820-file local Unity C# corpus (manifest
   `87f9d286…`) and the 1,753 Go 1.26.4 standard-library test files. Each
   difference is classified with the protocol's classes.

## Threshold (preregistered)

`QUALIFIED` requires oracle items 1 and 2 to hold. It also requires at least
80% of the 173 C# binding lines to be removed from C#-specific code: at most
35 non-blank lines may remain C#-specific, excluding binding-spec data rows.
Lines moved into the shared engine or a `.scm` query do not count as
remaining. If extraction cannot leave C# code, the measured residue is
reported and the label is `PARTIAL`. Any oracle failure that cannot be fixed
within three rounds gives `NO_GO`, with the failing constructs listed.

## Non-claims

No new language, framework, anchor, provider or dependency; no change to
UNKNOWN semantics. Role decision (which anchor applies, precedence, context
propagation) is out of scope and belongs to Phase 2b.
