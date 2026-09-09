# ADR-0038: Bounded GNU-as x86-64 ELF AT&T assembly substrate

- Status: Accepted
- Date: 2026-08-01
- Scope: Assembly in ADR-0020; structural candidates only
- Related: ADR-0020, `docs/reports/language-support/assembly-completion-review.md`

## Context

“Assembly” is not one syntax or execution model. GNU `as` is a family of
assemblers and can target multiple object formats and architectures. Its manual
also documents `.include`, macro/repetition directives, conditional assembly,
and the special uppercase `.S` convention for C-preprocessed input. A filename
alone cannot prove architecture, object format, syntax, include search path, or
macro-expanded behavior.

Reference, retrieved 2026-08-01:
<https://sourceware.org/binutils/docs/as.html> (GNU Binutils 2.46).

## Decision

The first candidate profile is fixed to GNU `as` 2.46, x86-64, ELF, AT&T
syntax. Discovery admits normalized lowercase `.s` only. Uppercase `.S` is an
explicit language-specific exclusion; `.asm`, NASM, MASM, other object formats,
and other architectures are outside the lane.

The bounded in-process lexical scanner consumes UTF-8 supplied bytes only. It
creates one module unit, generic owned label units, IR containment edges, and
source-ranged structural facts for selected directives, labels, and direct
call/jump spellings. It enforces one-MiB input, 100,000-line, 16-KiB-line,
8,192-label, and 16,384-fact limits. No fact has membership-supporting
certainty.

Every file retains `MissingProjectConfig` for the unproven target profile.
Intel/non-64-bit mode directives add `ConflictingFacts`; macro/repetition and
include/incbin forms add `MacroOrPreprocessor`; conditional forms add
`BuildVariantAmbiguity`; scanner overflow adds `InsufficientSupport`. Raw label,
branch, include, and condition text is not copied into fact targets, notes, or
assumptions.

The scanner never assembles, preprocesses, links, loads, emulates, executes,
reads includes, resolves symbols/relocations, evaluates macros/conditions, or
infers runtime control flow. It is not LLVM MC and is not an authoritative GNU
assembler parser.

## Consequences

Assembly is `structural_substrate`, not `bounded_preview`, and remains
unsupported. Other architectures and dialects are not silently mapped to this
profile. There is no exact labeled-procedure family, provider fact, relocation
model, object metadata, library dependency graph, or authoritative instruction
validation.

## Follow-up

Add repository-controlled build metadata that proves target triple, object
format, assembler, and syntax. Then qualify a version-pinned parse-only LLVM MC
or equivalent provider in a no-write/no-exec sandbox and build an exact
dialect-scoped labeled-procedure family with adversarial fixtures.
