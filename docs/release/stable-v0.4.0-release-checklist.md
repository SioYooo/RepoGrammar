# Stable v0.4.0 Release Checklist

This checklist is the canonical two-phase publication gate for RepoGrammar
`0.4.0`. It advances the stable source identity from the development-only
`0.4.0-preview.0` manifests and patch-forwards from the retained, abandoned
`v0.3.2` candidate. The public stable predecessor remains `0.2.2` until both
GitHub and npm publication complete and the read-only finalizer emits
`STABLE_RELEASE_READY`.

This release does not claim production readiness, 1.0 API stability, a stable
MCP API, sound static analysis, measured token savings, runtime equivalence,
Windows support, or support beyond the documented bounded language and
framework evidence. Product claims remain governed by the product
specification, limitations, product-core verdict, and launch kit; this
checklist does not expand them.

## Retained v0.3.2 abandonment evidence

- Annotated tag `v0.3.2` remains at commit
  `26ce59e098d18f3c7ca976de4c76ff13643285f5` and must not be moved, deleted,
  or reused.
- Tag workflow run `29654842298`, attempt 1, built and smoked the npm candidate,
  installer, four native archives, checksums, and the private 11-asset GitHub
  draft. Its source classification and release verification jobs passed.
- The protected `Stage immutable npm stable launcher` job executed no steps and
  was cancelled. The overall run is therefore `cancelled`, not a successful
  candidate run usable by the stable finalizer.
- GitHub retains a private, non-immutable `v0.3.2` draft. It is historical
  candidate evidence, not a public GitHub Release.
- The public npm registry contains only `0.2.0-preview.0` and `0.2.2`; it does
  not contain `0.3.2`. Public dist-tags remain
  `preview=0.2.0-preview.0` and `latest=0.2.2`.
- Before tagging `v0.4.0`, the maintainer must separately inspect authenticated
  npm stage state. A private stage cannot be inferred from public registry
  metadata. Any retained `0.3.2` stage must be rejected through the human 2FA
  boundary; CI must not inspect, approve, reuse, or reject it.

The failed or abandoned stable versions `0.2.0`, `0.2.1`, `0.3.0`, `0.3.1`,
and `0.3.2` are permanently non-reusable. They must remain absent from the
public npm version inventory, and their tags or drafts must never be used as
authority for `0.4.0` bytes.

## Immutable v0.4.0 identities

- Cargo, Cargo lockfile, and npm manifest versions are exactly `0.4.0` on the
  release commit.
- The only publication tag is annotated `v0.4.0`, created at the exact fetched
  `origin/main` commit after all release gates pass.
- The tag-run npm candidate is exactly
  `sioyooo-repogrammar-0.4.0.tgz`; it is packed once, retained as workflow
  artifact data, and never repacked before staging or publication.
- The expected final public dist-tags are `latest=0.4.0` and
  `preview=0.2.0-preview.0`.
- The exact tag SHA, candidate run id and successful attempt, npm stage id,
  candidate integrity, GitHub Release URL, npm URL, and finalizer run are
  recorded after those events occur. Placeholders are not publication proof.
- Any source or candidate-byte correction after `v0.4.0` is created is a new,
  unoccupied patch-forward version. Never move `v0.4.0`, replace assets, or
  reuse `0.4.0` after public npm approval.

## Source and claim gate

- The worktree is clean and `HEAD == origin/main` after fetch.
- `git tag --list v0.4.0`, GitHub Release/draft lookup, public npm versions,
  authenticated npm stage inspection, and npm dist-tags prove that `0.4.0` is
  unoccupied before tagging.
- Cargo, Cargo.lock, npm, installer hint, release workflow, finalizer,
  repository guard, tests, and canonical installation docs agree on `0.4.0`.
- Every `dtolnay/rust-toolchain` workflow step uses reviewed implementation
  commit `4cda84d5c5c54efe2404f9d843567869ab1699d4` and explicitly requests
  `toolchain: stable`; mutable third-party action refs fail the repository guard.
- The public preview remains the immutable `0.2.0-preview.0`; no stable action
  mutates or replaces that version.
- Release-facing documents distinguish source-ready, tagged candidate, public
  GitHub, public npm, and finalizer-verified states.
- Estimated read displacement remains `ESTIMATED`, never measured token
  savings. Every static-alignment certificate keeps
  `runtime_equivalence: "UNKNOWN"`.
- Historical `CONTEXT_ONLY` and preview transcripts are labeled historical and
  are not presented as current protocol evidence.
- macOS arm64/x86_64 and glibc Linux arm64/x86_64 are the complete public
  platform set. No Windows artifact or public-support claim is present.

## Local release-candidate gate

Run on the exact release commit:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run --quiet --bin repo-guard -- check
cargo run --quiet --bin repo-guard -- check-diff --base origin/main --head HEAD

RC_PRODUCT_OUT="$(mktemp -d)"
RC_SYNC_OUT="$(mktemp -d)"
RC_PAYLOAD_OUT="$(mktemp -d)"

cargo run --quiet --bin repo-guard -- product-eval \
  --corpus src/fixtures/evaluation/query-corpus-v1.json \
  --out "$RC_PRODUCT_OUT" --repetitions 1 \
  --bin target/debug/repogrammar --condition v0_4_0_rc
cargo run --quiet --bin repo-guard -- sync-equivalence \
  --fixture src/fixtures/incremental_equivalence/v1 --all \
  --bin target/debug/repogrammar --out "$RC_SYNC_OUT"
cargo run --quiet --bin repo-guard -- payload-measure \
  --fixture src/fixtures/evaluation/payload-measure \
  --bin target/debug/repogrammar --out "$RC_PAYLOAD_OUT"
python3 src/workers/python/worker.test.py
node src/workers/typescript/worker.test.js
node src/npm/repogrammar.test.js
bash src/install/repogrammar-install.test.sh
npm pack --dry-run
git diff --check
cmp -s AGENTS.md CLAUDE.md
```

Retain the three printed temporary output paths in the RC evidence ledger; do
not commit their generated artifacts. A manual `build-only` workflow
dispatch is rehearsal only and its artifacts can never become publication
candidates.

The local gate also proves:

- the exact npm tarball contains only the four allowed package files and
  reports version `0.4.0`;
- each archive includes the product binary and bundled Python worker;
- installer and npm wrapper smokes use isolated directories and local fake
  release assets where appropriate;
- release-source classification requires exact manifest agreement and exact
  `v0.4.0` at current `origin/main` for a tag push;
- stable staging uses the one registered explicit local tarball path and no
  traditional npm token, direct `npm publish`, approval, rejection, or dist-tag
  mutation authority;
- the finalizer is read-only and pinned to `v0.4.0`.

## Candidate tag workflow

Only after the local gate and preflight pass:

1. Create annotated tag `v0.4.0` at exact fetched `origin/main` and push it
   without moving any prior tag.
2. Treat that tag run as the sole candidate build. It must re-run the complete
   verify gate, build four native archives, retain four archive checksums,
   retain `install.sh` plus checksum, pack and smoke the exact npm tarball, and
   create one private GitHub draft.
3. The GitHub draft must contain exactly 11 assets: four archives, four archive
   checksums, `install.sh`, `install.sh.sha256`, and
   `npm-candidate-manifest.json`. The npm tarball and raw pack output remain
   private workflow data.
4. The stable OIDC job checks out `v0.4.0`, downloads the retained npm artifact,
   re-verifies its manifest and SRI without repacking, and runs exactly:

   ```text
   npm stage publish ./npm-candidate/sioyooo-repogrammar-0.4.0.tgz --access public --tag latest --provenance
   ```

5. Record the exact successful candidate run id and attempt, stage id, and
   integrity outside public logs containing credentials or raw stage details.

The Trusted Publisher identity is exact: owner `SioYooo`, repository
`RepoGrammar`, workflow `release.yml`, environment `npm-release`, GitHub-hosted
runner, Node 24, pinned npm `11.18.0`, `id-token: write`, and stage-only
publication. The protected environment and npm approval keep their required
human reviewer and 2FA boundaries.

## Candidate review gate

Before either private candidate becomes public, verify:

- tag SHA equals the merged release commit and candidate run `head_sha`;
- all platform jobs and package smokes passed in the same tag run;
- the draft's exact 11-asset inventory matches the locally retained filenames,
  sizes, and checksums;
- public GitHub release/asset attestations are not a pre-publication input and
  are verified only after immutable publication;
- `npm-candidate-manifest.json` matches the retained npm tarball filename,
  version, four-file allowlist, SHA-512, and SRI;
- the authenticated npm stage is the exact retained tarball and no failed or
  abandoned version is public;
- product, README judge path, demo rehearsals, claim audit, and release-source
  audit all passed on the release commit.

## Two-phase public publication

1. Publish the reviewed GitHub draft as a normal, non-prerelease release for
   exact `v0.4.0`, with Immutable Releases enabled.
2. Re-download all 11 public assets and verify every checksum and release/asset
   attestation. The release page alone is insufficient evidence.
3. The maintainer approves the exact npm `0.4.0` stage with 2FA. No workflow or
   coordinator bypasses this human boundary.
4. Wait for public npm metadata, tarball, integrity, provenance, and dist-tags
   to converge. Do not repair tags with ad-hoc `npm dist-tag` writes.
5. Dispatch `stable-release-finalize.yml` from `main` with the exact successful
   `v0.4.0` candidate run id and attempt. The verifier source checkout remains
   pinned to immutable `v0.4.0`.
6. Accept publication only when the finalizer emits exactly
   `STABLE_RELEASE_READY`.

## Final public postconditions

- GitHub `v0.4.0` is public, non-prerelease, immutable, and exposes exactly 11
  verified assets.
- npm versions include `0.2.0-preview.0`, `0.2.2`, and `0.4.0`, and exclude
  `0.2.0`, `0.2.1`, `0.3.0`, `0.3.1`, and `0.3.2`.
- npm dist-tags are exactly `latest=0.4.0` and
  `preview=0.2.0-preview.0`; no extra tag is accepted by the finalizer.
- Public npm SRI equals the retained candidate manifest and provenance binds to
  `.github/workflows/release.yml`, `refs/tags/v0.4.0`, the exact tag commit,
  and the recorded run id/attempt.
- The public native x86_64 Linux archive and installer pass isolated smoke, and
  the installed command reports `repogrammar 0.4.0`.
- Exact pinned and unversioned npm launcher lanes each report `0.4.0` and pass
  live repository-only setup in separate external work directories.
  `@preview` still reports `0.2.0-preview.0`.
- README judge commands and the current demo runbook pass in disposable
  workspaces without relying on the local source checkout, PATH binary, or npm
  cache.

## Recovery matrix

| State | Required action |
|---|---|
| Source defect before tag | Fix on the release branch, rerun all gates, merge, and tag only the corrected exact `origin/main`. |
| Tag run fails before a draft exists | Rerun only a proven transient external failure; a source correction consumes `v0.4.0` and requires patch-forward. |
| Existing draft detected | Fail closed. Never overwrite draft assets; rerun only failed jobs when that preserves the original candidate. |
| npm staging fails after draft creation | Keep GitHub draft private; inspect stage state under authenticated maintainer identity and retry only the allowed failed job. |
| npm staged, GitHub publication fails | Do not approve npm; retry GitHub publication or reject the stage through the human 2FA boundary. |
| GitHub public, npm pending | Report a visible partial release and continue the exact approval/recovery path; finalizer remains pending. |
| npm public, finalizer fails | `0.4.0` is consumed forever. Preserve evidence, deprecate only if warranted, and patch-forward to a new version. |

No recovery path may move a tag, replace immutable bytes, approve or reject a
stage from CI, unpublish a version, force-push, or claim success without the
public finalizer.

## Evidence record

Fill these only from verified external state:

- release commit: `<pending merge>`
- tag and tag SHA: `<pending v0.4.0 tag>`
- candidate run id / attempt: `<pending successful v0.4.0 tag run>`
- npm stage id and integrity: `<pending protected stage>`
- GitHub Release URL: `<pending public immutable release>`
- npm package URL: `<pending public npm package>`
- finalizer run: `<pending STABLE_RELEASE_READY>`
- public installer / pinned npm / latest npm / preview smoke:
  `<pending public finalizer>`

These pending fields are release gates, not claims that publication has
occurred. After finalization, replace them with exact evidence and keep only the
human video, `/feedback`, and Devpost actions pending in the submission kit.
