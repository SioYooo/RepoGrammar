# Python FastAPI And pytest Fixture Demo

This copy-paste demo runs RepoGrammar against committed test fixtures. It is a
source-checkout dogfood flow for coding-agent context reduction, not evidence of
measured token savings.

## Fixture Setup

Run this from the RepoGrammar source checkout:

```text
DEMO_REPO="$(mktemp -d)"
mkdir -p "$DEMO_REPO/app" "$DEMO_REPO/tests"
cp src/fixtures/python/release/v0_1/positive-strong-evidence/routes.py "$DEMO_REPO/app/routes.py"
cp src/fixtures/python/release/v0_1/pytest-basic/test_users.py "$DEMO_REPO/tests/test_users.py"
cp src/fixtures/python/release/v0_1/pytest-basic/conftest.py "$DEMO_REPO/tests/conftest.py"
```

Baseline behavior without RepoGrammar is broad source reading. An agent that
does not have pattern-family context would usually inspect the route and test
files before deciding what to edit:

```text
find "$DEMO_REPO" -name '*.py' -print
sed -n '1,220p' "$DEMO_REPO/app/routes.py"
sed -n '1,220p' "$DEMO_REPO/tests/test_users.py"
sed -n '1,220p' "$DEMO_REPO/tests/conftest.py"
```

## Source-Free RepoGrammar Query

Initialize the fixture repository, then ask for context about the route file:

```text
cargo run --quiet --bin repogrammar -- init --project "$DEMO_REPO" --yes --no-autosync --progress never
cargo run --quiet --bin repogrammar -- find \
  --project "$DEMO_REPO" \
  --token-budget 8000 \
  --json app/routes.py
```

Expected shape:

```json
{
  "status": "ok",
  "family": {
    "family_id": "family:python:fastapi_route:framework_fastapi_route",
    "classification": "DOMINANT_PATTERN",
    "support": 3
  },
  "query_route": {
    "route": "discover_hydrate_compose",
    "selected_family_id": "family:python:fastapi_route:framework_fastapi_route"
  },
  "read_plan": {
    "source_snippets_included": false,
    "requires_source_before_edit": true,
    "items": [
      {
        "path": "app/routes.py",
        "start_line": 6,
        "end_line": 8,
        "purpose": "target_body_required_for_edit",
        "source_required_before_edit": true
      }
    ]
  },
  "output": {
    "estimated_potential_token_savings_kind": "ESTIMATED",
    "estimated_potential_token_savings_caveat": "estimated potential only; not measured token savings",
    "source_snippets_included": false
  }
}
```

The exact token counts may differ by build and fixture state. Treat
`estimated_potential_token_savings` as an estimated potential-read-displacement
diagnostic only. It is not measured token savings and is not a causal claim.

## Optional Source Spans

Request source text only when the agent needs bounded line-numbered spans from
the read plan:

```text
cargo run --quiet --bin repogrammar -- find \
  --project "$DEMO_REPO" \
  --token-budget 8000 \
  --json \
  --include-source-spans app/routes.py
```

Expected additional shape:

```json
{
  "read_plan": {
    "source_snippets_included": true
  },
  "source_spans": {
    "requested": true,
    "source_snippets_included": true,
    "spans": [
      {
        "path": "app/routes.py",
        "start_line": 6,
        "end_line": 8,
        "text": "6\t@router.get(\"/users\")\n7\tdef list_users():\n8\t    return []"
      }
    ]
  }
}
```

Only the listed spans are rendered. Use normal source reads before editing
outside them.

## How An Agent Should Use The Result

- Use `family` and `query_route` to understand why this target matched a local
  implementation family.
- Use `read_plan.items` as the source-reading checklist before editing.
- Keep `source_snippets_included: false` as the default; opt into source spans
  only for bounded inspection.
- If RepoGrammar returns `PARTIAL_CONTEXT`, use the read plan as local metadata
  only; no family claim was made.
- If RepoGrammar returns `UNKNOWN`, fallback, stale evidence, omitted spans, or
  insufficient support, do not claim the pattern is proven. Run
  `repogrammar status`, `repogrammar doctor`, or `repogrammar resync` when
  appropriate, or fall back to normal source reads for the affected files.

## Negative Cases To Preserve

- Dynamic route decorators or route prefixes must not become support evidence.
- Custom pytest wrappers must not be treated as native runner support.
- Ambiguous or plugin-provided fixtures must remain typed `UNKNOWN`.
- Low-support examples should return `UNKNOWN` instead of a weak family claim.
