# ADR-0054: Default conditional global agent instruction profiles

- Status: Accepted
- Date: 2026-09-30
- Authority: the maintainer's efficiency sprint, Phase 8; implementation is
  unreleased and changes no published 0.5.0 artifact.

## Observation and obligation

MCP registration alone does not supply persistent user guidance. The historical
pilot observed zero calls in four tool-enabled runs; it does not establish the
current cause or an effect of new instructions. PR #20 proposed default wiring,
but its path discovery omits current profiles/shadowing, its empty override
falls through to a default, and refresh can lose previous receipt ownership.
Its ADR-0030 number also conflicts with library semantics. It is not merged.

## Decision

This refines ADR-0007 optional instruction consent and ADR-0026 current-owned
integration skipping: the native entry may be current while instruction-only
reconciliation remains necessary. All other ownership and rollback rules stand.

Reuse the existing native registration, marker writer, ownership receipt and
snapshot rollback. Global install/setup may write a short conditional profile
after the displayed plan is confirmed. `--no-instructions` skips new/refresh
guide writes while preserving prior receipt ownership. No real global file is
modified by tests or by this implementation sprint.

Resolve an absolute explicit `REPOGRAMMAR_INSTRUCTION_FILE_<TARGET>` first.
A present empty/relative override defers; it cannot select a different default.
Otherwise Codex uses absolute `CODEX_HOME` or `HOME/.codex`, and a nonempty
`AGENTS.override.md` before `AGENTS.md`. Claude Code uses absolute
`CLAUDE_CONFIG_DIR` or `HOME/.claude/CLAUDE.md`. Invalid profile/shadow state
defers or fails regular-file admission. Other targets remain override-only
for instruction writes; opencode's live MCP path is preserved.

The concise global profile is version 4 under the existing exact fences. It
requires MCP plus `.repogrammar/`, applies to convention/analogue/framework-role
implementation and debugging, prefers precise targets and a compact read plan,
preserves UNKNOWN/FALLBACK recovery, and skips documentation-only/ineligible
exact lookups. It authorizes no indexing, resync or background process.
The full repository/MCP preflight remains version 3 and is unchanged. Both
profiles and exact legacy bodies are recognized; unknown/modified/duplicate or
partial fences are refused. Explicit-file sync preserves a current global v4
profile; creating a missing explicit-file guide still uses full v3.

An already-current owned native MCP entry may receive a same-path
instruction-only backfill/refresh. Never remove/add native MCP for that work.
Snapshot the receipt, backup and guide before mutation; restore their exact
bytes if a later write or self-test fails. Existing `created` ownership survives
same-path refresh and opt-out, so later disconnect can remove an otherwise
empty file created by RepoGrammar. A different desired path fails before writes
with `InstructionRelocationRequired`; disconnect before selecting the new
profile. One receipt cannot silently orphan its earlier instruction section.
Setup reports pre-existing reconciliation separately from newly created targets
and preserves it if a later repository/index/autosync step fails.

## Evidence and limits

Current official guidance was fetched on 2026-09-30:

- [OpenAI instruction discovery](https://learn.chatgpt.com/docs/agent-configuration/agents-md):
  Codex home/profile and override precedence; local CLI 0.159.2 was verified.
- [Claude memory](https://code.claude.com/docs/en/memory) and
  [environment variables](https://code.claude.com/docs/en/env-vars): user guide,
  profile relocation and advisory instruction behavior; local CLI 2.1.285 was
  verified.

Inline `global_instruction_*` tests cover profile resolution/deferral,
shadowing, short-profile ownership, idempotence, foreign/malformed refusal,
same-path backfill without native writes, exact failure rollback, opt-out
receipt preservation, refused relocation and receipt-driven disconnect.
Required Rust/installer/repository gates still apply before integration.

Wiring proves configuration behavior only. A0/A1/A2 eligible/ineligible agent
adoption, source-reading work, correctness and host token/cost measurements are
NOT_MEASURED until an authorized isolated study executes. The old pilot is not
updated or reused as an effect estimate. Instructions are advisory; a global
file write is not proof that any particular host loaded or obeyed it.

## Alternatives

Direct PR #20 merge is rejected because current authority and ownership differ.
Replacing the full repository/MCP gate with a short global hint is rejected
because it would weaken the existing repository contract. Automatic cross-path
migration is deferred; explicit disconnect/reinstall is smaller and auditable.
No new production dependency, user hook, provider, network path or website is
introduced.
