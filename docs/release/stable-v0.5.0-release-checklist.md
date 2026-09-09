# v0.5.0 GitHub-only Release Checklist

The maintainer authorized GitHub-only publication on 2026-09-09. Publish the
compiled Linux/macOS binaries and installer on GitHub. Do not stage or publish
npm, change npm dist-tags, or require npm 2FA for this release. The existing npm
package tests and local pack remain compatibility checks.

Status: `GITHUB_RELEASE_READY`, verified 2026-09-09. The [public release](https://github.com/SioYooo/RepoGrammar/releases/tag/v0.5.0) is immutable and contains ten verified assets.
This does not establish measured battery savings, runtime equivalence, or additional language support.

## Source and build gate

- Preserve user changes, untracked work, and all historical releases.
- Cargo, Cargo.lock, and package.json must agree on `0.5.0`.
- Run the repository's required fmt, clippy, full Rust tests, repo-guard,
  Python/TypeScript worker tests, npm launcher tests, installer tests,
  `npm pack --dry-run`, `git diff --check`, and mirrored-guide equality check.
- Merge and synchronize the reviewed release commit to `origin/main`.
- Confirm `v0.5.0` has no local/remote tag or GitHub release/draft. Never replace
  an occupied version or rewrite an existing tag.
- Confirm `stage_npm_stable` retains its explicit `&& false` guard. Tag pushes
  must not accidentally stage an npm candidate. Preview gates remain unchanged.

Dispatch the existing four-platform build:

```bash
gh workflow run release.yml --repo SioYooo/RepoGrammar --ref main -f mode=build-only
```

Record the exact successful run id, run attempt, and full source SHA. Require
`event=workflow_dispatch`, `conclusion=success`, and the reviewed source SHA.
All four native builds must pass their packaged-product smoke, including product
uninstall; both Linux builds must pass their declared glibc floor checks.
The installer artifact and its checksum must come from this same run.

## Retained candidate gate

Download these artifacts from that exact run, never from an unspecified latest
run:

- `repogrammar-x86_64-unknown-linux-gnu.tar.gz`
- `repogrammar-aarch64-unknown-linux-gnu.tar.gz`
- `repogrammar-x86_64-apple-darwin.tar.gz`
- `repogrammar-aarch64-apple-darwin.tar.gz`
- `repogrammar-installer`

The publication inventory is exactly ten files: four native archives, their
four `.sha256` sidecars, `install.sh`, and `install.sh.sha256`. Verify every
checksum. Exclude npm tarballs and `npm-candidate-manifest.json` from this
GitHub-only inventory. Preserve retained bytes and run identity for comparison;
do not repack or rebuild the candidate.

## Publish the exact candidate

Create annotated `v0.5.0` at the exact successful build's source SHA. Upload only
the ten reviewed files to a normal, non-prerelease GitHub draft and inspect the
inventory before publishing it immutably. Automatic draft creation is limited
to preview tags, and stable npm staging is explicitly disabled. A stable tag
push can repeat verification/build jobs but cannot create a competing draft or
stage npm. Use only the retained successful build-only run's artifacts.

Record the annotated tag object, dereferenced commit, release id/URL, and
publication timestamp. Any source correction after tagging requires a new
unoccupied version; never move the tag or replace public assets.

## Public verification gate

- Download the ten public assets into a fresh directory.
- Require exactly the expected inventory, immutable normal release state, and
  tag commit equality with the retained build source SHA.
- Compare every public asset byte-for-byte with its retained counterpart and
  verify all SHA-256 sidecars.
- Run `gh release verify v0.5.0 --repo SioYooo/RepoGrammar --format json` and
  `gh release verify-asset v0.5.0 <asset> --repo SioYooo/RepoGrammar --format json`
  for every public asset; retain the results.
- Unpack the matching public native archive and run:

```bash
cargo run --quiet --locked --bin repo-guard -- smoke-packaged-artifact \
  --binary "$UNPACKED/repogrammar" \
  --worker "$UNPACKED/workers/python/worker.py" \
  --fixture src/fixtures/python/release/v0_1/pydantic-basic/schemas.py \
  --expected-version 0.5.0 \
  --require-product-uninstall
```

- Run the downloaded, verified public installer in an isolated HOME and install
  directories, verify `repogrammar 0.5.0`, and initialize an isolated fixture
  repository with `--yes --no-autosync`. Keep the real machine installation and
  working repository untouched during this verification.
- Read public npm metadata before and after publication to confirm it was not
  changed; do not attempt to repair or reconcile dist-tags.

Record `GITHUB_RELEASE_READY` only after these gates pass. The historical
`STABLE_RELEASE_READY` dual-channel finalizer requires npm evidence and is not
this release's completion criterion. Report npm explicitly as not published.

## Evidence record

- Source commit: `881e0d0b243e9c1884413929d1cc55830051e369`.
- Annotated tag object: `827cdb8bdf85ca9b50c240c4f4167b3e2d921dba`.
- Successful [build-only run 34314166404](https://github.com/SioYooo/RepoGrammar/actions/runs/34314166404), attempt 1; all four native packaged-product smokes and Linux ABI gates passed.
- [Immutable GitHub release](https://github.com/SioYooo/RepoGrammar/releases/tag/v0.5.0), id `385246396`, published `2026-09-09T05:29:13Z`.
- All ten public assets matched retained bytes, passed SHA-256 verification, and passed `gh release verify-asset`; `gh release verify` passed.
- The public macOS arm64 archive passed packaged-product/uninstall smoke. The downloaded public installer installed `repogrammar 0.5.0` in a canonical temporary HOME/data/command layout; `init --yes --no-autosync --progress never --json` returned `initialized`, available storage, and `gen-000001`.
- npm was not published or retagged: `latest=0.4.3`, `preview=0.2.0-preview.0` before and after.
- Full asset digests and verification notes: [machine-readable summary](stable-v0.5.0-release.summary.json).
- Verdict: `GITHUB_RELEASE_READY`.
