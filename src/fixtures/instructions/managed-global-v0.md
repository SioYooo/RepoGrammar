<!-- BEGIN REPOGRAMMAR MANAGED SECTION -->
## RepoGrammar

In repositories initialized with RepoGrammar (`.repogrammar/` exists), call the MCP tool `repogrammar_context` before CodeGraph, grep/find, or manual Read when a task involves implementation patterns, framework roles, family membership, conformance/deviation, analogous examples, or an edit plan for repeated behavior. Use CodeGraph afterward only for exact source/call-path detail or when RepoGrammar returns `UNKNOWN`, stale, omitted, or insufficient support. Use the returned `read_plan`; if line-numbered `source_spans` are included, treat those spans as already read. Read files directly only for spans marked missing, stale, UNKNOWN, omitted, or required before editing outside the shown range.

If no `.repogrammar/` exists, skip RepoGrammar for that repository.
<!-- END REPOGRAMMAR MANAGED SECTION -->
