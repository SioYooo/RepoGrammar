# Incremental invalidation audit

Date: 2026-09-30. Status: measured work report; no invalidation narrowing shipped.
Authority: `classify_sync_context_gate`, incremental copy-forward and
`record_family_claims` in `src/rust/application/indexing.rs`, plus the
[indexing contract](../specifications/indexing-pipeline.md).

## Current work closure

The deterministic N16 expansion admits 21 files, including 18 Python files.
The transport change preserves these invalidation decisions. Counts below are
portable product counters from the [A/B samples](data/python-session-ab.v1.json),
not assumed filesystem traffic or SQLite write counts.

| Change | Gate / reason | Parsed or reparsed files | Copied files | Families recomputed |
| --- | --- | ---: | ---: | ---: |
| Zero delta | Incremental, no new generation | 0 | 0 | 0 |
| One Python body edit | Equal verified interface | 1 | 20 | 1 |
| Interface-preserving comment | Equal verified interface | 1 | 20 | 1 |
| Python interface change | `python_interface_changed` | 21 | 0 | 1 |
| Python add | `project_context_changed` | 22 | 0 | 1 |
| Python remove | `project_context_changed` | 21 | 0 | 1 |
| Project config change | `project_context_changed` | 21 | 0 | 1 |
| One Rust edit | File-local incremental | 1 | 20 | 1 |
| One TS/JS edit | File-local incremental | 1 | 20 | 1 |

Modified conftest files force full context before interface probing because
ancestor fixture projections affect a subtree. Missing/unverified interface
hashes force conservative rebuild. A Python edit near the context admission
boundary still uses the existing six-times JSON-escape bound and can fall back
with `python_context_budget`; sessions do not bypass or weaken that decision.
Those negative cases pass the 14-scenario sync-equivalence oracle.

The current closure is:

1. Hash discovery determines changed files and the context/interface gate.
2. On incremental admission, unchanged file/unit/IR/fact/dependency/interface
   records copy forward; the changed file's owned records are replaced.
3. Semantic/support derivation consumes the complete resulting unit/fact set.
4. `record_family_claims` runs family construction over that complete set and
   writes every returned family/member/slot/evidence/profile. Its public counter
   is the number of returned family claims, not a dirty-bucket count. Thus the
   fixture's `1` does not prove that one affected bucket was selectively rebuilt.

The incremental `semantic_facts` DTO is unsuitable as an actual fact/work
count: its implementation combines the copied-record count with an allocator
high-water offset (`next_fact_offset`), rather than a fresh row total. The N16
body DTO reports 605 versus full indexing's 329. Preserve these raw observations
as reported counters, not actual fact totals or measured SQLite rows written;
do not rank optimizations from them. Complete stored-row fingerprints are the
independent correctness evidence. Interface/config changes currently consider all modules
dependent rather than traversing a persisted reverse-import closure. This is
conservative behavior, not evidence that every module's facts actually changed.
Self zero-delta sync performs no frontend work but still takes about two seconds
on this machine; discovery/hash/validation costs remain independently unmeasured.

## Ranked follow-up obligations

The highest measured opportunity after transport is a Python interface change:
one changed file still reparses every admitted file. A bounded reverse repo-local
import closure is the next candidate; it needs persisted import/reexport
dependencies and conservative handling of ambiguous imports, stars, missing
roots and context-budget transitions. Preregister and test it before narrowing.

Conftest subtree closure, config-projection invalidation, dirty family buckets
and unaffected-family/evidence copy-forward are separate follow-ups. Each must
compare canonical full versus incremental results, retain stale/UNKNOWN and
rollback behavior, and quantify actual work. Do not combine them with the
worker protocol, remove global checks, or infer closure from fewer parser calls.

Ignored autosync bursts currently generate two zero-delta sync attempts and no
activation. Native-event filtering could be studied separately, but this sprint
makes no idle or battery improvement claim. Query cost is covered in its
[independent lane](query-serving-efficiency.md); provider/language completion
and live agent adoption remain separate obligations.
