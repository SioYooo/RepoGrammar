# One-command quickstart publication checklist

Status: **PREPARATION / UNRELEASED** for candidate `0.5.1`. The maintainer's
explicit 2026-10-15 request authorizes latest-version npm publication after CI
fixes; the [0.5.1 dual-channel checklist](stable-v0.5.1-release-checklist.md)
owns the source/tag/stage/publication gates. This document supplies its native
onboarding qualification. It records no future gate as passed and authorizes no
test writes to real user profiles or global installations.
The immutable 0.5.0 release remains unchanged. It lacks `quickstart-v1` and
cannot serve the current-source `--quickstart` flow.

## Publication decision and source identity

- Use the recorded `0.5.1` release authorization and verify that the new
  version/tag/private-stage identity is unoccupied under the canonical gate.
  Never move an existing tag or replace immutable assets.
- Synchronize Cargo.toml, Cargo.lock and the npm source manifest to that chosen
  version. Manifest agreement is not npm publication. The new `0.5.1` stable
  lane uses a private GitHub draft and OIDC npm stage; the earlier GitHub-only
  restriction remains historical to `0.5.0`.
- Update the installer stable-version hint and public version-pinned examples
  only as part of that reviewed new release; do not relabel immutable 0.5.0.
- Complete review, the [required repository gates](../development/testing.md),
  and atomic commits before the authorized release integration and tag.
- Freeze the reviewed source commit. Record its full SHA, version, local
  verification commands, results and unresolved platform limitations.

## Exact candidate artifact gate

Qualify one exact successful `release.yml` tag run and attempt at the reviewed
`v0.5.1` commit. A manual build-only run is an optional preflight, not a
substitute for the tag-bound publication candidate. Retain run id, attempt,
source SHA and the annotated tag identity. Require all four native targets:

| Artifact target | Native runtime gate |
|---|---|
| `aarch64-apple-darwin` | macOS arm64 |
| `x86_64-apple-darwin` | macOS x86_64 |
| `aarch64-unknown-linux-gnu` | Linux arm64, glibc 2.39+ |
| `x86_64-unknown-linux-gnu` | Linux x86_64, glibc 2.35+ |

Retain the four `repogrammar-<target>.tar.gz` archives, four matching `.sha256`
files, `install.sh`, `install.sh.sha256`, and `npm-candidate-manifest.json`:
eleven GitHub draft assets from that exact run. Retain the once-packed npm
tarball and npm pack metadata separately as workflow artifacts; the tarball is
not an extra GitHub release asset.
Each archive must contain the executable and `workers/python/worker.py`.
Do not rebuild or repack between review and publication. Windows and musl Linux
remain outside scope. npm stage/SRI/provenance, pinned/latest launchers and the
preserved preview tag follow the canonical `0.5.1` gate.

On each native runner, test the exact unpacked candidate binary and bundled
worker with the existing packaged-product smoke, including managed product
uninstall. Retain Linux ABI checks and the interpreter's Python 3.10+ identity.
The installer test must use the exact candidate `install.sh`, not an older
download or a wrapper from another commit. Confirm its binary answers
`quickstart-v1` to the internal capability probe.

Run the shared qualification helper on every native candidate:

```text
cargo run --quiet --locked --bin repo-guard -- smoke-quickstart-artifact --binary <unpacked/repogrammar> --release-dir <exact-current-archive-and-sidecar-dir> --out <new-evidence-dir> [--installer <exact-installer>]
```

The source installer is the default. An explicit installer must match the
source checkout's bytes/hash. The helper rejects a candidate version mismatch
before predecessor network access, verifies the supplied current archive and
sidecar, acquires the attested immutable 0.5.0 predecessor, and passes both its
binary and bundled worker to the suite. The predecessor creates a valid index
before the upgrade case. Qualification requires all **34** uniquely named
cases to PASS, no skips/failures/errors, supplied rather than synthesized release
assets, and unchanged input hashes. A partial suite is not a release gate PASS.

Retain `qualification.json`, `suite/summary.json`, suite logs, completed stage
output and predecessor asset/attestation evidence. Capture timeout/output-limit
or spawn errors may retain only a stage-labelled CI error; never invent missing
failure logs. `quickstart-<target>` workflow artifacts retain this evidence and
do not change the eleven-asset publication inventory.
Public qualification is retained inside the finalizer's
`stable-0.5.1-public-evidence-<run_id>-<run_attempt>` workflow artifact; uploads
after failed stages preserve available evidence without asserting completion.

## One-command onboarding gate

Use isolated HOME, PATH, XDG/agent profiles, managed directories and disposable
Git repositories. Do not write real agent configuration or a developer's
global installation. Execute the candidate installer against the real
candidate binary and bundled worker; a mocked product binary is insufficient.
Controlled native-agent fixtures may verify configuration and failure paths,
but do not establish a real external agent session's readiness.

Retain evidence for:

- direct execution and `cat install.sh | bash -s -- --quickstart`, with a
  controlling terminal, approval, refusal and EOF;
- one reviewed confirmation and a no-terminal run with explicit `--yes`;
  no-terminal live execution without authorization must fail before writes;
- dry-run with no persistent changes, default/named/nested Git roots,
  outside-repository behavior, `--skip-repository`, invalid project paths and
  paths containing spaces or shell characters;
- supported single-agent and auto multi-agent selection, no detected agents,
  `--target none`, missing explicit agents, `--no-instructions`, deferred local
  scope, owned current/outdated integrations and preserved foreign/drifted or
  malformed state;
- clean/repeated installation, update, rollback after acquisition or receipt
  failure, pre-existing installation preservation and explicit unmanaged
  command backup authorization. Qualify the published immutable `0.5.0`
  predecessor's verified native binary as the upgrade input; a locally renamed
  candidate or a skipped `--previous-binary` case does not prove this upgrade;
- earlier PATH commands preserved with actionable guidance, missing/old Python,
  missing/corrupt/truncated downloads, malformed/missing checksums, unsafe or
  duplicate archive entries, symlinks, permissions and ownership failures;
- fresh/stale index behavior, indexing/autosync/MCP failures, retained valid
  generations, newly created versus pre-existing agent rollback, and absence
  of duplicate entries, instructions, receipts or daemons on rerun;
- telemetry preference preservation, CLI-only/source-install compatibility,
  agent-only configuration, managed uninstall and independent repository
  `uninit` behavior.

Record native execution separately from simulated platform classification or
cross-compilation. A local macOS run does not qualify Linux or another CPU.
Do not mark a runner `PASS` merely because its archive exists.

## Authorized publication and public verification

Under the recorded maintainer request, after source/artifact gates pass:

1. Verify the new annotated `v0.5.1` tag equals the retained successful tag-run
   source SHA, inspect both private candidates, and publish the exact eleven
   reviewed assets as a stable immutable GitHub release.
2. Download the public assets independently, compare every byte with the
   retained candidate, and verify all checksums.
3. Run `gh release verify TAG --repo SioYooo/RepoGrammar --format json` and
   `gh release verify-asset TAG ASSET --repo SioYooo/RepoGrammar --format json`
   for each asset. Retain the results. A same-channel checksum alone does not
   establish publisher authenticity.
4. Test the public pinned installer and matching native archive in isolated
   environments. Exercise direct, piped, interactive and noninteractive
   quickstart, including actual product MCP self-test and repository readiness.
   The finalizer invokes the same helper with `--release-dir evidence/github-assets`,
   `--installer evidence/github-assets/install.sh` and
   `--out evidence/public-quickstart`. Its final verifier binds the 34-case PASS
   summary to the exact public archive/checksum/installer and source
   worker/harness, including verified predecessor hashes.
5. Approve the exact retained npm stage through maintainer 2FA after GitHub
   assets are publicly verified. Confirm npm SRI/provenance and pinned/latest
   launchers against `0.5.1`, preserve `preview=0.2.0-preview.0`, and run the
   read-only stable finalizer. Confirm latest-stable binary acquisition also
   resolves to the newly qualified release. Do not repair dist-tags with
   ad-hoc writes.

The preferred immutable entrypoint **after public 0.5.1 verification** is:

```bash
quickstart_tag='v0.5.1'
curl -fsSL "https://github.com/SioYooo/RepoGrammar/releases/download/${quickstart_tag}/install.sh" \
  | bash -s -- --quickstart --version "$quickstart_tag"
```

The originally proposed moving-branch entrypoint is **not yet advertised as
working**:

```text
curl -fsSL https://raw.githubusercontent.com/SioYooo/RepoGrammar/main/src/install/repogrammar-install.sh | bash -s -- --quickstart
```

It can work only after the reviewed installer is on `main` and the selected
stable release contains the compatible binary. If retained as a convenience,
document that it trusts the moving bootstrap before archive verification. Keep
the pinned release-hosted and checksum-before-execution alternatives in the
[quickstart guide](../quickstart.md#bootstrap-trust-and-a-pinned-future-entrypoint).

## Documentation promotion and evidence

Promote one-command onboarding to the primary published path only after all
applicable public verification gates pass. Update README, the general and
agent quickstarts, installation/CLI specifications and current release evidence
together. Replace future placeholders with the verified immutable tag, and
retain explicit advanced CLI-only, agent-only and repository-only paths.
Never rewrite the historical 0.5.0 evidence record.

The publication report must include source/tag/release identities, asset hashes,
retained run identity, native runner and interpreter evidence, direct/piped
transcripts, authorization and failure controls, readiness results, local
versus public evidence labels, and remaining limitations. A downloaded binary
or written config file alone is not onboarding completion. Until publication,
the accurate result is a locally verified candidate with publication pending.
