# Build Week Demo Runbook

This is the canonical recording and rehearsal plan for the OpenAI Build Week
demo. It uses a real, permissively licensed FastAPI repository at an immutable
commit and the exact public RepoGrammar `0.4.0` npm package. The target cut is
90–120 seconds with spoken audio and must remain under three minutes.

The demo shows one narrow product claim: RepoGrammar gives Codex
repository-local, source-backed implementation-family context and a bounded
read plan before broad source inspection. It does not replace source reads. It
also rejects stale evidence, keeps runtime equivalence unknown, and abstains by
type when the repository cannot support a stronger answer.

## Recording truth gates

Do not record the final take until every applicable gate passes:

- [ ] GitHub Release `v0.4.0` is public and immutable, and npm
      `@sioyooo/repogrammar@0.4.0` is public with verified provenance and
      integrity.
- [ ] The pinned and unversioned public `npx` smokes pass outside the
      RepoGrammar checkout; this runbook always uses the pinned package.
- [ ] The target checkout resolves exactly to
      `4d3d5e92c1ea6b3fa0fab02c41124844ec45bca8`.
- [ ] `codex mcp get repogrammar` and Codex `/mcp` show the RepoGrammar server
      connected before any MCP claim is recorded.
- [ ] The active Codex instruction guide is known explicitly and its managed
      RepoGrammar section reports `current`; RepoGrammar must not guess the
      path.
- [ ] Codex calls `repogrammar_context` with `find_analogues` before broad
      source reads, consumes the returned `read_plan`, and does not request
      source spans by default.
- [ ] The current live output shows `measurement_kind: ESTIMATED` and the
      not-measured caveat wherever potential read displacement is visible.
- [ ] The static-alignment certificate uses a current status token and carries
      `runtime_equivalence: "UNKNOWN"`; it never says `CONTEXT_ONLY`, `PASS`,
      `CONFORMS`, or runtime-equivalent.
- [ ] The stale probe is rejected before sync, and the final unresolved query
      returns a typed `UNKNOWN` with a recovery action.
- [ ] All terminal output is generated live. Do not paste, fabricate, or splice
      output from another build, package, repository, or rehearsal.
- [ ] A spoken line discloses the agent-study pilot result: RepoGrammar was
      connected but proactively adopted in **0/4 A3 runs**. This guided the
      explicit managed instruction and prompt used here; the demo is not
      evidence of spontaneous adoption.
- [ ] The final YouTube URL works signed out and the video remains under 03:00.

If any release or identity gate fails, stop. Do not fall back to a source build
for the final judge/recording path and do not describe an unpublished package as
public.

## Target repository and attribution

- Repository: `fastapi/full-stack-fastapi-template`
- URL: <https://github.com/fastapi/full-stack-fastapi-template>
- Commit: `4d3d5e92c1ea6b3fa0fab02c41124844ec45bca8`
- License at that commit: MIT, copyright 2019 Sebastián Ramírez
- Task: add a read-only `GET` summary endpoint for one item while following the
  existing `SessionDep`, `CurrentUser`, and `response_model` conventions.

The target source is cloned only into a disposable workspace. Do not copy the
repository, generated patch, raw Codex transcript, credentials, or target build
artifacts into RepoGrammar.

## Prerequisites

- macOS or supported Linux platform with Git, Node.js/npm, `jq`, Docker with
  Compose, and a signed-in Codex CLI;
- network access to GitHub, npm, and the public RepoGrammar release assets;
- enough disk space for the FastAPI template containers;
- the exact active global Codex instruction file identified by the maintainer;
- no private source, secrets, notifications, usernames, or absolute home paths
  visible in the recording.

Use a fresh shell and set only the disposable paths. `DEMO_CODEX_GUIDE` must be
the actual guide used by the Codex installation; do not assume the example path
is correct.

```bash
DEMO_ROOT="$(mktemp -d)"
DEMO_REPO="$DEMO_ROOT/full-stack-fastapi-template"
DEMO_CODEX_GUIDE="/absolute/path/to/the/active/codex/AGENTS.md"
DEMO_NPM_CLI_CACHE="$DEMO_ROOT/npm-cli-cache"
DEMO_REPOGRAMMAR_NPM_CACHE="$DEMO_ROOT/repogrammar-npm-cache"
export npm_config_cache="$DEMO_NPM_CLI_CACHE"
export REPOGRAMMAR_NPM_CACHE_DIR="$DEMO_REPOGRAMMAR_NPM_CACHE"
```

## 1. Release-phase identity and target identity

The same behavioral flow has two distinct release gates. Do not collapse their
evidence or claims.

### 1A. Pre-tag RC gate: local exact candidate artifacts

Before tagging, the Test Agent runs the complete flow twice using artifacts
built from the clean, merged `0.4.0` source candidate:

- the exact retained `sioyooo-repogrammar-0.4.0.tgz` produced by `npm pack`;
- the current platform's packaged native archive and checksum, built from the
  same source SHA and placed in one read-only local release directory;
- a fresh npm launcher cache and disposable target clone for each rehearsal.

This is release-candidate evidence, not a public-install claim. It must not say
that npm, GitHub Release, provenance, or a registry package is public. Record the
source SHA, tarball SHA-256, archive SHA-256, and `repogrammar version` result in
the release checklist before the tag gate.

Set the Release Agent-provided absolute artifact paths and define the RC-only
launcher. `REPOGRAMMAR_RELEASE_DIR` makes the exact local npm tarball consume the
packaged archive and checksum instead of a public GitHub asset.

```bash
RC_TARBALL="/absolute/path/to/sioyooo-repogrammar-0.4.0.tgz"
RC_RELEASE_DIR="/absolute/path/to/the/current-platform-release-assets"
RC_NPM_CACHE="$DEMO_ROOT/rc-npm-cache"
test -f "$RC_TARBALL"
test -d "$RC_RELEASE_DIR"

rc_repogrammar() {
  REPOGRAMMAR_RELEASE_DIR="$RC_RELEASE_DIR" \
  REPOGRAMMAR_NPM_CACHE_DIR="$RC_NPM_CACHE" \
    npx --yes --package "$RC_TARBALL" repogrammar "$@"
}

rc_repogrammar version
```

For each pre-tag rehearsal, use the shared target setup in section 1C, then use
`rc_repogrammar` with the exact RepoGrammar arguments in sections 2–10. Do not
mix in the registry package, a PATH binary,
the source-tree binary, another tarball, another native archive, or a cache from
the other rehearsal. The function above expands every invocation to the exact
local npm tarball plus its packaged binary assets.

### 1B. Post-public gate and final recording: pinned registry package

Every RepoGrammar CLI invocation in this runbook resolves the exact npm package;
there is no ambient `repogrammar` binary or mutable npm tag in the command.

```bash
npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar version
```

The version output must be exactly the public stable `0.4.0` identity.

### 1C. Shared pinned target checkout

Both RC and public rehearsals clone and pin the target independently:

```bash
git clone https://github.com/fastapi/full-stack-fastapi-template.git \
  "$DEMO_REPO"
git -C "$DEMO_REPO" checkout --detach \
  4d3d5e92c1ea6b3fa0fab02c41124844ec45bca8
git -C "$DEMO_REPO" rev-parse HEAD
git -C "$DEMO_REPO" status --short
```

Required checkpoints:

- `rev-parse` prints the pinned 40-character commit;
- target status is clean and detached;
- the target `LICENSE` contains the MIT grant and the attribution above.

## 2. Setup Codex, the repository, and autosync

First inspect the chosen instruction file without writing it. A `foreign` or
`malformed` result is a stop condition: preserve it and repair manually outside
the recording. A missing or exact known older RepoGrammar section may be
previewed and synchronized explicitly.

```bash
npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar instructions status \
  --file "$DEMO_CODEX_GUIDE" --json

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar instructions sync \
  --file "$DEMO_CODEX_GUIDE" --dry-run --json

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar instructions sync \
  --file "$DEMO_CODEX_GUIDE" --yes --json

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar setup --project "$DEMO_REPO" \
  --target codex --yes --progress never
```

Setup must report repository initialization, indexing, autosync, product MCP
self-test, and Codex integration as separate facts. It must not enable
telemetry. If Codex integration is foreign, malformed, or drifted, stop rather
than overwriting it.

Verify all three integration layers:

```bash
codex mcp get repogrammar

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar instructions status \
  --file "$DEMO_CODEX_GUIDE" --json

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar doctor --project "$DEMO_REPO" --json
```

Required checkpoints:

- the native Codex entry is named `repogrammar` and launches `serve`;
- managed instruction state is `current`;
- repository state has a readable active index and is query-ready;
- autosync is enabled and running, or the doctor output gives the exact recovery
  action to run before continuing.

Restart Codex after setup/instruction synchronization. Already-open Codex MCP
children and instruction snapshots do not hot-swap. In the restarted session,
open `$DEMO_REPO`, run `/mcp`, and show `repogrammar` connected. Use `/model` to
select an available GPT-5.6 family model; do not hardcode an account-specific
model slug.

## 3. Exact Codex task prompt

Paste this prompt unchanged. It deliberately requires RepoGrammar because the
small agent-study pilot observed **0/4 proactive MCP adoption in its A3 arm**.
The pilot proved harness mechanics only and made no effect claim.

```text
In this repository, add a new read-only endpoint that returns a lightweight
summary for a single item. It must be a GET route whose path ends in `/summary`
for a single item id, live in the same router module as the existing item detail
endpoints, and follow this repository's existing conventions for that router:
obtain the database session through SessionDep, enforce the CurrentUser
dependency and existing item-ownership rule, and declare a response_model.
Name the route handler `read_item_summary` so the post-sync static-alignment
query can address its exact indexed member.

Before any broad search or source reading, call the read-only RepoGrammar MCP
tool repogrammar_context exactly once with operation `find_analogues`, mode
`compact`, and target `backend/app/api/routes/items.py`. Do not request source
spans. State the returned status and follow its read_plan. Read every item marked
source_required_before_edit before editing. If RepoGrammar returns UNKNOWN,
FALLBACK, stale, omitted, ambiguous, or insufficient evidence, state that reason
and use bounded ordinary source reads; do not turn candidate context into a
family or conformance claim.

Make the smallest coherent patch, add or update the focused item-route test, and
do not add dependencies or files unless the repository's conventions require
them. Finish by summarizing the source spans read, files changed, and validation
run. Write explanatory text in English.
```

## 4. Expected agent checkpoints

The recording must show behavior, not fixed hashes or a memorized transcript.
Exact family ids, generations, line ranges, hashes, token estimates, and member
counts may vary with the release and must not be hardcoded.

1. `find_analogues` is the first repository-context tool call.
2. A supported result identifies relevant FastAPI route-family evidence and a
   bounded `read_plan`. A partial or unknown result remains acceptable if Codex
   states the reason and follows the recovery path.
3. Codex reads the target body and every read-plan item marked required before
   editing. Typical bounded source reads include:
   `backend/app/api/routes/items.py`, the relevant response models in
   `backend/app/models.py`, and the focused tests in
   `backend/tests/api/routes/test_items.py`.
4. The patch stays in the existing item route/model/test surfaces, preserves
   the current-user ownership rule, and introduces no dependency.
5. RepoGrammar output is described as source-backed context plus remaining read
   obligations, never as proof that the patch works.

## 5. Validate the target patch

Review the patch before running the repository's own test path:

```bash
git -C "$DEMO_REPO" status --short
git -C "$DEMO_REPO" diff --check
git -C "$DEMO_REPO" diff -- \
  backend/app/api/routes/items.py \
  backend/app/models.py \
  backend/tests/api/routes/test_items.py

docker compose -f "$DEMO_REPO/compose.yml" \
  -f "$DEMO_REPO/compose.override.yml" up -d --wait
docker compose -f "$DEMO_REPO/compose.yml" \
  -f "$DEMO_REPO/compose.override.yml" cp \
  "$DEMO_REPO/backend/tests" backend:/app/backend/tests
docker compose -f "$DEMO_REPO/compose.yml" \
  -f "$DEMO_REPO/compose.override.yml" exec backend \
  pytest -x tests/api/routes/test_items.py
```

If the selected implementation does not need a model change, an empty
`backend/app/models.py` diff is expected. Test success is target-repository
runtime evidence; RepoGrammar's later certificate remains static-only. The
explicit `compose cp` is required because the target's production-oriented
backend image does not copy its test tree.

## 6. Controlled stale-evidence rejection and explicit sync

Stop autosync so the stale interval is deterministic, preserve and modify the
already indexed route file, then query that same target before syncing. A new
unindexed file would not stale the existing member hash and is therefore not a
valid probe.

```bash
npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar autosync stop --project "$DEMO_REPO"

STALE_TARGET="$DEMO_REPO/backend/app/api/routes/items.py"
STALE_BACKUP="$DEMO_ROOT/items.py.before-stale-probe"
cp "$STALE_TARGET" "$STALE_BACKUP"
printf '\n# temporary stale-evidence probe; restore before sync\n' \
  >> "$STALE_TARGET"

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar find --project "$DEMO_REPO" \
  --mode compact --json \
  backend/app/api/routes/items.py
```

The JSON query must expose a typed stale-evidence reason and recovery action.
If it returns an unqualified fresh family claim, stop the rehearsal and file a
product blocker; do not edit the output or continue recording.

Restore the exact pre-probe contents, verify the temporary change is gone,
explicitly synchronize the real patch, and restart autosync:

```bash
mv "$STALE_BACKUP" "$STALE_TARGET"
git -C "$DEMO_REPO" diff --check

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar sync --project "$DEMO_REPO" --progress never

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar autosync start --project "$DEMO_REPO"
```

## 7. Minimal output, static alignment, and typed UNKNOWN

Resolve the new handler's exact indexed member id from the source-free unit
inventory. This prevents `check` from ambiguously targeting a file containing
several routes:

```bash
SUMMARY_UNIT="$(
  npx --yes --package @sioyooo/repogrammar@0.4.0 \
    repogrammar units --project "$DEMO_REPO" --json |
  jq -r '.units[] | select(
    .path == "backend/app/api/routes/items.py" and
    (.id | contains("read_item_summary"))
  ) | .id' |
  head -n 1
)"
test -n "$SUMMARY_UNIT"
```

Use that exact member. The minimal query must stay useful without hiding its
read obligation or typed uncertainty:

```bash
npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar find --project "$DEMO_REPO" \
  --mode compact --verbosity minimal \
  "$SUMMARY_UNIT"

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar check --project "$DEMO_REPO" \
  --mode compact --verbosity minimal \
  "$SUMMARY_UNIT"

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar find --project "$DEMO_REPO" \
  --mode compact --json \
  unit:backend/app/api/routes/items.py#definitely_missing_summary_member
```

Required checkpoints:

- minimal `find` retains status, bounded read-plan/source-reading obligation,
  and estimated-potential measurement kind/caveat when present;
- `check` emits one current static-alignment token and always
  `runtime_equivalence: "UNKNOWN"`;
- the deliberately missing exact member's JSON returns typed `UNKNOWN`
  (normally an `InsufficientSupport` reason) plus recovery, rather than being
  attached to a plausible nearby FastAPI family;
- no line is described as measured token savings, runtime equivalence, a sound
  proof, or prevention of hallucinations.

## 8. Failure recovery

Use the product's reported action; do not improvise around safety gates.

| Failure | Required response |
| --- | --- |
| `0.4.0` cannot be fetched or reports another version | Stop; verify npm/GitHub release state outside the recording. |
| Target SHA differs | Delete only the disposable clone and clone/pin again. |
| Codex MCP entry is foreign, malformed, or drifted | Preserve it; repair ownership outside the demo. |
| Managed instruction file is foreign or malformed | Preserve it; select the correct guide or repair manually. |
| `/mcp` does not show connected | Restart Codex after setup; then use `codex mcp get repogrammar` and product `doctor` evidence. |
| Query returns `StaleEvidence` | Run the exact explicit `sync` step, then issue one new, materially justified query. |
| Query returns `UNKNOWN`/`FALLBACK` | State the typed reason and follow bounded source fallback. Never claim a selected family. |
| Docker/test failure | Stop the take; diagnose the target environment or patch. Do not treat static alignment as a substitute. |
| Stale probe receives an unqualified fresh claim | Stop and report a release blocker. |

## 9. Four-rehearsal release gate

The Test Agent must execute the full flow four times in separate disposable
roots: twice against the exact pre-tag RC tarball/packaged archive, then twice
against the pinned public registry package after GitHub/npm publication. Each
rehearsal starts from a fresh clone, fresh `.repogrammar/` state, fresh npm
launcher cache, and a restarted Codex session. Do not reuse another rehearsal's
index, target patch, MCP child process, target container volumes, npm cache, or
terminal output.

Record only claim-safe evidence in the release checklist; keep raw Codex
transcripts and target workspaces untracked and outside RepoGrammar.

### Pre-tag RC rehearsals

These two rows gate tagging. `Package identity` means the retained local tarball
hash plus the packaged native archive hash; it does not mean public npm.

| RC rehearsal | Source SHA | Package identity | MCP + instruction current | pre-read `find_analogues` | target test | stale rejected | sync + static alignment | typed UNKNOWN | cleanup |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | pending | pending | pending | pending | pending | pending | pending | pending | pending |
| 2 | pending | pending | pending | pending | pending | pending | pending | pending | pending |

### Post-public rehearsals

Only these rows may support the public installation and final-demo claims. The
Test Agent must use the literal pinned package commands shown in sections 1B–10
and verify registry integrity and provenance separately in the release gate.

| Public rehearsal | Pinned SHA | Registry package + integrity | MCP + instruction current | pre-read `find_analogues` | target test | stale rejected | sync + static alignment | typed UNKNOWN | cleanup |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | pending | pending | pending | pending | pending | pending | pending | pending | pending |
| 2 | pending | pending | pending | pending | pending | pending | pending | pending | pending |

The tables remain `pending` until the Test Agent supplies real evidence. The RC
rows must pass before tag creation; the public rows must pass after publication
before `RELEASE_AND_REPO_CONTENT_READY`. This runbook does not claim that any
rehearsal has occurred.

## 10. Cleanup

Stop containers, remove only RepoGrammar's repository-local state, and delete
the disposable root:

```bash
docker compose -f "$DEMO_REPO/compose.yml" \
  -f "$DEMO_REPO/compose.override.yml" down -v

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar autosync stop --project "$DEMO_REPO"

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar uninit --project "$DEMO_REPO" --yes

rm -rf "$DEMO_ROOT"
```

Global Codex integration and the managed instruction block are intentionally
preserved by the default cleanup because they may have existed before the demo.
Only if preflight proved both were absent and this rehearsal created them may
the maintainer restore that exact pre-demo state:

```bash
npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar uninstall --target codex --scope global --yes

npx --yes --package @sioyooo/repogrammar@0.4.0 \
  repogrammar instructions remove \
  --file "$DEMO_CODEX_GUIDE" --yes --json
```

Do not run those global removal commands when setup refreshed or reused a
pre-existing owned integration or instruction block.

## Recording outline

Keep the final cut honest and readable:

1. Problem and product insight: coding agents repeatedly rediscover local
   conventions; context reduction is useful only with explicit evidence,
   freshness, read obligations, and abstention.
2. Public pinned installation, target commit, setup, `/mcp`, and managed
   instruction verification.
3. Exact task prompt, first `find_analogues`, bounded source reads, and minimal
   patch/test.
4. Controlled stale rejection, explicit sync, minimal output, static alignment
   with runtime equivalence unknown, and typed UNKNOWN recovery.
5. Disclosure: the pilot observed 0/4 proactive A3 adoption, so this demo uses
   explicit instructions and an explicit prompt; it does not claim spontaneous
   agent adoption.
6. Collaboration attribution: the human maintainer supplied the core insight,
   architecture, evidence policy, scope, review, and merge authority; ChatGPT
   on GPT-5.6 supported planning, review, scope refinement, and claim audit;
   Codex on GPT-5.6 supported implementation, tests, documentation, and release
   tooling.
7. End card: `Local-first · Source-backed · Bounded reads · Typed UNKNOWN`.

The historical `0.2.0-preview.0` fixture transcript remains available only as
an audit record in [Historical Verified CLI Transcript](verified-cli-transcript.md).
It is not the current demo, current protocol, or current visual authority.

**Human work still required:** record and edit the video, add English narration
and captions, upload to YouTube, verify signed-out access, run Codex `/feedback`
in the final session, and add the verified video URL and Session ID to the
submission materials.
