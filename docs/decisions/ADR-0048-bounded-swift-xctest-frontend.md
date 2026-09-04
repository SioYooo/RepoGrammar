# ADR-0048: Bounded Swift XCTest frontend

- Status: Accepted
- Date: 2026-09-04
- Scope: Swift in ADR-0020; admits a family-bearing frontend over one
  class-and-method shape
- Refines: ADR-0025 (authorizes the Swift source unit and the XCTest family
  its evidence ladder had closed; supersedes the family closure only, in D8
  below)
- Related: ADR-0041, ADR-0042, ADR-0043, ADR-0044, ADR-0045, ADR-0046 (the
  bounded-parser precedent this decision follows),
  `docs/reports/language-support/swift-completion-review.md`,
  `docs/plans/swift-n1-qualification-handoff.md`,
  `docs/plans/multi-language-expansion-plan.md`

Primary specifications this ADR reads as authority. Each statement below was
read from the cited page.

- Apple, "Defining Test Cases and Test Methods":
  <https://developer.apple.com/documentation/xctest/defining-test-cases-and-test-methods>.
  A test method is "an instance method on a subclass of XCTestCase" that
  "starts with the lowercase prefix `test`", "takes no parameters" and
  "returns no value". This ADR narrows that contract exactly the way
  ADR-0025 D6 narrowed it, and inherits the narrowing.
- Swift documentation, "Expressions" and "Types", for the token boundaries
  this frontend must reproduce: string literals with `\(` interpolation
  holes that nest arbitrary expressions including further literals;
  `"""…"""` multiline literals; backtick-quoted identifiers; and nested
  `/* … */` block comments.
- Swift Evolution SE-0355 (regex literals, Swift 5.7) and SE-0168 (multi-
  delimiter raw strings, Swift 5.0): the two constructs whose lexing is
  version-dependent within the Swift 5 line, which D4b refuses rather than
  reads.

## Context

Swift source is discovered and never decoded. ADR-0025 D1 keeps `.swift`
paths as metadata-only discovery whose bytes are never passed onward, and
forbids invoking the Swift toolchain, SourceKit, SwiftPM, Xcode, macros, and
plugins. ADR-0025 also froze a delivery ladder (D9) whose stage 3 is a
documentation/evidence-only qualification of SwiftSyntax 603.0.2, and
`docs/plans/swift-n1-qualification-handoff.md` records the pause checkpoint
after stage 2 with that qualification as the paste-ready next mission.

### The evidence ladder was read first

ADR-0025 D7 carries an evidence ladder, and its item 5 forbids, *for the
claim*, "regex/text-only matching, extension/import spelling alone,
recovered trees, unpinned snapshots or `main`, manifest evaluation,
build/test output, macro execution, runtime discovery, or structural
similarity without exact identity". ADR-0042 records why that check comes
first: ADR-0041's first version authorized a Go family without reading
ADR-0021's ladder, whose item 4 forbids the same route, and the family had
to be withdrawn.

ADR-0042 through ADR-0046 then established the precedent this ADR follows.
Each of those lanes faced a preflight ADR whose ladder named a pinned real
frontend — `go/ast`, Prism, a sandboxed PHP parser, SwiftSyntax — and each
was landed as a hand-written recursive-descent parser over a declared
subset instead, because the zero-external-dependency program constraint
admits no new crate, grammar, toolchain, or downloaded artifact. The
ladder's forbidden item is discharged by building a real parse: the anchor
becomes a question about a parsed declaration, not about text positions,
and everything outside the declared subset abstains. Swift is the last of
those lanes. Item 5's remaining prohibitions — spelling alone, recovered
trees, manifest evaluation, macro execution, runtime discovery — bind this
frontend in full: the superclass spelling is never sufficient by itself
(the import binding of D2 is also required), the parser never recovers, and
nothing is evaluated or executed.

### Why the hand-written route is sound for the admitted subset

Swift is a large language, and ADR-0025 was right that its full surface —
conditional compilation, macros, result builders, generics, accessors,
operator and keypath expressions — needs a real frontend. But the anchor
this lane needs rests on a narrow spine: an `import`, a class header, and a
method header. That spine is small, fixed, and — for the constructs this
ADR admits — lexically and structurally invariant across the Swift 5
release line. Swift's genuinely undecidable constructs are refused whole
rather than approximated: `#if` selects a branch from a build configuration
no bytes in the file fix; regex literals changed token boundaries inside
the 5.x line; attached macros and result builders execute compiler plugins.
None of them is read under an assumption.

### Relationship to the qualification handoff

`docs/plans/swift-n1-qualification-handoff.md` froze the stage-3
SwiftSyntax/compiler-differential qualification as the next Swift mission
and its pause checkpoint. **This ADR supersedes that pause checkpoint and
that next mission.** The bounded-parser route requires no toolchain, no
artifact, no sandbox, and no differential corpus, so the qualification pass
that gated the SwiftSyntax worker is no longer the prerequisite for a
Swift source unit: ADR-0042 through ADR-0046 established that a bounded
in-process parser over a declared subset is a different route than the one
those preflights gated. The handoff remains authority for its discovery
record. If a future maintainer wants the wider, identity-exact surface
ADR-0025 describes — exact `XCTest.XCTestCase` module identity, indirect
ancestry, conditional-profile selection — the SwiftSyntax worker path and
its qualification program remain exactly as ADR-0025 wrote them, deferred
and unauthorized here.

## Decision

### D1. Admission is every discovered `.swift` file

XCTest discovers tests by class shape wherever they are compiled, so like
MSTest under ADR-0043 and `matlab.unittest` under ADR-0046 there is no
runner-defined file set to narrow on. Narrowing by filename would invent a
convention and present it as evidence. Every discovered `.swift` file is
decoded, and the blast radius is bounded by what the frontend emits, in D3.

`swift-config` discovery, the static `Package.resolved` reader, and every
ADR-0025 D4/D5 inventory boundary are unchanged. This ADR adds no project
model: the frontend reads one file's own bytes and nothing else — not
`Package.swift`, not `Package.resolved`, not a test-target profile, not a
build setting.

### D2. The exact anchor

One shape is admitted:

```swift
import XCTest

final class CatalogTests: XCTestCase {
    func testLoadsCatalog() {
        XCTAssertEqual(items.isEmpty, true)
    }
}
```

Every condition is required, and each is a property of the parsed
declaration.

1. The file imports the `XCTest` module — a plain `import XCTest`, an
   `@testable import XCTest`, or a kinded form such as
   `import class XCTest.XCTestCase`, all of which bind the module named by
   the first path component.
2. A class's inheritance list carries the **bare identifier `XCTestCase`**
   as one complete entry. A qualified `XCTest.XCTestCase`, a generic
   argument, or a lookalike such as `XCTestCaseSubclass` does not count.
3. The method is declared directly in that class's body (a nested class is
   examined on its own behalf), as an instance `func` — not `static`, not
   `class`.
4. The bare name starts with the exact lowercase ASCII prefix `test` and
   carries at least one further identifier character. A backticked name and
   an operator name are not bare spellings.
5. The parameter list is exactly `()`, and the return is absent, `Void`,
   `Swift.Void`, or `()`.
6. The declaration is not under conditional compilation, in an extension,
   generic, or carrying an attribute with arguments — each of those refuses
   the whole file under D4.

`throws`, `async`, and `async throws` effects are compatible variations, as
ADR-0025 D6 already recorded. `rethrows`, parameters, non-Void returns,
operator and subscript declarations, protocol requirements, extension-added
methods, inherited methods, and Objective-C selector customization are
outside the family and are reported as bounded observations rather than
silently dropped.

This is deliberately narrower than ADR-0025 D6's anchor, which required a
semantically verified `XCTest.XCTestCase` module identity under a qualified
sandboxed verifier. That verifier remains unauthorized; what this anchor
claims is the source-visible shape bound by the file's own import, and D6
states the difference as non-claims.

### D2a. The declared subset the parser admits

The frontend is a hand-written lexer and recursive-descent parser. It
abstains; it never recovers. Outside the declared subset it records a typed
refusal for the whole file and yields no declaration, so a partial or
recovered tree can never become an anchor.

```
file        := (import | class | free_func | simple_decl | opaque_type | hash_construct)*
import      := '@testable'? 'import' kind? dotted_name
class       := attrs? modifiers? 'class' name (':' type_list)? '{' body '}'
              -- the header, from 'class' to '{', on one physical line
func        := attrs? modifiers? 'func' (name | operator) params? effects? ('->' type)?
              -- params may wrap lines inside the parentheses; the rest may not
member      := func | init | deinit | simple_decl | class
simple_decl := ('var' | 'let' | 'typealias') head-to-end-of-line   -- brace-free
opaque_type := ('struct' | 'enum' | 'actor' | 'protocol' | 'extension') head '{' body '}'
body        := <opaque token run, brace-balanced; expressions deliberately unparsed>
```

Four properties of that grammar carry weight.

**Expressions are deliberately unparsed.** A body is consumed as a
brace-balanced token run. This is what lets D6 say the frontend never
interprets a statement, and it is why the lexer — not the parser — must be
string-, comment-, and interpolation-correct (D4).

**A class body admits six member kinds and nothing else**: `func`, `init`,
`deinit`, brace-free `var`/`let`/`typealias` heads, and nested `class`.
Anything else — `subscript`, nested `struct`/`enum`, a computed property,
a keyword a later Swift release adds — leaves the declared subset, so the
extents of the members around it are unproven and the file abstains. This
is ADR-0046's release-difference answer, applied to Swift.

**File-level non-class types are consumed for extent only.** `struct`,
`enum`, `actor`, `protocol`, and `extension` bodies are skipped
brace-balanced with no claim inside them. An `extension` therefore cannot
re-open a class this frontend admitted, and a test class nested inside a
value type is a stated recall cost rather than a descent.

**Modifiers are a closed set, and `class` is disambiguated the way the
language does.** `class` is both the nested-type keyword and the
member modifier; it is read as a modifier exactly when a declaration
keyword or another modifier follows. An unknown word in modifier position
is not a modifier, so it reaches the member dispatch and abstains instead
of mis-nesting.

### D3. Only admitted declarations become units

The frontend emits a module unit per decoded `.swift` file, one
`swift_test_class` unit per admitted test class, and one
`swift_test_method` unit per admitted test method, with IR nodes and
containment edges per the shared lane substrate. Ordinary Swift code
produces no unit, so the absence of a unit is not evidence that a file has
no code. This is the bound that replaces a filename gate.

### D4. The lexer is string-, comment-, and interpolation-aware

`//` line comments, **nested** `/* … */` block comments, `"…"` literals
with `\` escapes, `"""…"""` multiline literals, and `\(` interpolation
holes that nest arbitrary expressions — including their own string
literals, tracked by an explicit bounded stack rather than recursion —
never contribute to an anchor or move a brace. Backtick-quoted names lex
as distinct tokens that never match a keyword or a bare test prefix.

This is a decision because the repository has shipped the opposite defect
repeatedly, and because Swift makes it easy: an interpolation hole may
contain braces, quotes, and comment markers as content, and a multiline
literal may contain anything at all, including lines that read exactly
like a declaration.

### D4b. Swift 5.x syntactic invariance bounds the claim

ADR-0020 gate 2 requires that the admitted parse not depend on anything
RepoGrammar cannot determine. The declared invariance set is **Swift 5.x
source-mode syntax**: a construct is admitted only when every member of
that set lexes and nests it the same way, so the frontend never selects a
compiler version, language mode, or toolchain and never needs one.

The two places where that would fail are refused rather than read:

- **Regex literals.** SE-0355 made a `/` in expression-start position open
  a regex literal from Swift 5.7; before that the same bytes lex as
  operators, and the two readings disagree about where a string starts.
  The lexer applies the language's own contextual rule — a `/` may start a
  regex exactly where a prefix expression can start, and never after an
  identifier, literal, or closing bracket — and refuses the file with
  `ConflictingFacts` under `swift_dialect_invariance`. This mirrors
  ADR-0046's `matlab_dialect_invariance` refusal shape: a named divergence
  class, a fixed note, no divergent text copied into any fact. Ordinary
  division, which follows an operand, still parses.
- **Non-ASCII bytes in code position.** Swift classifies identifier
  characters with the Unicode standard; this byte lexer would split such
  an identifier at the first multi-byte character. Comments and strings
  are unaffected, because their bytes are never classified.

Two further version-sensitive families are refused as unadmitted rather
than as divergences, because refusing them is a subset choice rather than
an invariance argument: raw strings `#"…"#` (admitted by Swift 5.0, but
this frontend carries no second delimiter grammar for them) and `#`
constructs it does not read.

Everything else the subset admits — the declaration spine, both string
forms with interpolation, nested block comments, the modifier sets, the
effects and Void spellings — is fixed across the Swift 5 line, and the
one-directional representational facts (bodies unparsed, opaque types
undescended) can only under-anchor.

### D4c. Conditional compilation bounds the claim

`#if`, `#else`, `#elseif`, or `#endif` anywhere in the file makes it
abstain whole, with `BuildVariantAmbiguity` under
`swift_conditional_compilation`. Admitting every branch would invent a
declaration the build may not contain; admitting one would assert a build
configuration nothing in the file fixes. This is the same rule ADR-0044
D4a states for Delphi and ADR-0043 D4a for VB.NET, with one deliberate
widening: where those lanes preserved anchors in unconditional regions of
the same file, Swift's inactive branches are not even fully parsed by the
real compiler, so this frontend declines to prove any boundary in a file
that uses conditional compilation at all. The abstention is wider than
strict necessity and is stated rather than dressed as one.

### D4d. The hostile-construct refusals

Attributes with non-empty argument lists (`@available(iOS 15, *)`),
generic parameters on parsed declarations (`func testFoo<T>()`,
`class Suite<T>`), accessor-bearing or closure-initializer properties
(`var x: Int { get }`, `let cb = { … }`), and result-builder-bearing
declarations — which this frontend cannot distinguish from other
attributes by spelling and therefore refuses with the attribute-argument
rule wherever one carries arguments — each refuse the whole file with
`InsufficientSupport` under `swift_syntax_admission`, plus a
degraded-parse diagnostic. SwiftSyntax, SourceKit-LSP, and every toolchain
artifact remain excluded: this list exists precisely so the declared
subset can be honest without one.

A refusal is file-level rather than declaration-level for ADR-0040 D3's
reason: once the token stream diverges, every later boundary in the file
is unproven, so a per-declaration degradation would report confident
boundaries derived from an unproven split.

### D5. What the anchor claims

It claims that a file whose own import binds the XCTest module declares a
class whose inheritance list carries the bare `XCTestCase` spelling, and
that the class declares an instance method with the test prefix, no
parameters, and a Void return — which is the shape XCTest's documented
discovery rule selects. It claims nothing about execution, outcomes,
expectations, ordering, fixtures, setUp/tearDown effects, parallelization,
or test-plan selection. No `swift`, `swiftc`, `sourcekit-lsp`, SwiftPM,
Xcode, `xcodebuild`, macro, plugin, package script, child process, or
network operation runs; ADR-0025 D3's execution prohibitions are carried
forward unchanged.

### D6. What the anchor explicitly does not claim

Four non-claims are worth naming rather than leaving to inference, because
they are the ones a reader will reach for.

- **Module-qualified superclass identity.** The anchor binds the bare
  spelling through the file's import; it does not resolve
  `XCTest.XCTestCase` identity, and a same-named class from another module
  in a file that imports XCTest would satisfy the anchor's source-visible
  conditions. Exact identity is ADR-0025 D3's semantic-verifier obligation
  and remains open. This is the widest gap between this anchor and
  ADR-0025 D6's, and it is deliberate: closing it needs the worker this
  program declines to admit.
- **Indirect ancestry and cross-file assembly.** Only the inheritance list
  of the class's own header is read. A test class whose superclass is
  itself a `XCTestCase` subclass defined elsewhere, a `class` split across
  files (not legal Swift, but not proven absent either), and methods added
  by extensions are not anchored and not resolved.
- **Execution and selection.** That the file compiles, that the test target
  includes it, that the method runs, passes, or is skipped: runtime state,
  not claimed. The SwiftPM project scope of ADR-0025 D5 remains unbuilt,
  so no test-target membership is consulted or implied.
- **Swift Testing.** `@Test` remains what ADR-0025 D6 left it: an attached
  macro whose identity cannot be inferred from spelling. It does not
  anchor, and the attribute-argument refusal never has to reason about it.

### D7. Typed uncertainty

Every abstention is a typed `UNKNOWN` with a bounded kind and a fixed
note; no divergent source text is ever copied into a fact target, note,
assumption, or diagnostic. The claims are:

| Claim | Reason | Kinds |
|---|---|---|
| `swift_syntax_admission` | `InsufficientSupport` | `unterminated_string_literal`, `unterminated_block_comment`, `unterminated_backtick_name`, `unadmitted_raw_string`, `unadmitted_non_ascii_code`, `unadmitted_attribute_argument`, `generic_declaration`, `unadmitted_accessor`, `unadmitted_member_shape`, `unadmitted_file_construct`, `unadmitted_declaration_header`, `unbalanced_block_structure`, `unadmitted_hash_construct`, `parser_resource_limit`, `source_byte_limit`, `unit_resource_limit` |
| `swift_dialect_invariance` | `ConflictingFacts` | `unadmitted_regex_literal` |
| `swift_conditional_compilation` | `BuildVariantAmbiguity` | `conditional_compilation_region` |
| `swift_xctest_class_binding` | `UnresolvedImport` | `test_methods_without_testcase_base` |
| `swift_xctest_import_binding` | `UnresolvedImport` | `testcase_without_xctest_import` |
| `swift_xctest_method_shape` | `InsufficientSupport` | `static_test_method`, `test_method_with_parameters`, `test_method_non_void_return`, `test_method_rethrows`, `free_test_function` |

None of these blocks family membership, and none need to: a refused file
yields no anchor, so the abstention has already removed everything a block
could act on. The binding unknowns are recoverable only by source-backed
facts — the import appearing, or the superclass spelling changing — never
by ambient state.

### D8. Supersession, exactly scoped

This ADR supersedes ADR-0025 **only** in that its evidence ladder item 5 no
longer closes the `swift.xctest.test_method` family against a hand-written
bounded parser, per the ADR-0041-through-ADR-0046 precedent, and in that
the stage-3 qualification pause checkpoint of
`docs/plans/swift-n1-qualification-handoff.md` is superseded as the
prerequisite route for a Swift source unit and family. Everything else in
ADR-0025 remains in force: discovery and inventory (D1, D4, D5), the
execution prohibitions (D3), the resource and supply-chain gates (D8), and
the staged delivery discipline (D9) for any future stage that needs them.
SwiftSyntax and SourceKit-LSP remain excluded, unauthorized, and
unadmitted; a future ADR may still pursue that route, and it will find
ADR-0025's qualification program intact.

### D9. No dependency

No Rust crate, no grammar, no toolchain, no downloaded artifact. The lexer
and parser are hand-written Rust in the existing core. Input is untrusted:
byte, token, unit, header, interpolation-depth, and class-recursion
ceilings all abstain with typed `UNKNOWN`s rather than degrading silently
or failing unsafely.

## Alternatives considered

- Execute the qualification handoff and admit SwiftSyntax: rejected as the
  route for this lane, because the bounded-parser precedent supplies a
  family-bearing frontend with zero supply-chain surface, and the handoff's
  own constraints ("do not edit `src/`") show it was scoped as
  research-only. It remains the route for the identity-exact surface, and
  nothing here removes it.
- Keep a text scanner and argue the anchor from positions: rejected because
  ADR-0025 D7 item 5 forbids text-only matching for the claim, and because
  the repository has shipped that defect repeatedly — anchors invented from
  strings, comments, and conditional branches. A parse is the discharge.
- Anchor on `@Test` (Swift Testing): rejected for the first family for the
  reason ADR-0025 D6 records: the marker is an attached macro, and identity
  cannot be inferred from spelling. Direct XCTest methods are the smaller
  exact slice.
- Pin one Swift version as the reference grammar: rejected because it would
  make every anchor depend on an unproven selection; D4b's invariance set
  proves the same property without selecting anything.
- Descend into `struct`, `enum`, and `extension` bodies: rejected for the
  first slice because an extension cannot re-open an admitted class in the
  source-visible sense this anchor claims, and a nested-class descent adds
  recursion surface for a rare shape. A widening is a superseding decision.
- Admit multi-line class headers: rejected for the bounded-header shape
  this lane declares; a wrapped inheritance list is legal Swift and the
  abstention is a stated recall cost, not a correctness claim.
- Abstain per declaration rather than per file on a refusal: rejected for
  ADR-0040 D3's reason — a diverged token stream unproves every later
  boundary in the same file.

## Consequences

- Swift gains a bounded frontend, owned units and IR, typed `UNKNOWN`s, a
  registered framework role, and one exact family target,
  `swift.xctest.test_method`, over one class-and-method shape. It gains no
  project model, no provider, and no toolchain.
- `.swift` bytes cross the source-store boundary. No execution, network, or
  installation is added. Recursion over untrusted input is depth-bounded.
- ADR-0025 remains in force for discovery, inventory, limits, and every
  execution prohibition; the handoff plan's next-mission contract is
  superseded as recorded above.
- A file using conditional compilation, a raw string, a regex-literal
  position, non-ASCII identifiers, an attribute with arguments, generics on
  a parsed declaration, an accessor property, or a wrapped class header
  contributes no anchors at all. Every one of those is a real recall cost
  on real repositories, and every one is the intended conservative failure
  rather than something to soften by reading under an assumption.
- Product-level family formation for the new units, the shared
  `application` classifier wiring, and the CLI-surface readiness tests are
  integration steps outside this ADR's file ownership and remain recorded
  in the completion review until they land.
- Widening the admitted anchor shape, the declared subset, the invariance
  set, or the claim surface requires a superseding ADR.
