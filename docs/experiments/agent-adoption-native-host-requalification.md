# Native-host offline requalification (preregistration)

Date: 2026-10-22. Start: clean `main@d48008f`, branch
`feat/native-host-requalification`. Status: **PREREGISTERED, NOT_RUN**.
Real model, credentials, provider network, adoption, over-trigger, correctness,
tokens and cost are **NOT_MEASURED** by construction and stay so. Scripted
fixture success is a control result, never an adoption effect.

This is a **new campaign with a new producer identity**, not a rerun. The
[2026-10-01 campaign](agent-adoption-native-host-qualification.md) (Claude Code
2.1.286, CONTROL_ISOLATION_BLOCKED) is unedited and remains its own record.
Its next gate asked for a fresh bounded campaign on the repaired candidate;
this is that campaign.

## Evidence loss and why a rerun is impossible

- The pinned Claude Code 2.1.286 copy is gone. `~/.local/share/claude/versions/`
  holds only 2.1.302, 2.1.303 and 2.1.304; `claude --version` is 2.1.304. The
  harness correctly hard-fails with "CLI drift" against the old hash.
- The raw archive `/private/tmp/repogrammar-native-host-20261001/` no longer
  exists (tmp was cleaned). The old per-round raw captures, original producer
  copies and transcripts are **lost**. Only the committed
  [summary](data/agent-adoption-native-host.summary.json) and prose survive.
  This is recorded as an evidence loss; the old numbers are not re-derivable.

## New producer identity (frozen before any run)

| Item | Value |
| --- | --- |
| Claude Code | 2.1.304, `/Users/sioyoo/.local/share/claude/versions/2.1.304`, SHA-256 `b9ac5374839f14abf3f1f45610637c332b5daee5ce0d1634726cef87aab78e6a` |
| Product binary | `cargo build --release` on this branch at `d48008f`, SHA-256 `9f75481081200f2c5cbd20611af4fd1ad067b0bac57fe02349f00e1f0c1a66fb` |
| Guide artifact | v4 global block regenerated from `GLOBAL_AGENT_PREFLIGHT` in `src/rust/application/install.rs`: 824 bytes, SHA-256 `5df98f0677189042c7ae58aa74d6e7ac90bd99324a18edf263f9be8db7b4b256` (equals the harness-pinned hash; earlier prose says 825 bytes, the pinned hash is what binds) |
| Harness at preregistration | `native_host_v2.py` `ac0b1ad2...2873`, `mcp_bridge_v2.py` `75bce279...4b84c`, `security_service_probe_v2.py` `50186480...dbced`, `isolation_probe_v2.py` `ecef4bd8...2662`, `runner_v2.py` `94fc4afc...c615` (full hashes in the retained freeze) |

The only permitted change before round 1 is the CLI re-pin in
`native_host_v2.py` (version string and `EXPECTED_CLI_SHA256`) with its
regression test; the drift check stays strict. Final harness hashes are in the
results section and in each run's `freeze.json`.

Re-pin (code commit following this preregistration): `native_host_v2.py` now
carries `EXPECTED_CLI_VERSION = "2.1.304"` and the 2.1.304 SHA-256 above; a
regression test asserts the pin and that a non-matching CLI still fails with
"CLI drift". The drift check is unchanged in strictness. Post-re-pin
`native_host_v2.py` SHA-256: `00fcbc78a332d7fa05c3fb830866e15941432c9be18a1ae9d325c9f48c728985`.

## Retention (outside Git, outside /private/tmp)

`/Volumes/External/Development/repogrammar-evidence/native-host-20261022/`
holds: `inputs/` (guide artifact), `round-N/` per-attempt output directories
(copied CLI runtime, freeze.json, per-arm native transcripts, stderr, MCP bridge
frames, sandbox profiles, qualification.summary.json) and a hash manifest.
Nothing from it is committed; only hashes and the verdict summary are.

## Bounds, controls and rules

- At most **5** autonomous rounds. Each names the previous failure class and
  makes one conservative harness/bridge-only change with a negative test.
  Forbidden: product/MCP behaviour changes, Seatbelt weakening, access to the
  checkout, `.repogrammar/`, the product executable or other profiles, any real
  provider, key or login, or network beyond the fixture's loopback port.
- Controls are the same as before: B0/B1/B2/B3 arms; 22 kernel controls and the
  paired SecurityServer check per arm; 12 native acquisition/denial controls
  (positive read/grep/glob/bash/sqlite; denied index read/grep/glob/bash/sqlite,
  private file, product binary); empty plugin/agent/skill registries; exact
  tool inventory (four built-ins, plus the one MCP tool only where enabled);
  exact captured request audit for global-file discovery (PRESENT only in B2);
  protected-file and worktree integrity.
- Stop early on G5 success, an authority/isolation blocker, or an environment
  failure that cannot be fixed safely (for example an interactive login or
  network the CLI needs). Such a stop is recorded, not worked around. A product
  defect blocking G3 is the finding; the product is not changed.

## Decisive gates, in order

| Gate | Pass condition |
| --- | --- |
| G1 | B0 and B1 pass kernel, SecurityServer and native acquisition/denial controls |
| G2 | Native `initialize`, `notifications/initialized` and `tools/list` reach the bridge in B1 |
| G3 | B1 `tools/call` reaches the real product exactly once and the product payload is returned unchanged through the bridge (no `Connection closed`, no PRODUCT_ERROR) |
| G4 | B2 controls pass, with the global guide actually present in captured requests |
| G5 | B3 controls pass (explicit-use prefix delivered; guide absent) |

A gate is evaluated only if the previous passed; later gates are NOT_RUN.

## Failure classes (what each would mean)

- **ENV_FAILURE**: host precondition outside the harness (login prompt, network
  need, missing runtime, disk). Not evidence about the product or the design.
- **CONTROL_ISOLATION_BLOCKED**: a control leaked, or a denial/inventory/guide
  check failed. Isolation is unproven; no later gate may run.
- **TRANSPORT**: native host and bridge disagree on the MCP wire (framing,
  lifecycle, closed connection, dispatch). Harness or CLI behaviour, not product.
- **HARNESS_DEFECT**: assessor/parser/fingerprint bug demonstrated by the
  retained raw files against a correct native behaviour.
- **CLI_DRIFT**: the CLI bytes or observed protocol differ from the pinned
  identity; the run is void and requires a new identity.

Verdict vocabulary: PASS per gate, BLOCKED, NOT_RUN, NOT_MEASURED.

## Round log (harness changes, in order)

- Round 1 failure: kernel 0/22. Homebrew Python 3.14.6 `Python` framework is
  loaded by dyld from its resolved `Cellar` path, but the profile admitted the
  `opt` symlink prefix. Class: HARNESS_DEFECT (interpreter path). Change:
  `runner_v2.BASE_PREFIX` is `sys.base_prefix` resolved; same read scope.
- Round 2 failure: kernel 0/22 again, `_sqlite3` could not load Homebrew's
  `libsqlite3` (read blocked). Class: HARNESS_DEFECT (runtime dependency).
  Change: the profile admits exactly the resolved non-system dylib files that
  the stdlib sqlite extension links (`otool -L`), as literal reads, plus
  metadata on their ancestors. No directory subpath, no data or product path.
- Rounds 3 and 4 failure: identical kernel 0/22 (import blocked). The resolved
  file literal alone (round 3), then the linked symlink hops (round 4), were not
  enough: dyld also opens the alias symlink in the real `Cellar` directory.
  Class: HARNESS_DEFECT. The round-4 attempt was started before the change was
  verified in isolation, which wasted a round. Change: also admit the real
  directory's alias literal. Before round 5 the generated host profile was
  verified with a Homebrew-sqlite import and database open under
  `sandbox-exec`, outside the campaign. Separately, the older offline
  `runner_v2` qualification (not part of the native campaign) still fails
  under Homebrew Python 3.14 because its profile lacks worktree-ancestor
  metadata (sqlite open fails); the native host profile has it. Recorded, not
  fixed here.

## Results

Verdict: **BLOCKED (TRANSPORT)** at the five-round cap. G1 PASS (B0 only; B1
control portion passes but its inventory check fails), G2 BLOCKED, G3 NOT_RUN,
G4 NOT_RUN, G5 NOT_RUN. Real model, adoption, tokens and cost: NOT_MEASURED.
No sixth round was run.

| Round | Previous failure class | Conservative change | Outcome |
| --- | --- | --- | --- |
| 1 | none (first run) | none (pinned harness at 2.1.304) | HARNESS_DEFECT: kernel 0/22, Cellar-resolved Python framework not admitted |
| 2 | HARNESS_DEFECT | resolved interpreter prefix | HARNESS_DEFECT: kernel 0/22, Homebrew libsqlite3 not admitted; SecurityServer PASS |
| 3 | HARNESS_DEFECT | exact resolved sqlite dylib literal + ancestor metadata | HARNESS_DEFECT: kernel 0/22 |
| 4 | HARNESS_DEFECT | symlink hops of the linked path | HARNESS_DEFECT: kernel 0/22 (wasted: not isolated-verified first) |
| 5 | HARNESS_DEFECT | real-directory alias literal | B0 PASS; B1 BLOCKED, TRANSPORT |

Gates:

| Gate | Result | Evidence |
| --- | --- | --- |
| G1 B0/B1 controls | PASS for B0 (22/22 kernel, SecurityServer, 12/12 native, empty registries, 4 tools, no MCP). B1: 22/22, 11/12 native controls, only the MCP call fails | round 5 |
| G2 initialize + tools/list reach bridge | BLOCKED | bridge saw one connection and zero frames; init manifest `mcp_servers[repogrammar].status = "failed"`; B1 exposes 4 tools |
| G3 tools/call reaches product | NOT_RUN | not reached |
| G4 B2 global-file discovery | NOT_RUN | B1 failed first |
| G5 B3 | NOT_RUN | |

Diagnosis (from retained raw files, not yet fixed): Claude Code 2.1.304 opens
its MCP session with a pre-`initialize` probe, `server/discover` (protocol
`2026-07-28`, `io.modelcontextprotocol/*` `_meta`; captured in the private
client audit). The 2.1.286 bridge allowlist is `initialize`,
`notifications/initialized`, `ping`, `tools/list`, `tools/call`, so it fails
closed on the probe and the client marks the server failed without ever
sending `initialize`. This is a new native protocol behaviour of the pinned
CLI, distinct from the 2.1.286 tools/call failure. The product answers the
same method with JSON-RPC -32601 "unsupported MCP JSON-RPC method" (checked
directly), which a client could fall back on. This is a harness/bridge finding;
the product is not a defect here and was not changed.

Separately, the older `runner_v2` offline qualification (not part of this
campaign) fails under Homebrew Python 3.14 for sqlite open (its profile lacks
worktree-ancestor metadata that the native host profile adds). Recorded only.

Identity actually used: Claude Code 2.1.304 SHA-256 `b9ac5374...ab78e6a`
(full value above); product `9f754810...66fb`; guide `5df98f06...b256`; bridge `75bce279...b84c` and probes unchanged from
preregistration; final `native_host_v2.py` `00fcbc78...8985`, final
`runner_v2.py` `697dcf1a...6696` (changed in rounds 1-5; full hashes in
`harness-final.sha256`). Raw evidence (all five rounds, logs, the offline
sqlite preflight, hash manifest) is under
`/Volumes/External/Development/repogrammar-evidence/native-host-20261022/`,
untracked and outside the repository.

Next gate (a new, separately bounded campaign): let the bridge relay
`server/discover` to the product unchanged so the product's -32601 reaches the
client, with a negative test that other unknown methods still fail closed;
then rerun G2/G3 and finish B2/B3. Real effects still require a held-out task
freeze and explicit budget/auth approval.

## Campaign 2 preregistration (2026-10-22)

Previous failure class: **TRANSPORT** from campaign 1 round 5. Claude Code
2.1.304 sends `server/discover` (protocol `2026-07-28`, `_meta` keys
`io.modelcontextprotocol/protocolVersion`, `.../clientInfo`,
`.../clientCapabilities`) before `initialize`; the bridge allowlist closes on it
and the client reports the server failed. Same identity as campaign 1 except the
bridge hash. Branch `feat/native-host-requalification`; evidence under
`/Volumes/External/Development/repogrammar-evidence/native-host-20261022/campaign-2/`
(untracked). Cap: **3 rounds**. Same constraints: scripted loopback only, no
real model/credential/network/spend, Seatbelt denials and product unchanged.

The one change: `mcp_bridge_v2.py` admits `server/discover` as a request with an
id and only the observed `_meta` keys, counts it, and relays it to the product
unchanged so the product's own JSON-RPC -32601 (verified directly) reaches the
client. Negative tests: every other unknown method, malformed frame, extra
params/`_meta` keys on `server/discover` and id reuse still fail closed before
the product.

Gates in order: G2 initialize + tools/list reach the bridge (B1); G3 B1
tools/call reaches the product once and its payload returns unchanged; G4 B2
global-file discovery controls; G5 B3 controls. If the client still fails the
server after -32601, retain the raw frames and classify TRANSPORT vs
CLIENT_PROTOCOL; at most one further bridge-side change, only if the frames name
a specific next required method; otherwise stop BLOCKED with the frames.
Effects remain NOT_MEASURED.
