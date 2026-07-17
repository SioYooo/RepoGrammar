# Codex Quickstart

This flow uses the zero-friction setup orchestrator to wire Codex, initialize
the current repository, build its index, start auto-sync, and run a product MCP
self-test behind one reviewed plan. Use the general quickstart's exact-version
availability gate; when either registry check fails, acquire from source.

## Acquire RepoGrammar From Source

```text
git clone https://github.com/SioYooo/RepoGrammar.git
cd RepoGrammar
cargo build --release
bash src/install/repogrammar-install.sh --install-cli-only --from-source --yes
repogrammar version
```

## Preview And Run Codex Setup

From the repository Codex will work on:

```text
cd /path/to/your/repo
repogrammar setup --target codex --dry-run
repogrammar setup --target codex
```

The live command presents one plan and asks once before writing. For a reviewed
noninteractive run, use:

```text
repogrammar setup --target codex --yes
```

Setup never enables telemetry. It writes only through the existing reversible
Codex MCP integration boundary, retains valid pre-existing state, starts
repo-local auto-sync by default, and rolls back only machine configuration that
the current failed attempt created. A missing `codex` CLI leaves a usable
repository index and returns one install-agent action rather than destroying
the repository-only result.

## Verify The Global Codex Pre-flight

Setup refreshes a managed instruction block only when the existing Codex
integration is safely owned **and** an explicit instruction-file override is
configured for that setup path. To inspect or refresh the default global guide
without reconfiguring MCP or touching repository state, select that file
explicitly:

```text
repogrammar instructions status --file "$HOME/.codex/AGENTS.md" --json
repogrammar instructions sync --file "$HOME/.codex/AGENTS.md" --dry-run
repogrammar instructions sync --file "$HOME/.codex/AGENTS.md" --yes
```

Use a different explicit path when `CODEX_HOME` or local policy places the
guide elsewhere. The command updates only an exact current or known legacy
RepoGrammar marker block, preserves unrelated instructions, and refuses foreign
or malformed marker content. It does not create `.repogrammar/`, run setup, or
mirror `CLAUDE.md`.

## Use Codex And GPT-5.6

Open Codex in the configured repository. Use `/mcp` to confirm the
`repogrammar` server is connected. Use `/model` and select an available
GPT-5.6 family option for the Build Week demo; model names and availability can
vary by account and Codex surface, so do not hardcode a hidden or unavailable
slug. The official Codex slash-command reference describes `/model` as the
current-task model selector.

Ask:

```text
How are API routes implemented in this repository?
```

Codex should call the read-only `repogrammar_context` MCP tool before CodeGraph
or broad source reads when an implementation, test, fix, refactor, or diagnosis
requires repository-local contract/convention, repeated implementation,
framework-role, or analogue evidence. This includes schema, protocol, API, and
prompt-output contract drift. RepoGrammar returns evidence, a read plan, and
typed uncertainty. `UNKNOWN`, fallback, stale evidence, or omitted spans mean
Codex must state the reason and use normal source reads for the affected files;
they must never be upgraded into a confident family claim.

RepoGrammar does not run GPT-5.6 or call the OpenAI API itself. GPT-5.6 is the
Codex development/demo reasoning surface, while RepoGrammar is the local MCP
developer tool supplying conservative repository context. No OpenAI API key is
required by RepoGrammar.

## Capture The Build Week Feedback Session ID

Keep the exact Codex task used to build or validate the submission open:

1. Type `/status` and record the visible task identifier for internal traceability.
2. Type `/feedback` in that same task.
3. Review the feedback text and choose whether to include logs; never include
   secrets or private repository content.
4. Submit the feedback and copy the Session ID shown by the confirmation into:
   `Feedback Session ID: <paste verified /feedback Session ID here>`.
5. If the client does not display a Session ID, leave the placeholder and ask
   the event/support channel which identifier is accepted. Do not substitute
   the `/status` task ID without confirmation.

Official Codex documentation says `/feedback` opens the feedback dialog and
can optionally include logs; it does not promise in the public reference that
every client displays a Session ID. The guarded placeholder avoids inventing
submission evidence.

References:

- [Codex slash commands](https://learn.chatgpt.com/docs/reference/slash-commands#available-slash-commands)
- [GPT-5.6 family in Codex surfaces](https://learn.chatgpt.com/docs/whats-new#choose-the-right-gpt-56-model)

## Exact No-Build Path

After the exact npm version and matching GitHub asset pass the availability
gate in `quickstart.md`:

```text
npx @sioyooo/repogrammar@0.2.0 setup --project /path/to/your/repo --target codex
```

If either check fails, use the source acquisition path above.
