# Public-preview install proof matrix

- Evidence date: 2026-07-16
- Candidate version: `0.2.0-preview.0`
- Candidate tag: `v0.2.0-preview.0`
- Setup/MCP smoke commit tested locally: `2a09e9a18dd1ce10194d75e80a3066430edb1f59`
- Integrated dogfood commit tested locally: `d64f861799d0fa77bcceb66f2c3d9428fbebf1e1`
- Release checklist: `../release/public-preview-release-checklist.md`

This report is a pre-tag evidence snapshot, not a current availability or
publication claim. Availability must always be rechecked for the exact version
with the commands in `../quickstart.md`. The recorded run proves one native
packaged candidate locally and that release automation is configured for four
supported targets; it is not evidence about later external state.

## Platform matrix

| Target | Release matrix | Packaged smoke | Public download | Verdict |
|---|---:|---:|---:|---|
| macOS arm64 (`aarch64-apple-darwin`) | yes | passed locally | no | native candidate verified |
| macOS Intel (`x86_64-apple-darwin`) | yes | not run locally | no | remote build-only required |
| glibc 2.35+ Linux x86-64 (`x86_64-unknown-linux-gnu`) | yes | not run locally | not observed on evidence date | remote build-only + ABI inspection required |
| glibc 2.39+ Linux arm64 (`aarch64-unknown-linux-gnu`) | yes | not run locally | not observed on evidence date | remote build-only + ABI inspection required |
| Windows | no | not applicable | no | unsupported; no artifact claim |

The Windows PowerShell installer is a source-checkout contributor path only.
It fails closed for release installation unless `-FromSource` is explicit. The
release matrix, npm launcher metadata, README, and installation specification
therefore agree that this preview supports macOS and Linux only.

The npm manifest cannot use a root `libc: glibc` restriction without also
making the same package inapplicable on Darwin, where npm reports no libc.
The manifest therefore limits OS/CPU only; the launcher and shell installer
are the fail-closed Linux glibc family/version authorities before download.

## Local packaged-artifact proof

The macOS arm64 binary was built with the same target, archive layout, and
smoke sequence as `.github/workflows/release.yml`. The binary and Python worker
were copied into `dist/`, archived, unpacked into a new temporary directory,
and executed with a fresh `HOME` and a PATH exposing only `git` and `python3`.
The archive was temporary and was not committed.

| Evidence | Result |
|---|---|
| Archive | `repogrammar-aarch64-apple-darwin.tar.gz` |
| Archive SHA-256 | `8940c1880a2a3e31ec99954fc0a5dbf211ab25c606b8ad222721323611760f2d` |
| Archive size | 3,860,015 bytes |
| `repogrammar version` | `repogrammar 0.2.0-preview.0` |
| dry-run setup | `status: dry_run`; repository not claimed ready |
| live setup | `ready_with_limitations` |
| product MCP self-test | `passed` |
| repository index | ready |
| coding-agent query readiness | false; no suggested question |
| `find` | `ok`; selected a pytest family |
| `check` | `CONTEXT_ONLY`; advisory `UNKNOWN` |

This proves repository-only CLI/index use from one packaged native artifact.
It deliberately does not turn product self-test success into native coding-
agent integration readiness.

The later integrated candidate at
`d64f861799d0fa77bcceb66f2c3d9428fbebf1e1` was separately packaged with the
same native archive layout. Its 3,892,030-byte archive had SHA-256
`fde8de2613ce9ef122a13b635b67bc8a14b89ca76e403ffb03b01db78aef3bbc`.
From that unpacked archive, self-dogfood, the frozen public FastAPI repository,
and a one-file dynamic control each completed `init`, `sync`, `find`, `check`,
and `stats`. This is integrated index/query evidence; the setup and MCP rows
above belong to the explicitly recorded earlier commit and must not be silently
attributed to the later archive.

## Pre-tag external observations on 2026-07-16

| Gate | Observed state | Consequence |
|---|---|---|
| GitHub CLI authentication | configured account token is invalid | branch push, metadata update, workflow dispatch, PR, and tag remain blocked |
| Local `NPM_TOKEN` / `NODE_AUTH_TOKEN` | absent | no local npm authority proof |
| npm registry package lookup | `E404` for `@sioyooo/repogrammar` | npm installation is not available |
| Remote build-only workflow | not run for this candidate | four-platform artifact proof is incomplete |
| GitHub prerelease | not created | no release download is claimed |
| tag | not created | `v0.2.0-preview.0` is reserved, not published |

The tag workflow checks version agreement, proves tag containment in
`origin/main`, and authenticates `NPM_TOKEN` with `npm whoami` before writing
GitHub release assets. Manual dispatch is build-only even from a tag ref and
cannot enter credential or publication jobs. Publication remains explicitly staged
as verify, build, GitHub prerelease, then npm; a failure after GitHub asset
creation remains a visible failed partial publication.

## Remaining release gate

1. Restore GitHub CLI authentication and push the reviewed candidate branch.
2. Merge only after required CI passes.
3. dispatch `Release` in `build-only` mode from the exact merged candidate;
4. download and verify all four archives and checksums;
5. confirm the repository `NPM_TOKEN` can publish the scoped package;
6. create `v0.2.0-preview.0` only from that verified `main` commit;
7. independently verify GitHub assets, npm metadata, `npx`, and a clean install.

The verdict for this dated evidence snapshot is `LOCAL_CANDIDATE_ONLY`; it must
not be reused as the current registry-availability verdict.
