# ADR-0057: Declarative language family packs

- Status: Accepted (maintainer acceptance 2026-10-20)
- Date: 2026-10-19
- Extends: ADR-0003, ADR-0004, ADR-0019, ADR-0020
- Evidence: [declarative language-pack spike](../experiments/declarative-language-pack-spike.md)

## Context

Every language lane currently repeats one function set across the family
builder: variation slots, variation prefixes, feature extraction, pair
compatibility, characteristic prefixes, minimum support, support-fact safety
and UNKNOWN classification. `application/family.rs` carries one copy per
language, plus one `FamilyUnknownDomain` variant each, and several copies must
be kept in sync by hand. For example, the characteristic prefixes must mirror
pair compatibility. Adding a language therefore edits discovery, parsing,
indexing, family construction and query code. The same anchor strings also
appear both in the parser's tables and in the framework registry.

No universal analyzer removes the per-language frontend. Tree-sitter covers
syntax for many languages, but name resolution, framework identity and
exact-anchor binding remain language- and framework-specific. The question
this ADR answers is narrower: where the per-language knowledge lives, and
whether the family layer can stay language-neutral.

## Decision

1. A language's family policy is a `static` `LanguageFamilyPack` value. The
   type lives in `core::model`; each instance lives with its framework registry
   under `adapters/frameworks/`. A pack holds the anchor and derived-support
   identities, the minimum support, the assumption-to-feature table and the
   per-role variation and compatibility prefixes. Characteristic prefixes are
   the equal prefixes followed by the required-equal prefixes. A pack also
   holds an UNKNOWN rule table: an exhaustive reason predicate, blocking claims,
   non-blocking reasons and claims, the default claim and the recovery scope.
   Role-namespace, role-target and support-family predicates are function
   pointers.
2. `application/family.rs` keeps one pack registry
   (`LANGUAGE_FAMILY_PACKS`). Each dispatch point has a single language-neutral
   pack implementation, and `FamilyUnknownDomain::Pack` replaces
   per-language variants for packed languages. A packed language has no
   dedicated functions in the family builder.
3. A Tree-sitter extraction lane selects its handled nodes with a committed
   `.scm` query embedded at compile time. Processing order is syntax-tree
   pre-order with ancestor-before-descendant ties, and context comes from the
   nearest scope or class-like ancestor. Anchor semantics stay procedural Rust:
   scoped imports, role precedence, receiver chains, data-source scopes,
   preprocessor regions and literal shapes. Tree-sitter remains a candidate
   layer; membership still needs exact anchors and the support gate.
4. Lanes migrate one at a time. Each migration requires byte-identical fixture
   outputs, including the full owned-generation fingerprint, an in-process
   legacy differential and unchanged `sync-equivalence`/`product-eval` results
   before the legacy functions are removed.

C# is the first and only packed lane in this change.

## Alternatives considered

- Keep per-language function sets. This is rejected because the duplication
  grows with every lane, and the hand-synchronized copies are a correctness
  risk.
- Load packs from external data files at runtime. This is rejected for now: it
  would add a parsing and validation surface and untrusted-input handling
  without a demonstrated need. Compile-time statics keep the zero-dependency
  posture.
- A generic AST shared by all languages, in the style of Semgrep's
  `ast_generic`. This is deferred because it moves the per-language cost into
  translators without removing the framework-specific binding rules that decide
  families.
- A name-resolution DSL, in the style of stack graphs. This is not adopted; the
  reference implementation was archived in 2025.
- Ingesting a standard index format, such as SCIP, for semantic facts. This
  belongs to the optional provider layer (ADR-0015/ADR-0017). It complements
  packs and does not replace them.

## Consequences

- A new Tree-sitter lane adds a pack instance, a `.scm` query, its anchor
  handlers and fixtures. The family builder only gains one registry entry.
- Anchor-semantics code stays per-language; the spike measures how much of an
  extractor that is.
- Packs are Rust data, so a policy change is still a reviewed code change with
  tests, and no runtime loading or new dependency is introduced.
- The existing application-to-adapter imports in the family builder are
  unchanged; the pack type itself lives in `core`.

## Non-claims

This ADR adds no language, framework, provider or dependency, and it changes
no public schema, support threshold or UNKNOWN semantics. The
zero-external-dependency constraint and the Go/Ruby/PHP/Swift evidence ladders
are unchanged, and strict Top-20 completion remains 0/20.

## Follow-up

- Migrate the next Tree-sitter lane (Java, then C/C++ and Rust)
  under the same oracle, and decide separately whether non-Tree-sitter lanes
  adopt the family pack without a `.scm` query.
