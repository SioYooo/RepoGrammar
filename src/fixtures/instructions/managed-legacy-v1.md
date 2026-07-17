<!-- BEGIN REPOGRAMMAR MANAGED SECTION -->
## RepoGrammar

In repositories initialized with RepoGrammar (`.repogrammar/` exists), call the MCP tool `repogrammar_context` before grep/find/Read when you need implementation-pattern context, analogous examples, family conformance, deviation explanation, or an edit plan. For find/check/explain operations, pass the repo-relative path, symbol/member id, framework role, or pattern question you have; RepoGrammar discovers candidate families internally and returns family ids as follow-up handles. Use the returned `read_plan`; if line-numbered `source_spans` are included, treat those spans as already read. Read files directly only for spans marked missing, stale, UNKNOWN, omitted, or required before editing outside the shown range.

Use `show_family` only with an exact family id returned earlier; use compact mode first and do not request `include_source_spans` by default. Stop and fall back to normal Read/Grep on UNKNOWN, FALLBACK, stale, omitted, or insufficient results. Do not run `repogrammar stats` in normal agent loops. Do not silently initialize repositories; run init/resync/autosync only when user or project policy permits repo-local analysis state.

If no `.repogrammar/` exists, skip RepoGrammar for that repository.
<!-- END REPOGRAMMAR MANAGED SECTION -->
