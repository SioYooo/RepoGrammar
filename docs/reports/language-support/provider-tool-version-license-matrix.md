# Provider, tool, version, and license matrix

Date: 2026-08-01. This is an admission matrix, not a claim that every listed
candidate is installed or approved. `Not integrated` and `NO_GO` rows execute
nothing in default indexing.

| Lane/tool | Version identity | License evidence | Product status | Exact boundary/non-claim |
|---|---|---|---|---|
| CPython `ast`/`symtable` worker | runtime version recorded per response; no universal Python version | host CPython license; no new vendored parser | integrated authoritative syntax slice | not Pyrefly/Pyright type/import semantics |
| Python type provider (Pyrefly primary; Pyright cross-check) | no pinned artifact | not qualified for admission | `not_integrated` provider slot | ports/planner only; no provider facts |
| TypeScript compiler API | TypeScript 6 family proposed; no compiler package available in this checkout during audit | upstream Apache-2.0; exact artifact not pinned here | protocol/adapter integrated, compiler availability optional | dependency-free fallback is structural and cannot promote support |
| Node runtime for bundled TS worker | package requires Node `>=18`; actual runtime environment-dependent | runtime not bundled by this Rust change | available host runtime, not semantic evidence | runtime presence is not TypeScript provider proof |
| Cargo metadata | host Cargo version recorded by provider when used; no fixed global version | Rust toolchain MIT OR Apache-2.0 upstream; host tool not bundled | integrated bounded project-model adapter | `--no-deps` manifest facts, not Rust source semantics or resolved graph |
| Tree-sitter runtime | 0.26.11 | MIT, verified from locked local crate metadata | integrated syntax substrate | candidate generation, never sole semantic oracle |
| Tree-sitter C | 0.24.2 | MIT | integrated syntax substrate | no Clang translation-unit semantics |
| Tree-sitter C++ | 0.23.4 | MIT | integrated syntax substrate | no template/include/ABI authority |
| Tree-sitter Java | 0.23.5 | MIT | integrated syntax substrate | no javac/JDT/classpath/effective-model authority |
| Tree-sitter C# | 0.23.5 | MIT | integrated syntax substrate | no Roslyn/MSBuild/assembly authority |
| Tree-sitter Rust | 0.24.2 | MIT | integrated syntax substrate | no rustc/rust-analyzer/macro/trait authority |
| Clang/clangd/libclang | no pin | license/supply chain not qualified in this program | no provider slot | C and C++ translation-unit/external-symbol obligations remain open |
| javac/JDT | no pin | license/supply chain not qualified | no provider slot | Java classpath/module/processor semantics remain open |
| Roslyn C#/VB | no pin | license/supply chain not qualified | no provider slot | C# project/NuGet and VB source semantics remain open |
| Go standard-library parser/type stack | no pinned Go toolchain | toolchain/license artifact not qualified | sandbox preflight only | no Go command or source frontend runs |
| `mago-syntax` | candidate 1.43.0 | `MIT OR Apache-2.0` recorded by ADR-0024 | qualification incomplete | no production dependency or PHP family |
| `nikic/PHP-Parser` / PHP CLI differential | candidate 5.8.0 / PHP 8.5.8 | candidate licenses/artifacts require final qualification | evidence-only candidate | not a production provider; current host lacked required PHP oracle |
| SwiftSyntax / Swift toolchain | candidate 603.0.2 / Swift 6.3.3 | license, signatures, closure, and redistribution matrix incomplete | qualification incomplete | no Swift dependency, worker, SourceKit, SDK, or XCTest proof |
| `ruby-prism` / CRuby | candidate 1.9.0 / CRuby 4.0.6 | Prism MIT recorded; native closure/advisories still incomplete | qualification incomplete | no native parser admitted; ambient Ruby 2.6.10 is not oracle evidence |
| Libadalang/GNAT | no admitted version | license/toolchain/project-provider matrix incomplete | `NO_GO` for current zero-read lane | no Ada source/project provider |
| Flang/f18 | no admitted version | LLVM licensing known upstream; exact artifact/closure not qualified | `NO_GO` for current zero-execution lane | prescan/include/preprocess behavior violates current boundary |
| GNU `as` profile | candidate behavior documented against 2.46, x86-64 ELF AT&T | external GNU tool not bundled or executed | internal lexical scanner only | extension and spellings do not prove dialect, instruction validity, or symbols |
| MATLAB/Code Analyzer | no admitted release/provider | proprietary licensed tool; redistribution and headless isolation unresolved | not integrated | `mpackage.json` static metadata only; no MATLAB/Octave execution |
| R parser/runtime/languageserver | no pin | not qualified | no provider | `.R` source remains zero-read |
| SQL grammar/database/catalog provider | no pin | not qualified | no provider | no database/client/migration execution or dialect selection |
| Scratch ZIP/deflate + VM | no admitted dependency | dependency/license/security qualification absent | product `NO_GO`; disconnected stored-entry preflight | no `.sb3` discovery, common deflate, VM, extension execution, or family |

## Admission conclusions

- Only checked-in/internal workers, CPython syntax extraction, Cargo metadata,
  and the locked Tree-sitter substrate are active implementation evidence.
- Candidate version and license notes are auxiliary preflight evidence. They do
  not authorize acquisition, execution, or a production dependency.
- Missing exact version, checksum, license closure, sandbox, or provenance is a
  blocker, never permission to use an ambient tool silently.
