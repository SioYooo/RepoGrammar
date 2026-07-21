# Quickstart

This guide keeps source, candidate, and public release state separate. A source
manifest or Git tag does not prove that either public registry is ready.

## 1. Verify exact stable availability

```bash
npm view @sioyooo/repogrammar@0.4.1 version
npm view @sioyooo/repogrammar dist-tags --json
curl -fsSI https://github.com/SioYooo/RepoGrammar/releases/download/v0.4.1/install.sh.sha256
npx --yes --package @sioyooo/repogrammar@0.4.1 \
  repogrammar version
```

Use the no-build path only when the exact package and GitHub asset exist and
the dist-tags are `latest=0.4.1` and `preview=0.2.0-preview.0`. The preview tag
must continue to resolve the immutable historical preview; stable publication
must not rewrite it.

The npm package is a thin launcher. It downloads and verifies the matching
native archive, then executes the Rust binary; it does not compile Rust and it
does not install a bare `repogrammar` command onto `PATH`. Use the full pinned
`npx --yes --package ... repogrammar` form for reproducible commands.

## 2. Set up one repository

Python 3.10 or newer is required for the bundled bounded Python analyzer. The
release path does not require Rust/Cargo, Docker, an LLM, embeddings, a vector
database, or an API key.

```bash
npx --yes --package @sioyooo/repogrammar@0.4.1 \
  repogrammar setup --project /path/to/repo --target auto
```

Setup reviews one plan, configures a safely owned detected Codex or Claude Code
integration, initializes and indexes the repository, starts that repository's
autosync daemon, and runs the read-only product MCP self-test. Foreign,
malformed, or drifted agent configuration is preserved and reported.

For CI or a deterministic one-shot index without a daemon:

```bash
npx --yes --package @sioyooo/repogrammar@0.4.1 \
  repogrammar setup --project /path/to/repo --target auto \
  --yes --no-autosync --progress never
```

`init` is the repository-only entrypoint when machine integration is already
configured. It starts repo-local autosync by default; add `--no-autosync` for a
one-shot index. RepoGrammar does not run a global repository scanner.

## 3. Ask for bounded context

```bash
npx --yes --package @sioyooo/repogrammar@0.4.1 \
  repogrammar find "FastAPI route" \
  --project /path/to/repo --mode compact --verbosity minimal

npx --yes --package @sioyooo/repogrammar@0.4.1 \
  repogrammar check "path/to/file.py:LINE" \
  --project /path/to/repo --mode compact --verbosity minimal
```

Consume the returned `read_plan` before broad source reads. A successful
`check` is a static-alignment certificate; it always keeps
`runtime_equivalence: UNKNOWN`. `UNKNOWN` and `PARTIAL_CONTEXT` are normal
typed results with source fallback or sync recovery, not silent failures.

If query-time hashes reject stale evidence, refresh explicitly:

```bash
npx --yes --package @sioyooo/repogrammar@0.4.1 \
  repogrammar sync --project /path/to/repo
```

Autosync is a best-effort convenience. Explicit sync is the authoritative
refresh path.

## 4. Optional permanent managed command

After the public asset check succeeds, download and verify `install.sh` and its
checksum from the exact `v0.4.1` release, then run:

```bash
bash install.sh --version v0.4.1 --install-cli-only --yes
repogrammar version
```

The installer acquires the same verified native archive and bundled worker. It
may also configure supported agents when explicitly requested; it never creates
repository `.repogrammar/` state on its own.

Contributor-only source builds remain available from a checkout:

```bash
cargo build --release
bash src/install/repogrammar-install.sh \
  --install-cli-only --from-source --yes
```

## 5. Cleanup on receipt-aware current source

```bash
repogrammar uninstall --dry-run
repogrammar uninstall --yes
```

Bare `uninstall` removes only the first-party managed machine installation and
its receipt-owned agent integrations. It preserves every repository's
`.repogrammar/`, telemetry and research data, unknown global files, and
npm/Cargo or unmanaged PATH copies. To remove one repository index separately:

```bash
repogrammar uninit --project /path/to/repo --yes
```

To disconnect coding agents while keeping the installed product:

```bash
repogrammar disconnect --target all --scope global --dry-run
repogrammar disconnect --target all --scope global --yes
```

The `disconnect` rename and full self-uninstall contract ship in `0.4.1`. The
earlier immutable public `v0.4.0` artifacts predate this breaking pre-1.0
change; do not infer `0.4.1` behavior from those older bytes, and use the
installed binary's help until the `0.4.1` patch-forward release is public.

## Platform and scope boundary

Stable release archives cover:

- macOS arm64 and x86_64;
- glibc Linux x86_64 with glibc 2.35 or newer; and
- glibc Linux arm64 with glibc 2.39 or newer.

Windows, musl Linux, older/unknown libc, and unsupported architectures fail
closed before download. See [limitations](limitations.md),
[installation specification](specifications/installation.md), and the
[Codex quickstart](quickstart-codex.md) for exact boundaries.
