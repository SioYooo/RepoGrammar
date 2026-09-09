# ADR-0039: Scratch 3 `.sb3` archive integration NO-GO

- Status: Accepted
- Date: 2026-08-01
- Scope: Scratch in ADR-0020; security prerequisite and explicit product NO-GO
- Related: ADR-0020, ADR-0030, `docs/reports/language-support/scratch-completion-review.md`

## Context

Scratch 3 projects are ZIP archives whose `project.json` represents targets and
blocks. Scratch VM documents that each target owns a block AST and that blocks
carry opcodes. Production `.sb3` handling must resist traversal, symlinks, ZIP
bombs, duplicate members, oversized JSON, deep JSON, and extension/opcode
ambiguity without running Scratch VM or downloading assets.

The current RepoGrammar source-store/parser port accepts verified UTF-8 text,
not binary documents. The repository has no reviewed ZIP/deflate dependency.
Silently adding `.sb3` discovery would make indexing read binary bytes through
the wrong port and could turn a common deflated project into a failure. A
stored-only implementation must not be advertised as general Scratch support.

Reference, retrieved 2026-08-01:
<https://github.com/scratchfoundation/scratch-vm>.

## Decision

Scratch remains absent from product discovery and indexing. `.sb3` is still an
unsupported extension and no Scratch language token, code unit, dependency
record, family, CLI/MCP behavior, VM, extension process, or asset download is
added.

A pure security-preflight module is added as a prerequisite for a future binary
port. It validates classic single-disk non-ZIP64 central-directory structure,
entry count, archive/entry/aggregate sizes, a 200:1 compression-ratio ceiling,
encryption, compression methods, canonical relative paths, duplicate paths,
Unix symlink bits, exact single root `project.json`, local-header/name/method
coherence, UTF-8, duplicate JSON keys, depth/member/key limits, total JSON node
count, targets, blocks, opcodes, event-hat counts, and declared extension count.
It can inspect only a stored (method 0) `project.json` by bounded slicing.
Deflated `project.json` returns `UnsupportedCompression` rather than invoking a
tool or guessing.

The module returns counts only. It does not retain asset data, names, block IDs,
opcode strings, extension IDs, broadcasts, comments, or source text. It never
extracts files to disk, runs Scratch VM/extensions, or accesses the network.

## Consequences

This is a source-backed `NO_GO` for product integration on the present port,
not `discovered_only` and not Scratch support. The security preflight covers
important malformed-input classes but is not a full ZIP implementation: it
does not decompress method 8, validate CRC/data descriptors as a general ZIP
reader, establish an owned Scratch IR, resolve extension semantics, or form an
event-stack/broadcast family.

## Follow-up

Design a bounded binary-document port and qualify a maintained ZIP/deflate
implementation with CRC, overlapping-record, ZIP64 policy, fuzz/adversarial
corpus, and extraction-free `project.json` streaming. Only then may `.sb3`
enter product discovery and the Scratch IR/family work begin.
