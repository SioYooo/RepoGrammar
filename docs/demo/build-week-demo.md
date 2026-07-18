# Build Week Demo Runbook

This is the canonical recording plan for one reusable RepoGrammar product demo.
The target cut is **90–100 seconds with spoken audio**. It must remain below
three minutes after title and end cards. The same master can supply a short
README clip later, but the current repository visual is the audited static SVG
because no recording/rendering tool was available in the verification
environment.

The demo presents RepoGrammar as a long-lived developer tool. Build Week is the
launch context, not the reason the product exists.

## Truth gates before recording

Do not record or publish until all applicable boxes are true:

- [ ] The recording starts from the intended `main` commit and states its tag.
- [ ] If the no-build install is shown, the exact GitHub asset and npm package
      have passed the stable `0.2.2` release checklist. Until then, record the
      source-checkout acquisition path and call it contributor dogfood.
- [ ] The disposable fixture and all CLI output are generated live from the
      current binary; no result is pasted into the terminal.
- [ ] `find` shows the `ESTIMATED` kind and its not-measured caveat whenever the
      estimate is visible.
- [ ] `check` returns a static-alignment token with
      `runtime_equivalence: "UNKNOWN"` (this demo predates the certificate
      rename from the legacy `CONTEXT_ONLY` advisory).
- [ ] The final query returns typed `UNKNOWN` and the recovery action.
- [ ] Codex shows `repogrammar` connected under `/mcp` before making an MCP
      claim. Select an actually available GPT-5.6 family model with `/model`;
      do not hardcode an unavailable model slug.
- [ ] A spoken audio track explains how ChatGPT/GPT-5.6 and Codex/GPT-5.6 were
      used to develop the project.
- [ ] Final duration is under 03:00, and the public YouTube video can be viewed
      in a signed-out/private browser window.

## Disposable fixture

Prepare this before recording. These commands use only committed fixture
source and do not run target-repository code:

```bash
DEMO_REPO="$(mktemp -d)"
mkdir -p "$DEMO_REPO/app" "$DEMO_REPO/experimental"
cp src/fixtures/python/release/v0_1/positive-strong-evidence/routes.py \
  "$DEMO_REPO/app/routes.py"
cp src/fixtures/python/release/v0_1/dynamic-unknown/dynamic.py \
  "$DEMO_REPO/experimental/dynamic.py"
```

For the current source-dogfood recording path, build and install once from the
RepoGrammar checkout:

```bash
cargo build --release
bash src/install/repogrammar-install.sh --install-cli-only --from-source --yes
repogrammar version
```

After `v0.2.2` artifacts and npm publication are independently
verified, replace that acquisition shot with the no-build path:

```bash
npx @sioyooo/repogrammar@0.2.2 setup --project "$DEMO_REPO" --target codex
```

That command is a recording option only after publication proof exists.

## 96-second shot and audio script

| Time | Screen and command | Spoken audio |
|---|---|---|
| 00–08s | Title: “RepoGrammar — source-backed repository context for coding agents.” Show a busy repository tree, then the compact terminal. | “Coding agents spend tokens rediscovering the same repository conventions. RepoGrammar gives them a small, auditable pattern map before they read broadly.” |
| 08–22s | Run `repogrammar setup --project "$DEMO_REPO" --target auto --yes --no-autosync --progress never`. Keep the separate repository, product-MCP, and agent-MCP lines visible. | “One setup command builds the local index, safely wires a supported agent when available, and self-tests the read-only MCP server. Here no agent CLI is present, so repository CLI readiness succeeds without pretending agent wiring is active.” |
| 22–40s | Run `repogrammar find --project "$DEMO_REPO" --token-budget 8000 app/routes.py`. Highlight `DOMINANT_PATTERN`, `support: 4`, `source_snippets: not_included`, and the two-item read plan. | “Find discovers a FastAPI route family backed by four compatible members. The default response is metadata, not a source dump, and it tells the agent which exact spans still need reading before an edit.” |
| 40–52s | Hold on `estimated_potential_token_savings_kind: ESTIMATED` and its caveat. | “The read-displacement number is explicitly estimated potential. It is not presented as measured token savings.” |
| 52–67s | Run `repogrammar check --project "$DEMO_REPO" --token-budget 8000 app/routes.py`. Highlight the static-alignment `status` token, `runtime_equivalence: UNKNOWN`, and the runtime-equivalence reason (recorded before the certificate rename, the transcript shows the legacy `CONTEXT_ONLY` form). | “Check can supply useful conformance context, but static family evidence does not prove runtime equivalence. RepoGrammar keeps that conclusion unknown.” |
| 67–79s | Run `repogrammar find --project "$DEMO_REPO" --token-budget 8000 registered_router`. Highlight `UNKNOWN`, `InsufficientSupport`, and `use source fallback`. | “When the target cannot be supported, it abstains by type and sends the agent back to source instead of guessing from the nearby FastAPI code.” |
| 79–91s | Cut to Codex. Show `/model` with an available GPT-5.6 family selection, `/mcp` with `repogrammar`, then a `repogrammar_context` result/read plan. | “ChatGPT on GPT-5.6 helped plan and review the product. Codex on GPT-5.6 implemented and tested it. RepoGrammar itself stays local: no model, cloud call, or OpenAI API key is required.” |
| 91–96s | End card: repository URL, `Local-first · Source-backed · Typed UNKNOWN`. | “RepoGrammar helps coding agents read what matters—and admit what the repository has not proved.” |

Do not speed up output so much that the status, estimate caveat, or UNKNOWN
recovery cannot be read. If a natural take runs long, shorten pauses or the
repository-tree opening; do not remove the truthfulness lines.

## Expected live checkpoints

The committed fixture currently produces these verified facts. Exact hashes
and token estimates belong to the live run and should not be manually edited:

```text
setup: completed with limitations
repository: 2 files indexed, 1 pattern groups verified
product MCP: repogrammar_context self-test passed
agent MCP: not active; use the repository index through the RepoGrammar CLI

find: evidence-backed family
family: family:python:fastapi_route:framework_fastapi_route
classification: DOMINANT_PATTERN
support: 4
estimated_potential_token_savings_kind: ESTIMATED
estimated_potential_token_savings_caveat: estimated potential only; not measured token savings

check: CONTEXT_ONLY   # legacy pre-certificate form as recorded; current
                      # builds emit a static-alignment status token with
                      # runtime_equivalence: UNKNOWN
advisory_status: UNKNOWN
reason: runtime equivalence remains unproven

find: UNKNOWN
unknown: blocking_unknown:InsufficientSupport affected_claim: query target
recovery: use source fallback
```

The complete capture is in the
[verified CLI transcript](verified-cli-transcript.md).

## Capture and publication checklist

- [ ] Record the terminal at 1080p or higher with a readable monospace font.
- [ ] Record clean spoken audio; remove secrets, usernames, absolute home paths,
      notification banners, and private repository content.
- [ ] Keep one continuous live-command take where practical. If cuts are used,
      never splice a command together with output from a different build.
- [ ] Add captions and a text transcript.
- [ ] Verify the final file duration with the available media tool; gate at
      `< 180 seconds`, with 90–100 seconds preferred.
- [ ] Upload to YouTube as a public or unlisted video accepted by the submission
      rules.
- [ ] Open the YouTube URL signed out and confirm audio, captions, 1080p
      playback, description links, and repository URL.
- [ ] Add the verified YouTube URL to this file, README, launch kit, and Devpost.
- [ ] Capture the accepted Codex `/feedback` Session ID without exposing logs,
      secrets, or private source.

**External work still required:** record the audio/video, edit/caption it,
upload it to YouTube, verify signed-out access, and add the final URL. Repository
automation cannot truthfully complete those human/publication steps in this
environment.
