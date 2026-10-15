# Stable v0.5.1 dual-channel release checklist

Status: **PREPARATION / UNRELEASED**. This is the canonical candidate and
publication gate for RepoGrammar `0.5.1`; no future build, stage or public
verification is recorded as passed by this document.

On 2026-10-15 the maintainer explicitly requested publication of the latest
version to npm after the remaining CI fixes. That instruction authorizes this
new dual-channel release and supersedes the earlier GitHub-only restriction
for this release. It does not change the immutable
[0.5.0 GitHub-only record](stable-v0.5.0-release-checklist.md), republish that
version, or satisfy the separate [v0.6 milestone](../plans/v0.6-adoption-retrieval-release-plan.md).
The chosen patch-forward identity is `0.5.1`, subject to the occupancy checks
below. Publication proceeds under the recorded request once the gates pass;
maintainer npm authentication/2FA remains a separate service boundary.

## Scope and non-claims

The release packages the reviewed integrated source, including the one-command
quickstart and bounded existing-family query repair. It adds no claim of new
language completion, provider semantics, agent adoption, token savings,
performance equivalence or v0.6 completion. Python remains the official bounded
focus, strict Top-20 remains 0/20, and existing limitations and the open PHP
admission-contract finding remain disclosed in the
[capability report](../reports/language-support/evidence-driven-capability-review.md).
Windows and musl Linux remain outside public installation scope.

The npm package is a launcher: by default its version selects the same-version
GitHub release assets. Publishing npm `0.5.1` without qualified public
`v0.5.1` native assets is not a usable release.

## Source identity and occupancy

- Preserve unrelated changes, historical tags/assets/packages and prior
  evidence. Synchronize `Cargo.toml`, `Cargo.lock`, `package.json`, current
  release-policy constants, installer version hint and candidate documentation
  to `0.5.1`; do not mass-replace historical 0.5.0 records.
- Finish review, CI fixes, required local gates and atomic commits. Integrate
  the exact reviewed candidate into `main`, fetch `origin/main`, and require a
  clean release checkout with `HEAD == origin/main` before tagging.
- Confirm that local/remote annotated tag `v0.5.1`, GitHub release/draft and
  public npm `0.5.1` are unoccupied. Public registry metadata does not establish
  absence of a private npm stage; check authenticated stage state separately
  without exposing credentials.
- Record GitHub release immutability and the npm Trusted Publisher binding to
  this repository, `release.yml` and protected `npm-release` environment.
  Workflow text alone is not proof that the remote binding is configured.
- Create a new annotated `v0.5.1` at that exact commit and push only the intended
  tag after source gates pass. Record both tag-object and dereferenced commit
  SHAs. A source correction after tagging requires a new unoccupied version;
  never move the tag or replace retained/public assets.

## Local and native candidate gates

Run the [required repository checks](../development/testing.md) on the exact
candidate. At minimum retain:

```text
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
python3 src/workers/python/worker.test.py
node src/workers/typescript/worker.test.js
node src/npm/repogrammar.test.js
bash src/install/repogrammar-install.test.sh
npm pack --dry-run
cargo run --quiet --locked --bin repo-guard -- check
cargo run --quiet --locked --bin repo-guard -- check-diff --base <reviewed-base> --head HEAD
git diff --check
cmp -s AGENTS.md CLAUDE.md
```

Preserve the retrieval, payload and full sync-equivalence evidence for the
integrated source; rerun affected checks when source changes invalidate it.
Missing or skipped required checks are not PASS. Local pack/dry-run evidence
does not establish public npm availability.

The new annotated tag triggers `release.yml`. Manual dispatch remains
build-only and may qualify a preflight candidate, but publication evidence must
come from one exact successful tag run and attempt, bound to the reviewed
commit. Do not substitute an unspecified latest run or pre-tag build artifacts.

Require all four native archive jobs, their packaged-product smoke with
`--require-product-uninstall`, and the Linux ABI checks:

| Target | Native gate |
| --- | --- |
| `aarch64-apple-darwin` | macOS arm64 |
| `x86_64-apple-darwin` | macOS x86_64 |
| `aarch64-unknown-linux-gnu` | Linux arm64, glibc 2.39+ |
| `x86_64-unknown-linux-gnu` | Linux x86_64, glibc 2.35+ |

Each archive contains `repogrammar` and `workers/python/worker.py` from the
same source, and each runtime records Python 3.10+ admission. Complete the
[one-command onboarding gates](quickstart-release-checklist.md) on the exact
candidate installer/binary/worker, including direct and piped execution,
TTY/non-TTY authorization, lifecycle preservation and actual product MCP/index
readiness. An archive, cross-compile or mocked product alone is not a native
runtime PASS.

Each native job runs:

```text
cargo run --quiet --locked --bin repo-guard -- smoke-quickstart-artifact --binary <unpacked/repogrammar> --release-dir <exact-current-archive-and-sidecar-dir> --out <new-evidence-dir>
```

The helper verifies candidate version before predecessor network access,
checks the supplied current release assets, and qualifies upgrade from the
attested immutable 0.5.0 binary with its paired worker and an existing old index.
All 34 cases must PASS without skips; hashes bind the binary, both workers,
installer, current archive/sidecar and harness. Retain the success
`qualification.json`, full suite summary and available stage evidence as the
separate `quickstart-<target>` workflow artifact. Failed/timeout runs cannot
produce qualification PASS, and transport failures may lack partial logs.

## One frozen npm candidate and private draft

The tag workflow packs once with `npm pack --json --ignore-scripts`, retaining
`sioyooo-repogrammar-0.5.1.tgz`, npm pack metadata and its candidate manifest.
`smoke-npm-package` and `verify-npm-pack-evidence` must pass on that exact
tarball before staging. The expected package files are exactly `LICENSE`,
`README.md`, `package.json` and `src/npm/repogrammar.js` beneath `package/`.
Freeze the README before this pack: its version, npm/native prerequisites,
quickstart behavior, evidence limits and links must match the candidate.
Do not repack to alter README or metadata after review.

The same successful tag run creates one non-overwritable private GitHub draft
with exactly **11** assets:

- four `repogrammar-<target>.tar.gz` archives;
- their four `.sha256` sidecars;
- `install.sh` and `install.sh.sha256`;
- `npm-candidate-manifest.json` copied from the retained npm candidate.

Retain all asset hashes, candidate SHA-512/SRI, run id/attempt and source SHA.
The npm tarball remains a retained workflow artifact, not a twelfth GitHub asset.
Both preview and stable tag paths must reach draft creation; merely enabling a
stable stage whose prerequisite draft job is preview-only cannot publish.

## OIDC stage and maintainer approval

The protected `npm-release` job uses Node 24, npm 11.18.0 and
`id-token: write`, downloads the once-built tarball, rechecks its smoke/pack
evidence and runs exactly:

```text
npm stage publish ./npm-candidate/sioyooo-repogrammar-0.5.1.tgz --access public --tag latest --provenance
```

CI may stage through Trusted Publisher OIDC only. It must not use a traditional
npm token, repack, directly publish, approve/reject a stage, or mutate dist-tags.
Record the exact stage id and candidate integrity. A staging failure does not
prove that no stage was created: authenticated maintainer inspection must
establish whether an existing stage matches the retained bytes before any
retry or approval. Keep successful candidate jobs intact; do not rerun them to
replace an occupied draft or reserved package version.

Review the exact private draft and stage, publish the complete GitHub draft as
an immutable normal release, verify its public assets, then approve the exact
npm stage through the maintainer 2FA boundary. Publishing GitHub first makes
the matching binary assets available before the npm launcher becomes public.
This authentication step is not replaced by workflow logs or local validation.

## Public verification and finalizer

- Independently download all 11 public GitHub assets, compare retained/public
  bytes, verify SHA-256 sidecars, tag/source/run identity, normal immutable
  release state, `gh release verify`, and every `gh release verify-asset`.
- Fetch public npm `0.5.1` with scripts disabled. Before executing it, require
  package file inventory, SHA-512 and registry SRI equality with the frozen
  tarball and identical retained/GitHub/public candidate manifests.
- Verify npm signatures/provenance against `release.yml`, `refs/tags/v0.5.1`,
  the release commit and exact candidate run id/attempt. Require public
  `latest=0.5.1` and unchanged `preview=0.2.0-preview.0`, retaining historical
  published versions including npm 0.4.3 and excluding abandoned candidates.
- Run the exact public native archive, pinned installer and npm pinned/latest
  launcher in isolated external working directories and profiles. Exercise
  actual repository initialization, bundled worker, MCP readiness and managed
  lifecycle. Do not run `npx --package` from this checkout, where its same-name
  package can mask the public package. Verify preview remains unchanged.
- The public finalizer must rerun `smoke-quickstart-artifact` with the downloaded
  current archive directory and explicit downloaded installer. Require
  `evidence/public-quickstart/qualification.json` and its unchanged-input,
  supplied-asset 34-PASS summary; its verifier binds the exact public assets,
  source worker/harness and predecessor evidence. Do not substitute the earlier
  native-build evidence for this public-byte check.
- Dispatch `stable-release-finalize.yml` from `main` with the exact successful
  candidate run id and attempt. Its verification checkout remains pinned to
  immutable `v0.5.1`; it is read-only and delegates the final evidence decision
  to `repo-guard verify-stable-release-evidence`.
  Retain its `stable-0.5.1-public-evidence-<run_id>-<run_attempt>` workflow
  artifact. The upload runs even after failure, so inspect the verdict and
  completeness rather than treating artifact existence as success.

Publication is complete only when the public checks and finalizer establish
**STABLE_RELEASE_READY**. `GITHUB_RELEASE_READY`, stage creation, an npm page,
or local green tests alone are insufficient. Once public evidence passes,
update current publication/quickstart claims together; preserve the immutable
0.5.0 and earlier release records and the distinct v0.6 NO_GO.

## Evidence status

| Gate | State at checklist creation |
| --- | --- |
| Maintainer request and new dual-channel scope | Recorded, 2026-10-15 |
| Exact release source/CI and version occupancy | Pending candidate evidence |
| Four native candidates and quickstart qualification | Pending candidate evidence |
| Frozen npm pack, private GitHub draft and OIDC stage | Pending candidate evidence |
| Public GitHub bytes and maintainer npm approval | Not established |
| Public npm SRI/provenance/pinned/latest smoke | Not established |
| Read-only finalizer | Not established |

Append exact identities, commands, results and remaining limitations as each
gate executes. Do not replace these pending states with inferred success.

## Preflight evidence, 2026-10-15

Reviewed source: `2f5ef5d625bc6f87be3ee7e8287e5d2450c6197f`, on
`chore/release-v0.5.1` in [PR #33](https://github.com/SioYooo/RepoGrammar/pull/33).
This snapshot records candidate evidence only; subsequent documentation-only
commits do not change the source identity of these runs.

- [PR CI 38729942001](https://github.com/SioYooo/RepoGrammar/actions/runs/38729942001)
  and [push CI 38729911640](https://github.com/SioYooo/RepoGrammar/actions/runs/38729911640)
  both passed the required quality, macOS and Linux checks. The existing
  Windows source-only diagnostic failed and remains nonblocking under the
  repository's deferred-Windows policy.
- [Release preflight 38729992347, attempt 1](https://github.com/SioYooo/RepoGrammar/actions/runs/38729992347)
  completed successfully at the same source SHA. It was a `workflow_dispatch`
  build-only run: draft creation and both npm stage jobs were skipped. It is
  not the authority for a future tag publication.
- All four native archive jobs passed packaging, product lifecycle smoke and
  the supplied-archive quickstart/verified-0.5.0-upgrade gate. Both Linux ABI
  checks passed. The four `quickstart-<target>` artifacts retain the native
  qualification and full suite summaries separately from the release assets.
- The once-packed npm candidate passed smoke and pack-evidence checks. Its
  retained tarball has exactly the four required files; independent local
  inspection confirmed their bytes match the reviewed checkout and the
  SHA-512/SRI match the retained manifest.

Preflight npm integrity (not a public registry assertion):

```text
sha512-BwiKREDXByAzNvlBoknOg+AWpZVTa88RfOy9DfMNNyIhI5TGLosV5Id+GyXWIVyKmV0WxKh7UTfDvLoK7qvR9Q==
```

Required local checks passed, including 2,598 Rust tests (two existing
benchmark tests ignored), worker/npm tests, the installer compatibility suite,
repository/GEO guards, guide equality and diff checks. The frozen retrieval
corpus remained 97/110 overall, 34/47 retrieval and 36/36 correct abstention,
with zero wrong-family selections; full sync-equivalence passed all 14
scenarios. These results do not close the distinct v0.6 claims.

The maintainer deferred npm login. Authenticated private-stage occupancy and
the live Trusted Publisher binding remain unverified; no new tag, GitHub
draft, npm stage or public package has been created. Resume the tag and
publication gates above after authentication, using one exact tag run and
attempt rather than these preflight artifacts.
