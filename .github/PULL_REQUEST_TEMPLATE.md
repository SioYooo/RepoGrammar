# Summary

-

# Scope

- [ ] Documentation-only
- [ ] CLI/MCP behavior
- [ ] Analyzer/indexing behavior
- [ ] Installer/release/npm
- [ ] Telemetry/metrics
- [ ] Other:

# Evidence And Boundaries

- Positive fixtures or examples:
- Negative fixtures or UNKNOWN cases preserved:
- Claims this PR does not make:

# Validation

Paste commands and results:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
node src/workers/typescript/worker.test.js
node src/npm/repogrammar.test.js
npm_config_cache="${TMPDIR:-/tmp}/repogrammar-npm-cache" npm pack --dry-run
python3 src/workers/python/worker.test.py
bash src/install/repogrammar-install.test.sh
cargo run --quiet --bin repo-guard -- check
git diff --check
cmp -s AGENTS.md CLAUDE.md
```

# Release Notes

- [ ] Changelog/docs updated when public behavior changed.
- [ ] No generated junk, logs, `.repogrammar/`, credentials, private source, or unrelated formatting included.
- [ ] No unsupported token-saving, release/npm availability, language-support, or production-stability claims.
