# Stable v0.2.2 Release Checklist

This checklist is the canonical gate for RepoGrammar `0.2.2`, the intended
first published stable-channel pre-1.0 release. It makes `0.2.2` the npm
`latest` version. It does not claim production readiness, 1.0 API stability,
a stable MCP API, sound static analysis, measured token savings, Windows
support, or support beyond the documented bounded language and framework
evidence.

The retained annotated `v0.2.0` tag remains at commit
`981eb9a0ab21e5cb7ea503feead4b2a350bf0471`. Its tag workflow run
`29571508953`, attempt 1, failed before a GitHub draft or npm stage existed
because the runner's `gh` version rejected the workflow's incompatible
`--paginate --slurp --jq` invocation. No GitHub `v0.2.0` Release and no npm
`@sioyooo/repogrammar@0.2.0` package were published. The tag is retained and
must never be moved, deleted, or reused.

The retained annotated `v0.2.1` tag points to
`22956a2d5dc8ef19241ae86cefbe42c6709b05a5`. Its tag workflow run
`29582156611`, attempt 1, passed the artifact and private-draft gates and left
private draft id `355686885` unpublished with the expected 11 assets. npm
staging then failed before creating any registry stage because the bare
`npm-candidate/sioyooo-repogrammar-0.2.1.tgz` argument was parsed as GitHub
shorthand rather than as a local package file. The npm stage inventory remained
empty, and neither GitHub `v0.2.1` nor npm
`@sioyooo/repogrammar@0.2.1` was published. The source correction is this
`0.2.2` patch-forward. The `v0.2.1` tag and its private draft are retained
historical candidate evidence; neither may be moved, replaced, published as
`0.2.2`, or reused as publication authority.

Publication is a two-phase process. GitHub and npm cannot be committed
atomically, so the workflow must keep npm non-public until the exact GitHub
artifacts are public and immutable. A green build-only workflow is not
publication evidence.

## Post-public finalizer execution record

Candidate run `29586694524`, attempt 1, produced the immutable `v0.2.2`
publication authority. Post-public finalizer run `29587973589` verified the
public immutable GitHub release, npm package metadata and provenance, packaged
native product, and public installer. It then failed before the final verdict
because its `npx --package` launcher commands ran from the checked-out
RepoGrammar root. npm treated the root's same-name `package.json` as the current
package without injecting the requested public package's `repogrammar` bin, so
the launcher command was unavailable. The run did not emit
`STABLE_RELEASE_READY`; the already-public tag, assets, and npm version must not
be changed or republished to conceal that verifier failure.

The correction is a verifier-only patch on `main`. The fixed workflow is
dispatched from `main`, creates an external work directory for each of the
`pinned`, `latest`, and `preview` lanes, and changes into that lane directory in
a child shell before invoking `npx`. The workflow fails visibly when its
definition is dispatched from any ref other than `main`. Its checkout remains
pinned to immutable `v0.2.2`, and the rerun remains explicitly bound to
candidate run `29586694524`, attempt 1. This preserves release-source and
candidate authority while allowing the read-only verifier orchestration to be
corrected.

## Immutable identities

- Cargo, Cargo lockfile, and npm manifest versions are exactly `0.2.2`.
- The publication candidate tag is `v0.2.2`, created at the exact current `origin/main`
  commit after all gates pass.
- npm `@sioyooo/repogrammar@0.2.2` is a new immutable version. The existing
  `0.2.0-preview.0` tarball and `preview` dist-tag are never replaced,
  unpublished, or repurposed.
- The expected final dist-tags are `latest=0.2.2` and
  `preview=0.2.0-preview.0`.
- Any correction after public npm approval is a patch-forward `0.2.3`; never
  reuse `0.2.2`, move `v0.2.2`, or replace immutable release assets. The failed
  `v0.2.0` and `v0.2.1` candidate tags are retained and non-reusable under this
  release policy.

## Product-truth gate

- Public copy calls this the first non-prerelease pre-1.0 release, not a
  production-ready or stable-API release.
- macOS arm64/x86_64 and glibc Linux arm64/x86_64 are the complete artifact
  set. No Windows artifact or support claim is present.
- Python/framework support, structural previews, discovery-only languages,
  typed `UNKNOWN`, `PARTIAL_CONTEXT`, and the current filesystem/privacy
  limitations remain explicit.
- The instruction writer's same-directory pathname race is explicitly retained
  as an unsupported hostile-concurrent-mutation scenario unless it is closed
  with the full ADR-0023 native evidence; temp+rename is not described as a
  cross-process compare-and-swap.
- Documentation links resolve and no historical preview evidence is rewritten
  as stable evidence.

## RepoGrammar instruction adoption gate

- The managed instruction block is short, marker-fenced, versioned, and
  reversible.
- After mandatory repository authority docs, initialized-repository work that
  needs a repository-local contract/convention, repeated implementation,
  framework role, or analogue comparison attempts one compact
  `repogrammar_context` preflight before CodeGraph, grep, find, or broad manual
  reads. This includes bug repair and schema/protocol/API/prompt-output or
  Meaning Contract qualification, conformance, and drift.
- The first request uses the path, symbol, framework role, or pattern question
  already present in that task. It does not require a family id.
- Pure prose, operational release/Git/environment/credential inspection,
  syntax-only YAML/config validation, and exact single-location lookup may skip
  the gate only when no contract comparison or implementation decision is
  involved. File type cannot exempt a mixed contract-conformance task. For
  covered questions, fallback is allowed after the tool is unavailable or
  RepoGrammar returns explicit `UNKNOWN`, stale, omitted, or insufficient
  evidence, and the agent records that reason.
- Successful and `PARTIAL_CONTEXT` responses consume the returned `read_plan`
  before broader discovery. Source spans are not requested by default.
- No instruction path silently runs setup, init, resync, or autosync.
- Instruction refresh has a zero-write dry run, requires explicit repository
  and file selection plus consent, updates only one complete owned marker
  block, preserves unrelated content, and refuses malformed or duplicated
  markers.
- Fresh-session acceptance evidence includes one EasyTrace
  schema/prompt/Meaning Contract task that records authority docs -> exactly one
  compact `repogrammar_context` -> returned `read_plan` -> CodeGraph detail, and
  one deterministic `UNKNOWN`/fallback task that states the fallback reason
  before CodeGraph without repeating the identical RepoGrammar call.
- Isolated fixture repositories prove current, outdated, absent, malformed,
  and foreign instruction states. A real cross-repository audit records only
  low-cardinality outcomes and never modifies a repository outside the
  explicitly approved set.
- A fresh coding-agent session given schema/prompt Meaning Contract drift in an
  initialized fixture demonstrably calls RepoGrammar before CodeGraph; an
  `UNKNOWN` fixture demonstrably records the reason and then falls back.

## Candidate build and package gate

Run the complete local quality gate on the release commit. A manual
`build-only` dispatch may then exercise the full build and smoke matrix, but it
is rehearsal only. Its artifacts are not publication candidates and must not be
promoted later.

The pushed `v0.2.2` tag run is the sole publication-candidate build. Before it
builds, `repo-guard release-source` must verify Cargo, Cargo lockfile, and npm
versions, the exact version tag, and that the tag is the current fetched
`origin/main` commit. That tag run must produce and retain as workflow
artifacts:

- four native archives and four matching SHA-256 files;
- `install.sh` and its SHA-256 file;
- one exact `sioyooo-repogrammar-0.2.2.tgz` plus its integrity and exact file
  manifest.

The immutable tag-run record binds the source commit, tag, workflow path, and
run attempt. Its platform logs record the imported GLIBC symbol floors and
smoke results; those logs are evidence, not additional release assets.

Only `npm-candidate-manifest.json` from the npm workflow artifact is copied into
the private GitHub draft. With the four archives, four archive checksums,
installer, and installer checksum, the draft has exactly 11 assets. The npm
tarball and raw pack output remain private workflow data used by the stage job,
not GitHub release assets.

Each native archive is unpacked on its native runner. With an isolated HOME and
the packaged Python worker it must pass `version`, `setup --dry-run --json`,
live repository-only setup and product MCP self-test, Pydantic fixture full and
incremental sync, `find`, advisory `check`, and autosync across at least three
poll intervals followed by a changed-file generation and clean stop.

The exact npm tarball is inspected, installed offline into an isolated prefix,
and run against local fake release assets. Repacking inside the tag run is
forbidden; the stage job consumes that exact tarball. During finalization, npm
pack metadata and SRI for the fetched public tarball must match the public
`npm-candidate-manifest.json` asset before any public package or launcher
execution.

## One-time external security configuration

Before creating the stable tag:

1. Keep GitHub Immutable Releases enabled for `SioYooo/RepoGrammar`. It was
   enabled before the failed `v0.2.0` tag run and applies to `v0.2.2`.
2. Configure an npm Trusted Publisher for GitHub Actions with owner
   `SioYooo`, repository `RepoGrammar`, workflow `release.yml`, environment
   `npm-release`, and staged publication only (`--allow-stage-publish`).
3. Protect the GitHub `npm-release` environment with a required human reviewer
   and a deployment-branch rule restricted to tags matching `v*`.
4. The publication job uses a GitHub-hosted runner, `id-token: write`, Node 24,
   and pinned npm `11.18.0`. It does not set `NODE_AUTH_TOKEN`, read
   `NPM_TOKEN`, run `npm whoami`, or call direct `npm publish`.
5. Require human 2FA for npm stage approval and disallow traditional write
   tokens for the package. Preview and stable publication use this same draft
   GitHub Release plus OIDC stage/2FA approval boundary.
6. Pin the third-party action that receives `contents: write` to the reviewed
   immutable `softprops/action-gh-release` v3.0.2 commit
   `3d0d9888cb7fd7b750713d6e236d1fcb99157228`, not a mutable tag.

## Two-phase publication

1. Merge the reviewed release source to `main` and repeat the local gate on
   exact merged HEAD. An optional manual build-only run is rehearsal only.
2. Immediately before tagging, record the live registry preflight: npm versions
   contain `0.2.0-preview.0` but not `0.2.0`, `0.2.1`, or `0.2.2`; no npm stage
   exists for `0.2.2`; and no GitHub release or draft exists for `v0.2.2`.
   This is a pre-publication gate, not a substitute for the finalizer.
3. Create and push `v0.2.2`. The tag run is the sole candidate run: it rebuilds
   and smokes the exact artifacts, creates a private 11-asset GitHub draft, and
   stages the exact npm tarball privately through trusted OIDC:

   ```text
   npm stage publish ./npm-candidate/sioyooo-repogrammar-0.2.2.tgz --access public --tag latest --provenance
   ```

4. Review the exact tag-run artifacts, checksums,
   `npm-candidate-manifest.json`, draft asset inventory, and npm stage before
   making either channel public.
   Record the tag workflow run id, successful run attempt, stage id, and
   expected integrity in the maintainer release record without exposing
   credentials. Raw stage output remains a protected workflow log, not a
   claimed artifact.
5. Publish the GitHub draft as a normal, non-prerelease release. Require
   immutable tag/assets and verify every public asset checksum and attestation.
6. The maintainer approves the exact npm stage with 2FA. This is the only step
   that makes npm `0.2.2` public.
7. Run the read-only finalizer with the tag-run id and successful run attempt.
   It fetches that immutable attempt record and requires all postconditions
   below before emitting `STABLE_RELEASE_READY`. Dispatch the workflow
   definition from `main`, but require its source checkout to remain
   `v0.2.2` and its inputs to remain candidate run `29586694524`, attempt 1.
   Each public npm launcher lane must execute from its own external working
   directory; the repository root is not a valid `npx --package` smoke cwd.

## Final public postconditions

- GitHub `v0.2.2` is public, non-prerelease, immutable, and contains exactly 11
  assets: four supported archives, their checksums, `install.sh`, its checksum,
  and `npm-candidate-manifest.json`.
- npm published versions contain both `0.2.0-preview.0` and `0.2.2`, and do not
  contain the failed, unpublished `0.2.0` or `0.2.1` candidates. Either
  candidate's presence fails closed.
- npm dist-tags are exactly compatible with `latest=0.2.2` and
  `preview=0.2.0-preview.0`.
- npm registry integrity equals the public candidate-manifest integrity and
  provenance verifies for the trusted GitHub workflow.
- The downloaded public x86_64 Linux archive passes the full packaged-artifact
  smoke with its worker and committed Pydantic fixture. The downloaded public
  `install.sh` installs that verified release into isolated directories and the
  installed command reports `repogrammar 0.2.2`.
- Pinned and unversioned public `npx` paths report `0.2.2` and each completes a
  separate live repository-only setup with `--yes --no-autosync --json` in a
  controlled no-agent environment. `@preview` still reports
  `0.2.0-preview.0`.
- Native-agent integration and fresh coding-agent instruction behavior have
  isolated pre-release/manual evidence. They are not implied by the automatic
  read-only public finalizer.

## Partial-publication recovery

| State | Required action |
|---|---|
| Candidate run failed before a draft exists | Rerun only for an external/transient failure. A source correction is `v0.2.3`; never move `v0.2.2` or manufacture authority from rehearsal artifacts. |
| Draft upload failed with no surviving draft or npm stage | A full rerun is permitted only after proving both private candidates are absent. |
| Existing draft detected during a full rerun | Fail closed; never overwrite draft assets. Rerun only failed jobs when that preserves the original candidate. |
| npm stage failed after the draft succeeded | Keep GitHub draft-only and rerun only the failed staging job. Record the successful run attempt. |
| npm staged, GitHub publish failed | Never approve npm; retry GitHub or reject the stage with 2FA. |
| GitHub public, npm awaiting approval | Report a visible partial release; retry review/approval, with finalizer still pending. |
| npm approved, final verification failed | `0.2.2` is consumed forever; deprecate if necessary and fix forward in `0.2.3`. |

A full workflow rerun must reject an existing release or draft. Rerunning only
failed staging jobs is supported because it preserves the original draft and
tag-run package artifact; the finalizer binds the successful run attempt. If
that exact candidate must instead be abandoned while both channels remain
private, reject any npm stage but retain the tag, draft, and audit evidence,
then patch forward with a new version. Never delete or reuse a failed candidate
to manufacture a clean history. No rerun may overwrite a public asset or npm
version, and every identity mismatch fails closed.
