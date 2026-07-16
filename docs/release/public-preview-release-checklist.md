# Public-preview release checklist

This is the canonical maintainer gate for the first downloadable RepoGrammar
preview. It prepares `v0.2.0-preview.0` for long-term developer use; it is not
proof that a GitHub Release or npm package already exists.

## Fixed release boundary

- `Cargo.toml` and `package.json` must both contain `0.2.0-preview.0`.
- The only valid candidate tag is `v0.2.0-preview.0`, and it must point at the
  intended, reviewed commit on merged `main`.
- The preview publishes exactly four platform archives plus their checksums:
  - `repogrammar-x86_64-unknown-linux-gnu.tar.gz`;
  - `repogrammar-aarch64-unknown-linux-gnu.tar.gz`;
  - `repogrammar-x86_64-apple-darwin.tar.gz`;
  - `repogrammar-aarch64-apple-darwin.tar.gz`.
- Windows is outside the public-preview and npm platform set. Do not upload a
  Windows archive or `install.ps1`; the source-tree PowerShell path is
  contributor dogfood only and does not establish product support.
- Each archive contains the native `repogrammar` executable and
  `workers/python/worker.py`. Each archive and the published `install.sh` has a
  matching `.sha256` asset.

## Phase 1: build-only candidate evidence

1. Start the Release workflow manually with `mode=build-only` from the exact
   candidate commit. A manual dispatch must never enter either publication job.
2. Require `verify` and all four native `build` matrix entries to pass.
3. Download all four workflow artifacts. Confirm that each contains exactly one
   release archive and its checksum; do not treat source-tree binaries as
   packaged-artifact evidence.
4. On every native platform available to the maintainer, unpack the archive
   into a fresh temporary directory and use a fresh `HOME` plus a PATH exposing
   only `git` and `python3`. Execute the same smoke path enforced in the
   workflow:

   ```text
   repogrammar version
   repogrammar setup --project <fresh-project> --target auto --dry-run --no-autosync --json --progress never
   repogrammar setup --project <fresh-project> --target auto --yes --no-autosync --json --progress never
   repogrammar find --project <fresh-project> --json tests/test_one.py
   repogrammar check --project <fresh-project> --json tests/test_one.py
   ```

   The fixture contains three simple pytest test functions so a family is
   deterministic. The live setup must report
   `product_self_test_state: "passed"` and `repository_index_ready: true`.
   With no native coding-agent CLI on the isolated PATH it must also report
   `agent_query_ready: false` and `suggested_question: null`. `find` must select
   the pytest family; `check` must remain `CONTEXT_ONLY` with
   `advisory_status: "UNKNOWN"`.
5. Preserve the workflow run URL, commit SHA, artifact names, checksums, runner
   results, and any platform not independently exercised. A green build-only
   run proves build and smoke only; it proves no public availability.

## Phase 2: tag publication gate

1. Confirm GitHub authentication and repository write authority without
   exposing credentials.
2. Confirm the repository Actions secret `NPM_TOKEN` belongs to an npm identity
   allowed to publish `@sioyooo/repogrammar`. Missing credentials must fail the
   tag run in `verify`, before GitHub assets are written.
3. Re-run all required repository gates on the exact tag commit and confirm a
   clean worktree.
4. Create and push `v0.2.0-preview.0` only after the candidate commit is merged
   to `main`. Pushing the tag is the publication trigger; do not use manual
   workflow dispatch as a publish mechanism.
5. Observe the explicit staged order:
   `verify -> build -> publish_release -> publish_npm`. GitHub prerelease assets
   are created before npm publication. These services cannot publish atomically;
   if npm fails after the GitHub stage, the workflow must stay red and the
   release is a visible partial publication, not a successful release.

## Phase 3: public verification

After the tag workflow succeeds, independently verify:

- the GitHub prerelease is marked prerelease and exposes exactly the four
  archives, four archive checksums, `install.sh`, and `install.sh.sha256`;
- every downloaded checksum validates and at least one clean-machine install
  uses the published `install.sh` plus explicit preview tag;
- `npm view @sioyooo/repogrammar@0.2.0-preview.0` resolves the intended package;
- `npx @sioyooo/repogrammar@0.2.0-preview.0 version` and the documented setup
  smoke execute without Cargo on supported macOS/Linux hosts;
- npm metadata admits only `darwin`/`linux` and `x64`/`arm64`;
- release notes do not claim Windows, measured token savings, agent readiness
  from product self-test alone, or publication that was not independently
  observed.

Only after these checks may README and external launch material describe the
preview as downloadable. Until then, the truthful state is: release automation
and local candidate checks exist, but no GitHub/npm publication is verified.

## Current external boundary

This repository change does not authenticate GitHub, set `NPM_TOKEN`, run a
remote build-only workflow, create or push a tag, create a GitHub Release, or
publish npm. Those are maintainer-controlled external actions and remain open
until their URLs and command results are recorded.
