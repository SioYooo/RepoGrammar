# Verified CLI Transcript

This transcript backs the terminal visual in the root README. It was captured
on 2026-07-16 from commit `73770e6` with the current debug binary:

```text
repogrammar 0.2.0-preview.0
```

The source files are committed fixtures. The actual temporary project path was
replaced with `$DEMO_REPO`; no result line was rewritten. The demo used an
isolated HOME and a PATH containing only system Git/Python tools so setup could
not inspect or modify the developer's real Codex or Claude Code configuration.

## Reproduce the fixture

Run from the RepoGrammar checkout:

```bash
cargo build --bin repogrammar

DEMO_REPO="$(mktemp -d)"
DEMO_HOME="$(mktemp -d)"
mkdir -p "$DEMO_REPO/app" "$DEMO_REPO/experimental"
cp src/fixtures/python/release/v0_1/positive-strong-evidence/routes.py \
  "$DEMO_REPO/app/routes.py"
cp src/fixtures/python/release/v0_1/dynamic-unknown/dynamic.py \
  "$DEMO_REPO/experimental/dynamic.py"

BINARY="$PWD/target/debug/repogrammar"
HOME="$DEMO_HOME" PATH=/usr/bin:/bin "$BINARY" setup \
  --project "$DEMO_REPO" \
  --target auto \
  --yes \
  --no-autosync \
  --progress never
```

Captured setup output:

```text
setup: completed with limitations
repository: 2 files indexed, 1 pattern groups verified
limitation: no supported live agent CLI was detected
telemetry: unchanged by setup; off by default
product MCP: repogrammar_context self-test passed
agent MCP: not active; use the repository index through the RepoGrammar CLI
next: install a supported coding agent, then run repogrammar setup
```

The limitation is intentional evidence that product self-test, repository
readiness, and native coding-agent wiring are reported as separate facts.

## Find a supported family

```bash
"$BINARY" find --project "$DEMO_REPO" --token-budget 8000 app/routes.py
```

Captured output:

```text
find: evidence-backed family
active_generation: gen-000001
family: family:python:fastapi_route:framework_fastapi_route
classification: DOMINANT_PATTERN
support: 4
evidence_mode: evidence
estimated_evidence_tokens: 82
source_snippets: not_included
query_route: discover_hydrate_compose
query_input_kind: path_symbol_role_or_pattern_target
query_family_id_policy: family_ids_are_returned_follow_up_handles_not_required_initial_inputs
query_candidate_limit: 5
query_pipeline: discover_candidates,hydrate_bounded_candidates,select_single_fresh_family,compose_context_bundle
query_selected_family_id: family:python:fastapi_route:framework_fastapi_route
query_candidate_family_ids: family:python:fastapi_route:framework_fastapi_route
query_follow_up_family_ids: family:python:fastapi_route:framework_fastapi_route
query_why_selected: target resolved to one fresh candidate family; RepoGrammar hydrated that family and composed bounded context
evidence_selection: greedy_marginal_coverage_v1
budget_satisfied: true
estimated_read_plan_tokens: 150
estimated_potential_token_savings: 254
estimated_potential_token_savings_kind: ESTIMATED
estimated_potential_token_savings_caveat: estimated potential only; not measured token savings
read_plan_requires_source_before_edit: true
covered_claims: canonical,support
Suggested source spans to read
read_plan: items: 2	estimated_tokens: 150	source_snippets: not_included
read: target_body_required_for_edit	path: app/routes.py	range: 55-107	lines: 6-8	content_hash: sha256:316b921a32e0dcd09918f37a7f4916a2e3b9f0f8e367450372488f01af9321aa	estimated_tokens: 71	requires_source_before_edit: true	why: read this target body before editing; family metadata is context only
read: support_evidence	path: experimental/dynamic.py	range: 905-1034	lines: 46-48	content_hash: sha256:cc5625ade08142027085adf749e4b9da0853da3383e2cd78aed56a4a611df774	estimated_tokens: 79	requires_source_before_edit: false	why: farthest-contrast supporting source span; verify before applying the family blindly
member: unit:app/routes.py#fastapi_route:list_users:55-107:1	role: framework:fastapi.route
member: unit:app/routes.py#fastapi_route:list_accounts:111-169:2	role: framework:fastapi.route
member: unit:app/routes.py#fastapi_route:list_teams:173-225:3	role: framework:fastapi.route
member: unit:experimental/dynamic.py#fastapi_route:dynamic_dependency:905-1034:6	role: framework:fastapi.route
evidence: family-evidence:family_python_fastapi_route_framework_fastapi_route:000000	path: app/routes.py	range: 55-107	content_hash: sha256:316b921a32e0dcd09918f37a7f4916a2e3b9f0f8e367450372488f01af9321aa	estimated_tokens: 82	covered_claims: canonical,support
variation_slot: slot:python_fastapi_effect_marker	description: variation:python_fastapi_effect_marker:context metadata differs across supported members
variation_slot: slot:python_fastapi_service_call_shape	description: variation:python_fastapi_service_call_shape:context metadata differs across supported members
variation_slot: slot:runtime_unknown	description: non_blocking_unknown:FrameworkMagic:runtime equivalence remains unproven
variation_slot: slot:unknown:runtimedependencyinjection:family_python_fastapi_route_framework_fastapi_route_fastapi_dependency_target:000000	description: unknown|non_blocking_unknown|RuntimeDependencyInjection|family:python:fastapi_route:framework_fastapi_route:fastapi_dependency_target|resolve this Python subclaim before relying on it
unknown: non_blocking_unknown:RuntimeDependencyInjection affected_claim: family:python:fastapi_route:framework_fastapi_route:fastapi_dependency_target
recovery: resolve this Python subclaim before relying on it
unknown: non_blocking_unknown:FrameworkMagic affected_claim: family:python:fastapi_route:framework_fastapi_route:runtime_equivalence
recovery: add semantic-worker or framework adapter evidence
```

The `254` value is output from the product, but its adjacent kind and caveat
are inseparable from the claim: it is an estimate, not a measured saving.

## Check without overclaiming conformance

```bash
"$BINARY" check --project "$DEMO_REPO" --token-budget 8000 app/routes.py
```

Captured output:

```text
check: CONTEXT_ONLY
active_generation: gen-000001
family: family:python:fastapi_route:framework_fastapi_route
classification: DOMINANT_PATTERN
support: 4
evidence_mode: evidence
estimated_evidence_tokens: 82
source_snippets: not_included
query_route: discover_hydrate_compose
query_input_kind: path_symbol_role_or_pattern_target
query_family_id_policy: family_ids_are_returned_follow_up_handles_not_required_initial_inputs
query_candidate_limit: 5
query_pipeline: discover_candidates,hydrate_bounded_candidates,select_single_fresh_family,compose_context_bundle
query_selected_family_id: family:python:fastapi_route:framework_fastapi_route
query_candidate_family_ids: family:python:fastapi_route:framework_fastapi_route
query_follow_up_family_ids: family:python:fastapi_route:framework_fastapi_route
query_why_selected: target resolved to one fresh candidate family; RepoGrammar hydrated that family and composed bounded context
evidence_selection: greedy_marginal_coverage_v1
budget_satisfied: true
estimated_read_plan_tokens: 150
estimated_potential_token_savings: 254
estimated_potential_token_savings_kind: ESTIMATED
estimated_potential_token_savings_caveat: estimated potential only; not measured token savings
read_plan_requires_source_before_edit: true
covered_claims: canonical,support
Suggested source spans to read
read_plan: items: 2	estimated_tokens: 150	source_snippets: not_included
read: target_body_required_for_edit	path: app/routes.py	range: 55-107	lines: 6-8	content_hash: sha256:316b921a32e0dcd09918f37a7f4916a2e3b9f0f8e367450372488f01af9321aa	estimated_tokens: 71	requires_source_before_edit: true	why: read this target body before editing; family metadata is context only
read: support_evidence	path: experimental/dynamic.py	range: 905-1034	lines: 46-48	content_hash: sha256:cc5625ade08142027085adf749e4b9da0853da3383e2cd78aed56a4a611df774	estimated_tokens: 79	requires_source_before_edit: false	why: farthest-contrast supporting source span; verify before applying the family blindly
advisory_status: UNKNOWN
reason: runtime equivalence remains unproven
member: unit:app/routes.py#fastapi_route:list_users:55-107:1	role: framework:fastapi.route
member: unit:app/routes.py#fastapi_route:list_accounts:111-169:2	role: framework:fastapi.route
member: unit:app/routes.py#fastapi_route:list_teams:173-225:3	role: framework:fastapi.route
member: unit:experimental/dynamic.py#fastapi_route:dynamic_dependency:905-1034:6	role: framework:fastapi.route
evidence: family-evidence:family_python_fastapi_route_framework_fastapi_route:000000	path: app/routes.py	range: 55-107	content_hash: sha256:316b921a32e0dcd09918f37a7f4916a2e3b9f0f8e367450372488f01af9321aa	estimated_tokens: 82	covered_claims: canonical,support
variation_slot: slot:python_fastapi_effect_marker	description: variation:python_fastapi_effect_marker:context metadata differs across supported members
variation_slot: slot:python_fastapi_service_call_shape	description: variation:python_fastapi_service_call_shape:context metadata differs across supported members
variation_slot: slot:runtime_unknown	description: non_blocking_unknown:FrameworkMagic:runtime equivalence remains unproven
variation_slot: slot:unknown:runtimedependencyinjection:family_python_fastapi_route_framework_fastapi_route_fastapi_dependency_target:000000	description: unknown|non_blocking_unknown|RuntimeDependencyInjection|family:python:fastapi_route:framework_fastapi_route:fastapi_dependency_target|resolve this Python subclaim before relying on it
unknown: non_blocking_unknown:RuntimeDependencyInjection affected_claim: family:python:fastapi_route:framework_fastapi_route:fastapi_dependency_target
recovery: resolve this Python subclaim before relying on it
unknown: non_blocking_unknown:FrameworkMagic affected_claim: family:python:fastapi_route:framework_fastapi_route:runtime_equivalence
recovery: add semantic-worker or framework adapter evidence
```

## Typed UNKNOWN for an unresolved target

```bash
"$BINARY" find --project "$DEMO_REPO" --token-budget 8000 registered_router
```

Captured output:

```text
find: UNKNOWN
active_generation: gen-000001
query_route: discovery_unknown
query_input_kind: path_symbol_role_or_pattern_target
query_family_id_policy: family_ids_are_returned_follow_up_handles_not_required_initial_inputs
query_candidate_limit: 5
query_pipeline: discover_candidates,abstain
query_why_selected: candidate discovery or local target resolution could not produce a single supported family without overclaiming
unknown: blocking_unknown:InsufficientSupport affected_claim: query target
recovery: use source fallback
```

`registered_router` intentionally has no resolvable indexed family or exact
local target in the fixture. RepoGrammar abstains instead of attaching it to the
nearby FastAPI family.

## Visual provenance

`docs/assets/repogrammar-demo.svg` was manually typeset from exact lines in
this transcript. It abbreviates the successful `find` and `check` output but
does not alter their status, family, support, read-plan count, estimate label,
caveat, or uncertainty. The command paths are normalized from the actual
temporary directory to `$DEMO_REPO`.

The capture environment did not contain `vhs`, `asciinema`, `agg`, `ffmpeg`,
ImageMagick, `termtosvg`, `svg-term-cli`, or Pillow. No dependency was installed
and no AI-generated image was used. A timed GIF remains blocked until a
recording/rendering tool is available; the SVG and this transcript are the
auditable fallback.
