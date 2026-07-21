# Stable v0.4.3 Release Checklist

This is the canonical two-phase publication gate for RepoGrammar `0.4.3`.
It patch-forwards from the immutable public `0.4.2` release and fixes command
path recovery after first-party shell installation while preserving the split
installation lifecycle: the public shell installer owns binary acquisition
only, coding-agent integration is optional, and `init` owns each repository's
local state, index, and autosync.

This checklist does not claim production readiness, 1.0 API stability, sound
static analysis, runtime equivalence, measured token savings, Windows support,
or any language/framework support beyond the documented bounded evidence.

## Immutable identities

- Cargo, Cargo lockfile, and npm manifest versions are exactly `0.4.3`.
- The publication tag is annotated `v0.4.3` at the exact fetched
  `origin/main` commit after all local gates pass.
- The retained npm candidate is exactly
  `sioyooo-repogrammar-0.4.3.tgz` and is never repacked before approval.
- The final public dist-tags must be exactly `latest=0.4.3` and
  `preview=0.2.0-preview.0`.
- Public inventory must retain `0.2.0-preview.0`, `0.2.2`, `0.4.0`, `0.4.1`,
  `0.4.2`, and `0.4.3`; abandoned `0.2.0`, `0.2.1`, `0.3.0`, `0.3.1`, and `0.3.2`
  remain absent.
- Any source correction after tag creation consumes `0.4.3` and requires a new
  unoccupied patch-forward version. Never move the tag or replace assets.

## Pre-tag occupancy and source gate

- Fetch `origin/main`, prune, and fetch tags. Require a clean worktree and
  `HEAD == origin/main`.
- Require no local/remote `v0.4.3` tag, no GitHub release or draft for the tag,
  no public npm `0.4.3`, and no retained private npm stage for that version.
  Public registry metadata cannot prove private stage absence; the maintainer
  checks authenticated stage state separately.
- Preserve the immutable `v0.4.2` tag, assets, npm package, provenance, and
  finalizer evidence as historical predecessor authority.

## Local release-candidate gate

Run on the exact release commit:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run --quiet --bin repo-guard -- check
cargo run --quiet --bin repo-guard -- check-diff --base origin/main --head HEAD
python3 src/workers/python/worker.test.py
node src/workers/typescript/worker.test.js
node src/npm/repogrammar.test.js
bash src/install/repogrammar-install.test.sh
npm pack --dry-run
git diff --check
cmp -s AGENTS.md CLAUDE.md
```

The gate must prove that:

- `install.sh --install-cli-only` downloads the exact platform archive and
  checksum, validates the archive, installs the binary and bundled worker, and
  creates the product receipt without creating `.repogrammar/` or agent wiring;
- optional agent wiring is a separate `repogrammar install` operation;
- after installer acquisition, a writable Conda-like directory placed first on
  PATH cannot redirect a bare `repogrammar install`; the validated receipt's
  command path remains authoritative and the earlier PATH directory is untouched;
- `repogrammar init --project <path> --yes` creates the repository index and
  starts autosync by default, while `--no-autosync` is deterministic for CI;
- release workflow, finalizer, repository guard, npm launcher, installer hint,
  README, and canonical docs all agree on `0.4.3`.

## Candidate tag workflow

1. Create annotated `v0.4.3` at exact `origin/main` and push only that tag.
2. The tag workflow reruns all gates, builds the four supported native archives,
   retains four checksums, packages `install.sh` plus checksum, packs and smokes
   the exact npm tarball, and creates one private GitHub draft.
3. The draft contains exactly 11 assets: four archives, four archive checksums,
   `install.sh`, `install.sh.sha256`, and `npm-candidate-manifest.json`.
4. The protected OIDC job runs exactly:

   ```text
   npm stage publish ./npm-candidate/sioyooo-repogrammar-0.4.3.tgz --access public --tag latest --provenance
   ```

5. Record the exact successful tag-run id/attempt, npm stage id, candidate SRI,
   tag object, and tag commit. Workflow logs are not approval authority.

## Candidate review and public approval

- Verify the tag SHA equals the merged release commit and tag-run `head_sha`.
- Verify every matrix build, packaged smoke, npm smoke, checksum, and retained
  candidate manifest from the same run.
- Publish the complete GitHub draft as a normal immutable release.
- Re-download and verify all 11 public assets and attestations.
- Approve the exact npm `0.4.3` stage through the maintainer 2FA boundary.
  CI cannot approve, reject, reuse, or inspect private stages.
- Wait for npm package bytes, SRI, provenance, inventory, and dist-tags to
  converge. Do not repair tags with ad-hoc writes.

## Read-only finalizer

Dispatch `.github/workflows/stable-release-finalize.yml` from `main` with the
exact successful candidate run id and attempt. Its checkout remains pinned to
immutable `v0.4.3`. It must verify:

- public immutable GitHub release and exactly 11 attested assets;
- retained/public npm manifest and registry SRI equality;
- provenance bound to `.github/workflows/release.yml`, `refs/tags/v0.4.3`, the
  release commit, and exact run id/attempt;
- public native archive smoke;
- public `install.sh` binary acquisition followed by a separate live
  `repogrammar init` in an isolated repository;
- pinned and `latest` npm version plus separate live repository init lanes;
- preview still resolves `0.2.0-preview.0`.

Publication is complete only when the finalizer emits exactly
`STABLE_RELEASE_READY`.

## Evidence record

The independent public verification completed on 2026-07-22 (Australia/Melbourne):

- Release commit: `c0d72bb48f0edaed0a15ea1eb7ccbd01df0fa1b0`.
- Annotated tag object: `eb4544012a0addf6ef36375d1a9893df266a29ef`;
  dereferencing `v0.4.3` resolves to the release commit above.
- Candidate workflow: [run `29870606932`, attempt `1`](https://github.com/SioYooo/RepoGrammar/actions/runs/29870606932).
- npm stage: `82c8ebae-e4de-43c8-8155-64694762d952`, approved through the
  maintainer 2FA boundary.
- Retained candidate and public npm SRI:
  `sha512-G2pzS0CAjxn1kornK2yLLgmqL/ZGuYDCMMus5Fc+UM+uWiBHFvPIzCXCRiN28Nlks/TDNYD/dLQehfXWUvQDiA==`;
  public npm shasum: `4eb8ca3116f58d3acac8071e844fe5bb105b2aac`.
- GitHub Release: [immutable `v0.4.3`](https://github.com/SioYooo/RepoGrammar/releases/tag/v0.4.3),
  normal and non-prerelease, with exactly 11 verified public assets.
- npm package: [`@sioyooo/repogrammar@0.4.3`](https://www.npmjs.com/package/@sioyooo/repogrammar/v/0.4.3),
  with SLSA provenance bound to `.github/workflows/release.yml`,
  `refs/tags/v0.4.3`, the release commit, and candidate run above.
- Dist-tags: `latest=0.4.3`, `preview=0.2.0-preview.0`.
- Public finalizer: [run `29871676832`](https://github.com/SioYooo/RepoGrammar/actions/runs/29871676832),
  verdict exactly `STABLE_RELEASE_READY`.
