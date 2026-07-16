# Public-preview install proof matrix

- Evidence date: 2026-07-16
- Candidate version: `0.2.0-preview.0`
- Candidate tag: `v0.2.0-preview.0`
- Candidate code commit tested locally: `2a09e9a18dd1ce10194d75e80a3066430edb1f59`
- Release checklist: `../release/public-preview-release-checklist.md`

This report is an evidence snapshot, not a publication claim. The current
state proves one native packaged candidate locally and proves that release
automation is configured for four supported targets. It does not prove that
the four remote artifacts, a GitHub prerelease, or an npm package exist.

## Platform matrix

| Target | Release matrix | Packaged smoke | Public download | Verdict |
|---|---:|---:|---:|---|
| macOS arm64 (`aarch64-apple-darwin`) | yes | passed locally | no | native candidate verified |
| macOS Intel (`x86_64-apple-darwin`) | yes | not run locally | no | remote build-only required |
| Linux x86-64 (`x86_64-unknown-linux-gnu`) | yes | not run locally | no | remote build-only required |
| Linux arm64 (`aarch64-unknown-linux-gnu`) | yes | not run locally | no | remote build-only required |
| Windows | no | not applicable | no | unsupported; no artifact claim |

The Windows PowerShell installer is a source-checkout contributor path only.
It fails closed for release installation unless `-FromSource` is explicit. The
release matrix, npm launcher metadata, README, and installation specification
therefore agree that this preview supports macOS and Linux only.

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

## Publication truth

| Gate | Observed state | Consequence |
|---|---|---|
| GitHub CLI authentication | configured account token is invalid | branch push, metadata update, workflow dispatch, PR, and tag remain blocked |
| Local `NPM_TOKEN` / `NODE_AUTH_TOKEN` | absent | no local npm authority proof |
| npm registry package lookup | `E404` for `@sioyooo/repogrammar` | npm installation is not available |
| Remote build-only workflow | not run for this candidate | four-platform artifact proof is incomplete |
| GitHub prerelease | not created | no release download is claimed |
| tag | not created | `v0.2.0-preview.0` is reserved, not published |

The tag workflow checks version agreement and authenticates `NPM_TOKEN` with
`npm whoami` before writing GitHub release assets. Manual dispatch is build-
only and cannot enter publication jobs. Publication remains explicitly staged
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

Until every step is recorded, the truthful release verdict is
`LOCAL_CANDIDATE_ONLY`.
