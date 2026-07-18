<!-- BEGIN REPOGRAMMAR MANAGED SECTION -->
<!-- REPOGRAMMAR MANAGED CONTENT VERSION: 2 -->
## RepoGrammar pre-flight gate

RepoGrammar MCP is read-only. Read mandatory repository authority and instruction documents first. Then, when `.repogrammar/` exists, use this gate when planning or executing an implementation, fix, refactor, test, or diagnosis that requires a repository-local contract or convention, repeated implementation, framework role, or analogue comparison. For a covered task, run it before any non-trivial code location is sought and before any code-search or source-reading tool.

Covered cases include root-cause diagnosis or repair and schema, protocol, API, prompt-output, or Meaning Contract qualification, conformance, or drift. A YAML prompt or qualification output checked against a repeated Meaning Contract is covered; neither its file type nor an exact file target exempts mixed work.

1. Call `repogrammar_context` once with `operation: "find_analogues"`, `target: "<the concrete repo-relative path, symbol/member id, framework role, or code-work question from the task>"`, and `mode: "compact"`.
2. Consume the returned `read_plan`; line-numbered `source_spans` included in the result are already read.
3. If the tool is unavailable or the result explicitly reports `UNKNOWN`, `FALLBACK`, stale, omitted, or insufficient evidence, stop. State that fallback reason before proceeding, then use CodeGraph for exact source or call-path detail; use ordinary search/read only when CodeGraph is unavailable or still insufficient.
4. Otherwise, consume the supported `read_plan` first, then use CodeGraph only for exact source or call-path detail that RepoGrammar did not supply.

Never use CodeGraph first for covered work merely because exact source or call-path detail will be needed later. Do not repeat the same RepoGrammar call unless the target or indexed evidence changed. Treat returned family ids as follow-up handles and use `show_family` only with an exact id returned earlier. Do not request `include_source_spans` by default and do not run `repogrammar stats` in normal agent loops.

Skip this gate for pure documentation or prose; operational release, git, environment, or credential inspection; syntax-only YAML or configuration validation; and an exact one-symbol, file, or call-path lookup, but only when no repository contract, convention, repeated implementation, framework role, analogue comparison, code-behavior diagnosis, or implementation decision is involved. Never initialize, resync, or start autosync silently; those writes require user or project-policy permission. If `.repogrammar/` does not exist, skip RepoGrammar for that repository.
<!-- END REPOGRAMMAR MANAGED SECTION -->
