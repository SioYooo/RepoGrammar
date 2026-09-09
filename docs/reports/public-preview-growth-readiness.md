# Public-preview growth readiness

- Evidence date: 2026-07-16
- Candidate version: `0.2.0-preview.0`
- Product posture: long-lived developer tool
- Install proof: `public-preview-install-proof-matrix.md`
- Real-repository evidence: `../case-studies/public-preview-dogfood.md`

## Verdict

`PRODUCT_STORY_READY; PUBLICATION_AND_MEASUREMENT_PENDING`

The repository now presents RepoGrammar as a continuing local-first developer
tool, with launch material derived from the product story rather than driving
it. Release automation and one native packaged candidate are locally
verified. Public no-build installation, four-platform candidate evidence,
measured token reduction, and a recorded video are not yet complete and must
not be described as complete.

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
| P1 | packaged candidate dogfood | complete on one host | self, frozen public FastAPI, and a dynamic control pass with truthful `PARTIAL_CONTEXT` and advisory `UNKNOWN` |
| P1 | measured token reduction | not measured | no paired baseline/treatment agent run; estimated diagnostics stay labeled estimated; one observed demo run saw 52% with no saved artifact, see Dogfood evidence boundary |
| P2 | repository description and topics | blocked externally | GitHub CLI authentication must be restored |
| P3 | public launch copy | prepared | product description, technical story, boundaries, and video script in launch kit |

## Product-facing assets

- `../../README.md` is ordered for developers: value proposition, real CLI
  visual, honest install state, shortest workflow, evidence model, limitations,
  project story, and community links.
- `../assets/repogrammar-demo.svg` is a real-output visual, not a fabricated
  success screenshot.
- `../demo/verified-cli-transcript.md` preserves copyable commands and the
  source transcript for accessibility and review.
- `../demo/demo-runbook.md` provides a reusable 96-second narrated demo and
  a publication checklist. It is not evidence that a video was recorded.
- `../promotion/launch-kit.md` keeps README, release notes, YouTube, and public
  launch claims aligned with the same product truth.

## Dogfood evidence boundary

The packaged integrated candidate completes `init`, `sync`, `find`, `check`,
and `stats` for RepoGrammar itself, the frozen public FastAPI repository, and a
dynamic insufficient-evidence control after conservative Python boundary fixes
found during the earlier run. Each selected target returns useful source-free
routing context while remaining `PARTIAL_CONTEXT`; conformance remains
`UNKNOWN`. That is one-host product evidence for index/query usability, not
proof of runtime equivalence or multi-platform release readiness.

The run reports an estimated diagnostic and no paired measurement. For this
run, README or any public launch copy may say “estimated potential token
reduction” only. “Measured token reduction” requires a preregistered
baseline/treatment pair with repository commit, command, configuration, actual
agent reads/tokens, failures, and result artifacts recorded.

Public copy may additionally report the separate single-run demo observation
below, provided it is attributed to one recorded run and carries its stated
limits. That permission covers only an explicitly labeled observation; it does
not relax the bar above for any claim phrased as measured, benchmarked, typical,
or expected.

A separate, single recorded demo run was observed to show a 52% token
reduction. That number comes from one demo session only: there is no saved
run artifact and no committed paired baseline/treatment run behind it, so it
is an observation, not a measurement, and public copy must not call it
"measured." Promoting it to measured requires re-running the demo under the
preregistered baseline/treatment pair described above, with repository
commit, command, configuration, actual agent reads/tokens, failures, and
result artifacts committed to the repository.

## Highest-value next action

Restore GitHub authentication, push and review this candidate, then run the
remote build-only workflow and verify all four native archives. Resolve npm
publisher authority before creating the tag. Only after that evidence is
green should `v0.2.0-preview.0` be published and the README install section be
switched from activation-pending to an executable no-build quick start.
