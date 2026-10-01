# Native-host offline qualification: retained blocked campaign

Date: 2026-10-01. Start: clean `main@c9d4fa3`.
Verdict: **CONTROL_ISOLATION_BLOCKED** after five bounded local rounds.
Real adoption, over-trigger, correctness effects, tokens/cost and actual B2
global-file discovery remain **NOT_MEASURED**. B2/B3 are **NOT_RUN**.
GitHub 0.5.0/npm 0.4.3 and v0.6.0 NO_GO are unchanged.

## Contract and fixture

Continue the [offline tool/oracle slice](agent-adoption-v2-qualification.md)
by testing one actual native host, without real model, credentials or spend.
Freeze producer/input identities before dispatch; deny direct index acquisition
in every arm, keep harness-only product access and preserve negative results.
Stop on a failed control or the five-round cap. The source fixture is an
existing three-route development control, not held-out task evidence.

[`native_host_v2.py`](../../src/experiments/agent_study/native_host_v2.py) copies
and pins Claude Code **2.1.286**, SHA-256
`75e3016e9d2570767b08e43a7467d4817a4f149232c169ca295f2c95fef21433`.
It uses fresh HOME/config/XDG directories, a fixed environment allowlist,
the current exact 825-byte v4 artifact and identical seeded source/index bytes.
The CLI receives a nonsecret dummy key and an exact loopback fixture endpoint;
there is no real-provider/auth switch. Scripted responses deliberately select
tools and supply synthetic usage. Neither these choices nor the native CLI's
calculated synthetic cost are study outcomes.

The native process runs under deny-default Seatbelt. Source/profile/scratch
paths and exact runtime helpers are admitted; `.repogrammar/`, hidden artifacts,
the product executable and other profiles are denied. Only the fixture's TCP
port and, outside B0, one exact MCP Unix socket are admitted. Mach access stays
denied. B2's guide and configuration/helper paths are protected from writes.

[`mcp_bridge_v2.py`](../../src/experiments/agent_study/mcp_bridge_v2.py) launches
the pinned read-only product outside the sandbox. One bounded client relays
initialize/initialized/ping/list/context calls; unknown methods/tools, malformed
frames and id reuse fail closed. Product payloads/errors remain authoritative.
The current product's unsupported ping error is preserved. Optional raw client
audit is private and requires explicit repository authority when copied; the
coordinator freezes that guard in the read-only MCP configuration.

[`security_service_probe_v2.py`](../../src/experiments/agent_study/security_service_probe_v2.py)
checks only acquisition/deallocation of the fixed `com.apple.SecurityServer`
Mach right. An outside positive lookup must succeed and release its right;
the inside lookup must return NOT_PRIVILEGED without a handle. It sends no
service messages and reads no credential/profile contents. This establishes
that named capability route, not every possible credential API.

## Five-round evidence

| Round | Observed failure / conservative change | Retained outcome |
| --- | --- | --- |
| 1 | Exact TCP SBPL address syntax rejected | Native host NOT_RUN; no control bypass |
| 2 | Loopback syntax fixed; native scratch tried shared `/tmp/claude-501` | Kernel 22/22 and SecurityServer PASS; shared directory remained denied |
| 3 | Documented CLAUDE_CODE_TMPDIR used with private 0700 scratch | Native acquisition/denial routes PASS; builtin registries and MCP transport remained unqualified |
| 4 | Supported builtin plugin/agent settings; bounded client audit | Empty registries confirmed; copied-client audit root bug retained and diagnosed |
| 5 | Explicit audit repository authority; exact captured native clientInfo metadata admitted | Native initialize/notification/tools-list succeed, but tools/call never reaches product; BLOCKED |

The [campaign summary](data/agent-adoption-native-host.summary.json) retains
the original fifth-round producer hash. B0 and B1 each pass 22 kernel controls,
the paired SecurityServer check and 12 native acquisition/denial controls.
Both native manifests contain zero plugins, agents and skills. B0 exposes four
built-in tools and no MCP; B1 exposes those four plus the one configured MCP.
The native terminal events still report success, while the coordinator correctly
rejects B1 because its actual tool call reports `Connection closed`.

Real native initialize metadata includes title/description/websiteUrl in
clientInfo. The bridge accepts only those observed optional fields alongside
required name/version, bounded as inert metadata. This changes no product tool,
schema, support policy or release artifact.

Raw files and original producer copies are retained outside Git under
`/private/tmp/repogrammar-native-host-20261001/`. Source, native bodies, prompts,
patches, raw MCP text and SDK binaries are absent from the committed summary.
All attempts are retained. No sixth native campaign was run.

## Locally tested follow-ups, not native requalification

The native failure led to a synthetic delayed-stdio reproduction: an inherited
nonblocking stdin can report would-block as empty buffered input. A duplex
stdio test also refuted a one-time blocking-mode workaround because stdout
flag changes can affect the aliased input description. The candidate uses a
shared bounded fd reader instead. These regressions explain a plausible
transport cause; they do not establish the repaired native result.

Independent review additionally reproduced two candidate defects: a descendant
ignoring TERM could survive a completed parent, and an untyped fingerprint could
confuse file bytes with symlink target bytes or ignore a FIFO. The candidate
adds bounded TERM/KILL process-group cleanup, type-tagged file/symlink hashes
and special-file rejection, with focused regressions. Process-group cleanup
does not independently prove confinement of an arbitrary descendant tree.

These changes have **LOCALTESTED_ONLY** evidence and a different producer hash
from the archived native run. They must not replace the old run's identity or
convert its BLOCKED verdict into PASS. Portable/product bridge tests and normal
repository gates validate the committed candidate; they do not close actual
native tools/call, B2 discovery, B3 or Linux-native qualification.

## Next gate

Freeze this repaired candidate for a fresh bounded native campaign, reproduce
the actual MCP call after deferred tool dispatch, then finish B2 global-file
discovery/B3 controls. Keep the old failures and producer archive. A separately
held-out task/oracle/index freeze and explicit real budget/auth approval are
still required before any adoption/effect wave.

Current host flags/temp/memory behavior were checked against the pinned local
CLI and first-party [CLI reference](https://code.claude.com/docs/en/cli-reference),
[environment reference](https://code.claude.com/docs/en/env-vars),
[memory documentation](https://code.claude.com/docs/en/memory) and
[plugin settings](https://code.claude.com/docs/en/settings). These sources
support the setup; retained native captures decide what actually occurred.
