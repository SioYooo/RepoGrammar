# Assembly language completion review

- Language/rank: Assembly, frozen Top-20 rank 17
- Status: Incomplete — `structural_substrate`
- Authority: ADR-0020 and ADR-0038
- Branch/base: `feat/top20-matlab-assembly-scratch` from `9d3a0ba`
- Last updated: 2026-08-01
- Top-20 counted: no

## Capability record

| Dimension | Current evidence |
|---|---|
| Dialect/version | Candidate profile fixed to GNU `as` 2.46, x86-64, ELF, AT&T syntax; the path does not prove the profile. |
| Provider/frontend | Bounded in-process lexical scanner only; not GNU as or LLVM MC. Authoritative provider unavailable. |
| Provider version | RepoGrammar crate version; GNU as 2.46 is the documentation snapshot, not an executed provider. |
| Discovery/config | Normalized lowercase `.s`; uppercase `.S` is explicitly excluded due C preprocessing. No target/build config. |
| Manifest/lockfile | None. `.include`/`.incbin` are unresolved syntax, not native-system packages. |
| Owned IR | Module plus generic label units and containment edges; selected directive and direct branch structural facts. |
| External symbols | None. Direct call/jump spellings are candidates; symbol, relocation, visibility, linkage, and target identity unresolved. |
| Library contracts | None. |
| Exact-anchor family | None. No labeled-procedure family reaches the compatibility/support gate. |
| Fixtures/tests | Positive lexical shapes, indirect/lookalike negatives, Intel conflict, macro/include/condition unknowns, line/label limits, discovery/routing/persistence/source-free tests. |
| Typed UNKNOWN | Unproven profile, dialect conflict, macro/include, conditional assembly, and resource limit. |
| Source-free | Facts use low-cardinality tokens only; tests reject raw include, condition, label, and branch text. |

## Four-part review

### Correctness

The scanner returns exact source ranges for a deliberately small lexical subset
and never labels them semantic. Intel/non-64-bit mode directives suppress branch
candidates after conflict. Indirect calls, registers, immediates, extra operands,
numeric labels, invalid labels, and comments do not form direct-branch/label
candidates. It does not validate mnemonics, operands, sections, symbols, or GAS
grammar, so the profile UNKNOWN remains mandatory.

### Security

No assembler, preprocessor, linker, loader, debugger, emulator, binary, include,
macro, conditional, repository code, subprocess, or network operation runs.
Input, line, line-length, label, and fact counts are bounded. Uppercase `.S` is
not read through the parser. Raw source identifiers are not copied into facts.

### Completeness

Only one declared candidate profile exists. NASM/MASM, Intel syntax, Mach-O,
COFF/PE, ARM/AArch64, RISC-V, WebAssembly text, inline assembly, generated
assembly, preprocessing, relocation/object metadata, and runtime control flow
are outside scope. There is no authoritative parser/provider or exact family.

### Performance

The scan is a single pass over at most 1 MiB, 100,000 lines, 16 KiB per line,
8,192 label units, and 16,384 facts. Targeted tests cover a 16-KiB+1 line and an
8,192+1 label workload, plus fact-ceiling reservation for typed abstention. No
allocator/peak-memory or cross-machine benchmark claim is made.

## ADR-0020 nine-gate checklist

- [ ] 1. Discovery/configuration — partial; deterministic `.s`/`.S` policy but
  no authoritative target/build configuration.
- [ ] 2. Authoritative frontend — lexical scanner is explicitly non-authoritative.
- [ ] 3. Owned code units/IR — partial generic IR; no resolved procedure/symbol model.
- [ ] 4. Typed UNKNOWN — partial; implemented for current scanner boundaries,
  no provider recovery registry.
- [ ] 5. Exact-anchor family — absent.
- [ ] 6. Fixture proof — inline unit/product tests, no committed family fixture corpus.
- [ ] 7. Source-free readiness — current product outputs tested, not the full
  Assembly readiness matrix.
- [x] 8. Four-part review — this record.
- [ ] 9. Atomic completion audit — prerequisite slice only; obtain exact SHA
  from file history after commit.

## Completion verdict and exact non-claims

`PARTIAL_AUDITED_PROGRESS`; strict gate count `1/9`; Top-20 counted `no`.
RepoGrammar can prove only that bounded source-visible spellings are candidates
under one declared scanner profile. It cannot prove that a file is x86-64 ELF
AT&T GAS, that an instruction is valid, that a symbol resolves, that a branch
is reachable, or that a labeled procedure forms a family. It does not support
“all assembly.”

## Evidence paths and risks

- `src/rust/adapters/languages/assembly.rs`
- `src/rust/adapters/parsing/assembly.rs`
- `src/rust/application/indexing.rs`
- `src/rust/bin/repogrammar.rs`
- `docs/decisions/ADR-0038-bounded-gas-assembly-substrate.md`

Highest risk: extension/dialect ambiguity. The next highest-value action is a
repository-controlled target-profile record plus a pinned parse-only provider
that can discharge the exact same profile obligation.

## ADR-0058 frontend change — 2026-10-20

The migration verdict is `BLOCKED`. No production change was made: the
scanner in `src/rust/adapters/parsing/assembly.rs` is unchanged and
`tree-sitter-asm` was not admitted. A spike with `tree-sitter-asm` 0.24.0
(`tree_sitter_asm::LANGUAGE`) showed that the generic grammar does not parse
GNU `as` AT&T syntax, so reproducing the candidate set would mean
reimplementing the scanner on top of an error-ridden tree.

Under the ADR-0058 rule that any `ERROR` or `MISSING` node refuses the file:

- 5 of the 8 probed unit-test sources, including the first emission test, the
  macro/include test and the comment/malformed-operand test, produce `ERROR`
  or `MISSING` nodes.
- All 679 `.s` files found on the machine (Rust compiler-builtins, LLVM
  libunwind tests and similar; several are non-x86) contain an `ERROR` or
  `MISSING` node, so the lane would abstain on every file.

Constructs the grammar mis-tokenizes or cannot express:

- `@` is not a token: `.type entry,@function`, `.section ...,@progbits` and
  `call bar@PLT` fail, although `@` is a legal symbol character for the scanner.
- `*` indirect operands (`call *%rax`, `call *target`) fail; the grammar admits
  `*` only as `*rel[int]`.
- A label and a statement on one line (`.L1: ret`, `foo: call bar`) and
  chained labels (`a: b:`) fail, because the grammar separates items only by
  newline.
- `;` and `//` are always comments; in GNU `as` x86 `;` separates statements
  and `//` is not a comment.
- A backslash macro parameter (`call \target`) is an `ERROR`.
- Numeric local labels (`1:`) and operands such as `bad-label!:` are read as
  instructions, expression fragments or `MISSING` nodes instead of being
  distinguished from labels.

The lane's whole job is these byte-exact spellings (labels, selected
directives, direct call/jump operands) with the scanner's own `is_symbol` and
line-shape rules, so no part of the syntax layer can move to the grammar
without re-implementing it. The scanner stays the production frontend. A
revisit needs either an AT&T-aware grammar or the parse-only LLVM MC provider
named in ADR-0038 follow-up. Status above is unchanged.
