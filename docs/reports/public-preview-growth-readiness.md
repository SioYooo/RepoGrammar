# Public-preview growth readiness

- Evidence date: 2026-07-16
- Candidate version: `0.2.0-preview.0`
- Product posture: long-lived developer tool; Build Week is a launch milestone
- Install proof: `public-preview-install-proof-matrix.md`
- Real-repository evidence: `../case-studies/public-preview-dogfood.md`

## Verdict

`PRODUCT_STORY_READY; PUBLICATION_AND_MEASUREMENT_PENDING`

The repository now presents RepoGrammar as a continuing local-first developer
tool, with Build Week material derived from the product story rather than
driving it. Release automation and one native packaged candidate are locally
verified. Public no-build installation, four-platform candidate evidence,
measured token reduction, a recorded video, and Devpost submission are not yet
complete and must not be described as complete.

## Readiness by priority

| Priority | Deliverable | State | Evidence or blocker |
|---|---|---|---|
| P0 | truthful macOS/Linux release matrix | complete in repository | four targets; no Windows artifact |
| P0 | tag credential gate and build-only separation | complete in repository | tag preflight requires accepted npm identity; dispatch cannot publish |
| P0 | packaged smoke path | local native proof complete | macOS arm64 archive passed version, setup, MCP self-test, find, and check |
| P0 | all four candidate artifacts | blocked externally | branch is not pushed and remote build-only has not run |
| P0 | npm/GitHub prerelease | blocked externally | GitHub auth invalid; npm token absent; registry package lookup returns `E404` |
| P1 | developer-first README | complete | value, visual, install truth, shortest workflow, limitations, then project story |
| P1 | accessible demo transcript | complete | audited command/output transcript linked from README |
| P1 | reusable video plan | script complete | 96-second narrated shot list and evidence checklist exist |
| P1 | animated GIF and public video | not produced | requires real screen recording, audio, editing, and public upload |
| P1 | public FastAPI dogfood | complete for one frozen repository | index/query path passes with truthful `PARTIAL_CONTEXT` and advisory `UNKNOWN` |
| P1 | measured token reduction | not measured | no paired baseline/treatment agent run; estimated diagnostics stay labeled estimated |
| P2 | repository description and topics | blocked externally | GitHub CLI authentication must be restored |
| P3 | Devpost copy | prepared | product description, technical story, boundaries, and video script in launch kit |
| P3 | Devpost submission | not submitted | public video, `/feedback` session ID, verified release URL, and form submission remain |

## Product-facing assets

- `../../README.md` is ordered for developers: value proposition, real CLI
  visual, honest install state, shortest workflow, evidence model, limitations,
  project story, and community links.
- `../assets/repogrammar-demo.svg` is a real-output visual, not a fabricated
  success screenshot.
- `../demo/verified-cli-transcript.md` preserves copyable commands and the
  source transcript for accessibility and review.
- `../demo/build-week-demo.md` provides a reusable 96-second narrated demo and
  a publication checklist. It is not evidence that a video was recorded.
- `../promotion/launch-kit.md` keeps README, release notes, YouTube, and Devpost
  claims aligned with the same product truth.

## Dogfood evidence boundary

The frozen public FastAPI repository completes `init`, `sync`, `find`, `check`,
and `stats` after three conservative Python boundary fixes found during the
run. The selected target returns useful source-free routing context while
remaining `PARTIAL_CONTEXT`; conformance remains `UNKNOWN`. That is product
evidence for index/query usability, not proof of runtime equivalence.

The run reports an estimated diagnostic and no paired measurement. README or
Devpost may say “estimated potential token reduction” only. “Measured token
reduction” requires a preregistered baseline/treatment pair with repository
commit, command, configuration, actual agent reads/tokens, failures, and result
artifacts recorded.

## Highest-value next action

Restore GitHub authentication, push and review this candidate, then run the
remote build-only workflow and verify all four native archives. Resolve npm
publisher authority before creating the tag. Only after that evidence is
green should `v0.2.0-preview.0` be published and the README install section be
switched from activation-pending to an executable no-build quick start.
