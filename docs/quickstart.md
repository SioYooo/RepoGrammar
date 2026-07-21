# Quickstart

This guide keeps source, candidate, and public release state separate. A source
manifest or Git tag does not prove that either public registry is ready.

## 1. Verify exact stable availability

```bash
npm view @sioyooo/repogrammar@0.4.3 version
npm view @sioyooo/repogrammar dist-tags --json
curl -fsSI https://github.com/SioYooo/RepoGrammar/releases/download/v0.4.3/install.sh.sha256
npx --yes --package @sioyooo/repogrammar@0.4.3 repogrammar version
```

Use the no-build path only when the exact package and GitHub asset exist and
the dist-tags are `latest=0.4.3` and `preview=0.2.0-preview.0`. The preview tag
must continue to resolve the immutable historical preview.

Python 3.10 or newer is required for the bundled bounded Python analyzer. The
release path does not require Rust/Cargo, Docker, an LLM, embeddings, a vector
database, or an API key.

## 2. Download and install the binary

Download the exact release installer, verify the installer asset itself, then
let it download and checksum-verify the matching native binary and worker:

```bash
curl -fsSLO https://github.com/SioYooo/RepoGrammar/releases/download/v0.4.3/install.sh
curl -fsSLO https://github.com/SioYooo/RepoGrammar/releases/download/v0.4.3/install.sh.sha256
shasum -a 256 -c install.sh.sha256
bash install.sh --version v0.4.3 --install-cli-only --yes
export PATH="$HOME/.local/bin:$PATH"
repogrammar version
```

On Linux, use `sha256sum -c install.sh.sha256` for the checksum step.
`--install-cli-only` installs the managed binary, bundled Python worker, and
product receipt. It does not configure coding agents and does not create
repository `.repogrammar/` state.

## 3. Optionally connect a coding agent

Skip this step for CLI-only use. To configure detected supported coding agents
for the read-only MCP server:

```bash
repogrammar install --target auto --scope global --yes --no-telemetry
```

This machine-level command does not initialize or index a repository. The
combined `repogrammar setup` journey remains available, but it is not required
by this explicit installation flow.

## 4. Initialize each repository

```bash
cd /path/to/repo
repogrammar init --project "$PWD" --yes
repogrammar status --project "$PWD"
```

`init` creates repository-local state, builds the active index, and starts
repo-local autosync by default. For CI or a deterministic one-shot index:

```bash
repogrammar init --project "$PWD" --yes --no-autosync --progress never
```

RepoGrammar does not run a global repository scanner. Run `init` once for every
repository you want indexed.

## 5. Ask for bounded context

```bash
repogrammar find "FastAPI route" \
  --project "$PWD" --mode compact --verbosity minimal

repogrammar check "path/to/file.py:LINE" \
  --project "$PWD" --mode compact --verbosity minimal
```

Consume the returned `read_plan` before broad source reads. A successful
`check` is a static-alignment certificate; it always keeps
`runtime_equivalence: UNKNOWN`. `UNKNOWN` and `PARTIAL_CONTEXT` are normal
typed results with source fallback or sync recovery, not silent failures.

If query-time hashes reject stale evidence, refresh explicitly:

```bash
repogrammar sync --project "$PWD"
```

Autosync is a best-effort convenience. Explicit sync is the authoritative
refresh path.

## 6. Contributor-only source install

```bash
cargo build --release
bash src/install/repogrammar-install.sh \
  --install-cli-only --from-source --yes
```

## 7. Cleanup

```bash
repogrammar uninstall --dry-run
repogrammar uninstall --yes
```

Bare `uninstall` removes only the first-party managed machine installation and
its receipt-owned agent integrations. It preserves every repository's
`.repogrammar/`, telemetry and research data, unknown global files, and
npm/Cargo or unmanaged PATH copies. Remove one repository index separately:

```bash
repogrammar uninit --project /path/to/repo --yes
```

To disconnect coding agents while keeping the installed product:

```bash
repogrammar disconnect --target all --scope global --dry-run
repogrammar disconnect --target all --scope global --yes
```

The `disconnect` rename and full self-uninstall contract first shipped in
`0.4.1`; use the installed binary's help for its exact lifecycle contract.

## Platform and scope boundary

Stable release archives cover:

- macOS arm64 and x86_64;
- glibc Linux x86_64 with glibc 2.35 or newer; and
- glibc Linux arm64 with glibc 2.39 or newer.

Windows, musl Linux, older/unknown libc, and unsupported architectures fail
closed before download. See [limitations](limitations.md),
[installation specification](specifications/installation.md), and the
[Codex quickstart](quickstart-codex.md) for exact boundaries.
