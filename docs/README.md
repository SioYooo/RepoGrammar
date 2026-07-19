# Documentation Map

This directory is the canonical entry point for RepoGrammar design,
development, and governance documentation.

## Directory responsibilities

- `architecture/`: module boundaries, dependency direction, and code layout.
- `specifications/`: product, domain model, indexing, storage, and MCP
  behavior contracts.
- `development/`: agent workflow, commits, documentation policy, repository
  guard usage, and testing policy.
- `decisions/`: accepted architecture decisions. ADRs are normative once
  accepted.
- `plans/`: current implementation coordination plans. Plans are active
  execution guidance and must stay consistent with accepted ADRs and
  specifications.
- `reports/`: release-readiness and audit reports. Reports are evidence
  snapshots, not canonical product contracts.
- `release/`: maintainer-owned release runbooks. The stable `0.3.2` checklist
  is the canonical two-phase immutable publication gate; the public-preview
  checklist remains historical evidence for the prerelease.
- `experiments/`: reproducible experiment and dogfood protocols. Protocols do
  not imply measured results until filled with run evidence.
- `examples/`: user-facing examples and fixture-oriented walkthroughs.
- `promotion/`: public-preview launch copy and promotion guardrails.
- `demo/`: recording and live-demo runbooks; the Build Week runbook is the
  canonical under-three-minute submission script and evidence checklist.
- `../algorithms/paper/`: metadata-only archive of algorithm and supply-chain
  references used to design implementation milestones.
- `roadmap.md`: current staged implementation plan and deferred work.

Repository-local skills live under `.agents/skills/`. Durable but non-normative
context lives under `.agents/memories/`.

## Canonical source by topic

- Agent-wide mandatory rules: `AGENTS.md` and `CLAUDE.md`.
- Module dependencies: `architecture/dependency-rules.md`.
- Module ownership: `architecture/module-map.md`.
- Product boundaries: `specifications/product.md`.
- v0.1 implementation coordination:
  `plans/v0.1-parallel-development-plan.md`.
- v0.1 substrate hardening checkpoint retained for historical context before
  later broad product slices:
  `plans/v0.1-substrate-hardening-checkpoint.md`.
- Python v0.1 analysis: `decisions/ADR-0011-python-first-v0-1.md`,
  `decisions/ADR-0012-python-selective-analysis-cascade.md`,
  `specifications/python-analysis.md`, and
  `plans/python-v0.1-implementation-plan.md`.
- Top-20 language expansion authority:
  `decisions/ADR-0020-top-20-language-expansion-gate.md` and
  `plans/top-20-language-expansion-plan.md`. The ADR freezes the TIOBE July
  2026 planning snapshot and defines the evidence required before a language
  can be described as supported; TypeScript is tracked separately as an extra.
  Go's N1 preflight plus discovery-only implementation record are
  `decisions/ADR-0021-go-standard-library-semantic-worker-preflight.md`.
  PHP's N1 preflight plus discovery-only implementation record are
  `decisions/ADR-0024-php-sandboxed-frontend-phpunit-preflight.md`; PHP is
  `discovered_only` and unsupported, and no parser/worker dependency or
  semantic runtime behavior is authorized.
  Swift's N1 preflight plus discovery-only implementation record are
  `decisions/ADR-0025-swift-syntax-sourcekit-xctest-preflight.md`; Swift is
  `discovered_only` and unsupported, and no parser/worker dependency or
  semantic runtime behavior is authorized. The exact pause checkpoint and
  paste-ready stage-3 resume goal are in
  `plans/swift-n1-qualification-handoff.md`.
  Ruby's N1 preflight plus discovery-only implementation record are
  `decisions/ADR-0022-ruby-prism-minitest-preflight.md`.
- v0.2 agent adoption and read displacement:
  `decisions/ADR-0013-agent-adoption-read-displacement.md`,
  `plans/v0.2-agent-adoption-read-displacement-plan.md`,
  `specifications/mcp-api.md`, and `specifications/cli.md`.
- v0.2 conservative TS/JS exact-anchor hardening:
  `plans/v0.2-agent-adoption-read-displacement-plan.md`,
  `specifications/indexing-pipeline.md`, `specifications/unknowns.md`,
  `specifications/cli.md`, and `specifications/mcp-api.md`.
- v0.2 public-preview readiness: `reports/public-preview-growth-readiness.md`,
  `reports/public-preview-install-proof-matrix.md`, and
  `experiments/v0.2-real-repo-dogfood.md`.
- Public-preview growth readiness:
  `reports/public-preview-growth-readiness.md`, `quickstart.md`,
  `quickstart-codex.md`, `quickstart-claude.md`, `limitations.md`,
  `examples/python-fastapi-pytest.md`, and `promotion/launch-kit.md`.
- Build Week demo and Developer Tools evidence:
  `demo/build-week-demo.md`, `quickstart-codex.md`, and
  `promotion/launch-kit.md`.
- Public-preview release rollout gate:
  `release/public-preview-release-checklist.md`.
- Stable `0.3.2` two-phase immutable publication and instruction-adoption gate:
  `release/stable-v0.3.2-release-checklist.md`.
- Public-preview install proof snapshot:
  `reports/public-preview-install-proof-matrix.md`.
- Build Week zero-friction onboarding authority and execution evidence:
  `decisions/ADR-0026-zero-friction-onboarding-orchestration.md`,
  `plans/build-week-zero-friction-onboarding-plan.md`. The ADR freezes new
  language/framework/provider scope and preserves install, repo-local state,
  MCP, telemetry, and abstention boundaries while one `setup` entrypoint
  composes them.
- Historical Python dogfooding boundary:
  `decisions/ADR-0009-experimental-python-dogfooding.md` and
  `plans/python-dogfooding-plan.md`.
- Optional CodeGraph provider boundary:
  `decisions/ADR-0010-optional-codegraph-provider.md` and
  `plans/codegraph-provider-plan.md`.
- CLI surface: `specifications/cli.md`.
- Agent installation: `specifications/installation.md`.
- Initialization progress: `specifications/initialization-progress.md`.
- Pattern-family vocabulary: `specifications/domain-model.md`.
- UNKNOWN governance: `specifications/unknowns.md` and
  `specifications/domain-model.md`.
- Indexing pipeline: `specifications/indexing-pipeline.md`.
- Storage boundaries: `specifications/storage.md`.
- Repo-local state boundary: `decisions/ADR-0008-repo-local-state-boundary.md`
  and `specifications/storage.md`.
- Default repository auto-sync after init:
  `decisions/ADR-0027-init-default-repository-autosync.md`.
- Concurrent filesystem confinement preflight:
  `decisions/ADR-0023-handle-relative-filesystem-confinement-preflight.md`.
  The decision requires
  one no-follow handle-relative authority for discovery, source reads, and
  autosync fingerprinting; it adds no dependency or runtime fix by itself.
- MCP tool intent: `specifications/mcp-api.md`.
- Deterministic query normalization and family retrieval substrate (not yet
  routed into the production lookup path): `specifications/query-resolution.md`.
- Metrics taxonomy: `specifications/metrics.md`.
- Telemetry policy: `specifications/telemetry.md`.
- Language-native semantic workers: `specifications/semantic-workers.md`.
- Algorithm source archive: `../algorithms/paper/README.md`.
- MVP language scope: `decisions/ADR-0011-python-first-v0-1.md`
  (supersedes `decisions/ADR-0005-ts-js-first-mvp.md`).
- Quality gates: `development/repository-guard.md` and `development/testing.md`.

## Task reading guide

- Code change: read the mirrored root guide, this file, the relevant skill under
  `.agents/skills/`, the module map, and the specification for the touched
  area.
- CLI surface change: read `specifications/cli.md` and
  `.agents/skills/repogrammar-cli/SKILL.md`.
- Installer change: read `specifications/installation.md` and
  `.agents/skills/agent-integration/SKILL.md`.
- Telemetry or metric change: read `specifications/telemetry.md`,
  `specifications/metrics.md`, and
  `.agents/skills/telemetry-and-metrics/SKILL.md`.
- Documentation change: read `development/documentation-policy.md` and the
  canonical source for the affected topic.
- MCP contract change: read `.agents/skills/mcp-contract-change/SKILL.md` and
  `specifications/mcp-api.md`.
- Semantic worker change: read `specifications/semantic-workers.md` and
  `decisions/ADR-0004-rust-core-language-native-workers.md`.
- Language support change: read `decisions/ADR-0011-python-first-v0-1.md`,
  `decisions/ADR-0012-python-selective-analysis-cascade.md`,
  `decisions/ADR-0020-top-20-language-expansion-gate.md`,
  `specifications/python-analysis.md`, `specifications/product.md`,
  `docs/roadmap.md`, and the relevant plan under `plans/`, including
  `plans/top-20-language-expansion-plan.md` for ranked-scope work. Go work must
  also read `decisions/ADR-0021-go-standard-library-semantic-worker-preflight.md`.
  PHP work must also read
  `decisions/ADR-0024-php-sandboxed-frontend-phpunit-preflight.md`.
  Swift work must also read
  `decisions/ADR-0025-swift-syntax-sourcekit-xctest-preflight.md`; qualification work
  must additionally read `plans/swift-n1-qualification-handoff.md`.
  Ruby work must also read
  `decisions/ADR-0022-ruby-prism-minitest-preflight.md`.
- Optional graph/provider change: read
  `decisions/ADR-0010-optional-codegraph-provider.md`,
  `plans/codegraph-provider-plan.md`, `specifications/product.md`, and
  `architecture/dependency-rules.md`.
- Storage change: read `specifications/storage.md` and
  `decisions/ADR-0002-local-sqlite-index.md`.
- Repo-local state, logs, locks, or project configuration change: read
  `decisions/ADR-0008-repo-local-state-boundary.md` and
  `specifications/storage.md`.
- Repository discovery, source-store, autosync fingerprint, symlink/reparse, or
  filesystem-confinement change: read
  `decisions/ADR-0023-handle-relative-filesystem-confinement-preflight.md`,
  `architecture/dependency-rules.md`, `specifications/indexing-pipeline.md`,
  `specifications/storage.md`, and `development/testing.md`.
- Core model change: read `specifications/domain-model.md` and
  `.agents/skills/repogrammar-domain/SKILL.md`.

## Precedence

When documents conflict, apply this order:

1. Current explicit maintainer request.
2. Identical content in `AGENTS.md` and `CLAUDE.md`.
3. Accepted ADRs.
4. `specifications/`.
5. `architecture/`.
6. `development/`.
7. `.agents/skills/`.
8. `.agents/memories/`.
9. Other notes.

If a memory conflicts with a normative document, the normative document wins.
Update, supersede, or delete the stale memory in the same commit that exposes
the conflict.
