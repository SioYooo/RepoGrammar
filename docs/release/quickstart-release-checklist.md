# One-command quickstart publication checklist

Status: **UNRELEASED**. This document authorizes no push, merge, tag, GitHub
Release, npm publication, global installation or user configuration write.
The immutable 0.5.0 release remains unchanged. It lacks `quickstart-v1` and
cannot serve the current-source `--quickstart` flow.

## Publication decision and source identity

- Obtain maintainer authorization for publication and an unused new stable
  version. Do not silently choose or occupy a tag, and never move an existing
  tag or replace immutable assets.
- Synchronize Cargo.toml, Cargo.lock and the npm source manifest to that chosen
  version. Manifest agreement is not npm publication. Preserve the current
  GitHub-only stable channel and the disabled stable npm staging job.
- Complete review, the [required repository gates](../development/testing.md),
  and atomic local commits. Push/merge requires its own authorization.
- Freeze the reviewed source commit. Record its full SHA, version, local
  verification commands, results and unresolved platform limitations.

## Exact candidate artifact gate

Run the existing `release.yml` build-only workflow at the authorized reviewed
commit. Retain the exact successful run id, attempt and source SHA. Require
all four native targets:

| Artifact target | Native runtime gate |
|---|---|
| `aarch64-apple-darwin` | macOS arm64 |
| `x86_64-apple-darwin` | macOS x86_64 |
| `aarch64-unknown-linux-gnu` | Linux arm64, glibc 2.39+ |
| `x86_64-unknown-linux-gnu` | Linux x86_64, glibc 2.35+ |

Retain the four `repogrammar-<target>.tar.gz` archives, four matching `.sha256`
files, `install.sh`, and `install.sh.sha256`: ten assets from that exact run.
Each archive must contain the executable and `workers/python/worker.py`.
Do not rebuild or repack between review and publication. Windows, musl Linux,
npm tarballs and dist-tag changes are outside this release gate.

On each native runner, test the exact unpacked candidate binary and bundled
worker with the existing packaged-product smoke, including managed product
uninstall. Retain Linux ABI checks and the interpreter's Python 3.10+ identity.
The installer test must use the exact candidate `install.sh`, not an older
download or a wrapper from another commit. Confirm its binary answers
`quickstart-v1` to the internal capability probe.

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
  command backup authorization;
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

Only after the source/artifact gates and separate publication authorization:

1. Bind the new tag to the retained build's exact source SHA and publish the
   exact ten reviewed assets as a stable immutable GitHub release.
2. Download the public assets independently, compare every byte with the
   retained candidate, and verify all checksums.
3. Run `gh release verify TAG --repo SioYooo/RepoGrammar --format json` and
   `gh release verify-asset TAG ASSET --repo SioYooo/RepoGrammar --format json`
   for each asset. Retain the results. A same-channel checksum alone does not
   establish publisher authenticity.
4. Test the public pinned installer and matching native archive in isolated
   environments. Exercise direct, piped, interactive and noninteractive
   quickstart, including actual product MCP self-test and repository readiness.
5. Confirm latest-stable acquisition resolves to the newly qualified release.
   Confirm existing npm metadata/dist-tags remain untouched.

The preferred immutable entrypoint, after replacing the placeholder with the
newly verified tag, is:

```bash
quickstart_tag='REPLACE_WITH_NEW_QUICKSTART_RELEASE_TAG'
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
