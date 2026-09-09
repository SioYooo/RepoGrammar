# Top-20 language and third-party analysis baseline — 2026-08-01

- Branch: `feat/top-20-language-library-support`
- Baseline commit: `86dba38ada7fe5646b5ab8770e2ad4183e8d22d7`
- Authority: ADR-0020 and ADR-0030
- Verdict: `0/20` ranked languages complete; TypeScript extra incomplete

## Evidence policy

This report distinguishes discovery, syntax/IR substrate, exact-family
substrate, and complete language support. A positive value below means direct
current implementation evidence for that column only. A partial value is
reusable substrate with known missing obligations. No row has passed the
nine-item ADR-0020 completion gate.

`G1` discovery/config; `G2` authoritative frontend; `G3` owned IR; `G4` typed
`UNKNOWN`; `G5` exact family; `G6` completion fixture matrix; `G7` source-free
readiness; `G8` four-part review; `G9` linked atomic audit.

| Lane | Language | G1 | G2 | G3 | G4 | G5 | G6 | G7 | G8 | G9 | Current strict status |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| C0 | Python | partial | yes | yes | partial | yes | partial | partial | no | no | incomplete |
| C0 | C | partial | no | yes | partial | no | no | partial | no | no | incomplete |
| C0 | C++ | partial | no | yes | partial | yes | partial | partial | no | no | incomplete |
| C0 | Java | partial | no | yes | partial | yes | partial | partial | no | no | incomplete |
| C0 | C# | partial | no | yes | partial | yes | partial | partial | no | no | incomplete |
| C0 | JavaScript | partial | partial | yes | partial | yes | partial | partial | no | no | incomplete |
| C0 | Rust | partial | no | yes | partial | yes | partial | partial | no | no | incomplete |
| X0 | TypeScript (extra) | partial | partial | yes | partial | yes | partial | partial | no | no | incomplete-extra |
| N1 | Go | partial | no | no | no | no | no | partial | no | no | discovered_only |
| N1 | PHP | partial | no | no | no | no | no | partial | no | no | discovered_only |
| N1 | Swift | partial | no | no | no | no | no | partial | no | no | discovered_only |
| N1 | Ruby | partial | no | no | no | no | no | partial | no | no | discovered_only |
| N2 | Visual Basic | no | no | no | no | no | no | no | no | no | not_started |
| N2 | Delphi/Object Pascal | no | no | no | no | no | no | no | no | no | not_started |
| N2 | Ada | no | no | no | no | no | no | no | no | no | not_started |
| N2 | Fortran | no | no | no | no | no | no | no | no | no | not_started |
| N3 | SQL | no | no | no | no | no | no | no | no | no | not_started |
| N3 | R | no | no | no | no | no | no | no | no | no | not_started |
| N3 | MATLAB | no | no | no | no | no | no | no | no | no | not_started |
| N4 | Assembly | no | no | no | no | no | no | no | no | no | not_started |
| N4 | Scratch | no | no | no | no | no | no | no | no | no | not_started |

## Current provider and dependency truth

- The optional provider registry has three slots. TypeScript is integrated;
  Python type-provider and rust-analyzer slots are not integrated. There are no
  Clang, javac/JDT, Roslyn, or new-language provider slots.
- Cargo metadata is an implemented, bounded project-model adapter. It is not a
  Rust code semantic frontend.
- Existing Python, TS/JS, C/C++, Java, C#, and Rust framework analysis is useful
  exact-anchor substrate but does not satisfy the provider/project-model and
  completion-review requirements.
- Go, PHP, Swift, and Ruby are bounded source/config inventory only. The product
  parser explicitly rejects all eight source/config tokens as unsupported.
- Visual Basic, Delphi/Object Pascal, Ada, Fortran, SQL, R, MATLAB, Assembly, and
  Scratch have no language token, discovery, parser, fixture, or review in the
  baseline.
- Before ADR-0030, dependency data was ecosystem-specific and mostly string
  facts. There was no shared dependency snapshot, external-symbol identity, or
  reviewed library-contract type.

## Predeclared next modules and stop conditions

| Workstream | Next smallest truthful module | Stop condition |
|---|---|---|
| Shared third-party layer | Owned dependency/external-symbol/contract model, then persistence and generic provider port | Any raw manifest fact is promoted beyond its evidence level |
| TS/JS | One bounded `Program`/`TypeChecker` external-symbol query with JS/TS mode and unresolved/resolved pair | Worker needs unrestricted filesystem/network/package installation |
| Python | One candidate-scoped third-party import/type identity operation | Provider cannot be isolated or version/provenance cannot be established |
| Rust | One bounded external-crate item identity operation; keep cfg/macro/build boundaries unknown | build script/proc macro execution becomes necessary |
| C/C++ | Compilation-command candidate plus isolated Clang identity query, reported separately for C and C++ | header language/dialect or TU cannot be pinned |
| Java | Static Maven metadata first, then one bounded javac/JDT symbol query | build/annotation processor execution becomes necessary |
| C#/VB.NET | Static project metadata plus isolated Roslyn supplied-source query | MSBuild/source-generator execution becomes necessary |
| Go | Separate OS-sandbox qualification before frontend/IR | filesystem/network/descendant/CPU/memory isolation is unproven |
| PHP | ADR-0024 stage-3 qualification only | artifact/differential/five-target/native sandbox evidence incomplete |
| Swift | ADR-0025 stage-3 documentation/evidence qualification only | exact XCTest identity requires build/index/Package.swift evaluation |
| Ruby | ADR-0022 stage-3 dependency and sandbox qualification only | exact Prism/CRuby/artifact/native sandbox evidence incomplete |
| N2–N4 | One decision-only preflight per language before tokens or dependencies | dialect/version/frontend/acquisition/license/sandbox/family/UNKNOWN not pinned |

## Allowed negative outcomes

`NO_GO`, `BLOCKED`, `INCONCLUSIVE`, provider unavailable, environment failure,
and source-evidence-insufficient are valid outcomes. Discovery-only, syntax-only,
or one positive fixture is never reported as language completion. Delphi and
MATLAB begin evidence-blocked; that is not permission to substitute an
unreviewed grammar or an incompatible language implementation.

## Baseline validation

- `cargo test --workspace --all-features`: passed (1573 library tests, 105
  repo-guard tests, 118 CLI tests, one doctest; two existing ignored tests).
- `git diff --check`: passed before implementation.
- The four referenced N1 completion-review files and
  `docs/reports/unknown-resolution-sota-analysis.md` were absent at the baseline;
  this is evidence drift, not proof that their gates passed.
