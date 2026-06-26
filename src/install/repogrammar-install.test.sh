#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
INSTALLER="${SCRIPT_DIR}/repogrammar-install.sh"
TMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TMP_ROOT"' EXIT

TARGET="$("$INSTALLER" --print-target)"
RELEASE_DIR="${TMP_ROOT}/release"
PACKAGE_DIR="${TMP_ROOT}/package"
COMMAND_DIR="${TMP_ROOT}/bin"
INSTALL_DIR="${TMP_ROOT}/share/repogrammar"
LOG_FILE="${TMP_ROOT}/fake-repogrammar.log"
mkdir -p "$RELEASE_DIR" "$PACKAGE_DIR" "$COMMAND_DIR"
mkdir -p "${PACKAGE_DIR}/workers/python"

cat > "${PACKAGE_DIR}/repogrammar" <<'FAKE'
#!/usr/bin/env sh
case "${1:-}" in
  version)
    echo "repogrammar 0.1.0-test"
    ;;
  install|uninstall)
    if [ -n "${REPOGRAMMAR_FAKE_LOG:-}" ]; then
      printf '%s' "$1" >> "$REPOGRAMMAR_FAKE_LOG"
      shift
      for arg in "$@"; do
        printf ' %s' "$arg" >> "$REPOGRAMMAR_FAKE_LOG"
      done
      printf '\n' >> "$REPOGRAMMAR_FAKE_LOG"
    fi
    ;;
  *)
    echo "unexpected fake repogrammar command: ${1:-}" >&2
    exit 2
    ;;
esac
FAKE
chmod +x "${PACKAGE_DIR}/repogrammar"
printf 'print("fake worker")\n' > "${PACKAGE_DIR}/workers/python/worker.py"

ARTIFACT="repogrammar-${TARGET}.tar.gz"
tar -czf "${RELEASE_DIR}/${ARTIFACT}" -C "$PACKAGE_DIR" repogrammar workers
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$RELEASE_DIR" && sha256sum "$ARTIFACT" > "${ARTIFACT}.sha256")
else
  (cd "$RELEASE_DIR" && shasum -a 256 "$ARTIFACT" > "${ARTIFACT}.sha256")
fi

REPOGRAMMAR_RELEASE_DIR="$RELEASE_DIR" \
REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$INSTALL_DIR" \
"$INSTALLER" --install-cli-only --yes >/dev/null

"${COMMAND_DIR}/repogrammar" version | grep -q "repogrammar 0.1.0-test"
test -f "${TMP_ROOT}/share/repogrammar/workers/python/worker.py"
test -x "${TMP_ROOT}/share/repogrammar/bin/repogrammar"

REPOGRAMMAR_RELEASE_DIR="$RELEASE_DIR" \
REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$INSTALL_DIR" \
REPOGRAMMAR_FAKE_LOG="$LOG_FILE" \
"$INSTALLER" --install-and-configure --yes --target codex >/dev/null

grep -q "install --target codex --scope global --yes --no-telemetry" "$LOG_FILE"

REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$INSTALL_DIR" \
REPOGRAMMAR_FAKE_LOG="$LOG_FILE" \
"$INSTALLER" --uninstall-agents --yes --target all >/dev/null

grep -q "uninstall --target all --scope global --yes" "$LOG_FILE"

REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$INSTALL_DIR" \
"$INSTALLER" --uninstall-command --yes >/dev/null

if [[ -e "${COMMAND_DIR}/repogrammar" ]]; then
  echo "repogrammar command was not removed" >&2
  exit 1
fi

SOURCE_COMMAND_DIR="${TMP_ROOT}/source-bin"
SOURCE_INSTALL_DIR="${TMP_ROOT}/source-data"
SOURCE_LOG="${TMP_ROOT}/source-fake.log"
REPOGRAMMAR_SOURCE_BINARY="${PACKAGE_DIR}/repogrammar" \
REPOGRAMMAR_COMMAND_DIR="$SOURCE_COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$SOURCE_INSTALL_DIR" \
"$INSTALLER" --install-cli-only --from-source --yes >/dev/null

"${SOURCE_COMMAND_DIR}/repogrammar" version | grep -q "repogrammar 0.1.0-test"
test -x "${SOURCE_INSTALL_DIR}/bin/repogrammar"

REPOGRAMMAR_SOURCE_BINARY="${PACKAGE_DIR}/repogrammar" \
REPOGRAMMAR_COMMAND_DIR="$SOURCE_COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$SOURCE_INSTALL_DIR" \
REPOGRAMMAR_FAKE_LOG="$SOURCE_LOG" \
"$INSTALLER" --install-and-configure --from-source --yes --target all >/dev/null

grep -q "install --target all --scope global --yes --no-telemetry" "$SOURCE_LOG"

FOREIGN_COMMAND_DIR="${TMP_ROOT}/foreign-bin"
FOREIGN_INSTALL_DIR="${TMP_ROOT}/foreign-data"
mkdir -p "$FOREIGN_COMMAND_DIR"
printf 'foreign\n' > "${FOREIGN_COMMAND_DIR}/repogrammar"
chmod +x "${FOREIGN_COMMAND_DIR}/repogrammar"
FOREIGN_ERR="${TMP_ROOT}/foreign.err"
set +e
REPOGRAMMAR_SOURCE_BINARY="${PACKAGE_DIR}/repogrammar" \
REPOGRAMMAR_COMMAND_DIR="$FOREIGN_COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$FOREIGN_INSTALL_DIR" \
"$INSTALLER" --install-cli-only --from-source --yes >"${TMP_ROOT}/foreign.out" 2>"$FOREIGN_ERR"
FOREIGN_STATUS=$?
set -e
if [[ "$FOREIGN_STATUS" -eq 0 ]]; then
  echo "foreign command path unexpectedly succeeded" >&2
  exit 1
fi
grep -q "not managed by RepoGrammar" "$FOREIGN_ERR"
grep -q "foreign" "${FOREIGN_COMMAND_DIR}/repogrammar"

FAKE_PATH="${TMP_ROOT}/fake-path"
mkdir -p "$FAKE_PATH"
cat > "${FAKE_PATH}/curl" <<'FAKE_CURL'
#!/usr/bin/env sh
exit 22
FAKE_CURL
chmod +x "${FAKE_PATH}/curl"

NO_RELEASE_ERR="${TMP_ROOT}/no-release.err"
set +e
PATH="${FAKE_PATH}:$PATH" \
REPOGRAMMAR_COMMAND_DIR="${TMP_ROOT}/no-release-bin" \
REPOGRAMMAR_INSTALL_DIR="${TMP_ROOT}/no-release-data" \
"$INSTALLER" --install-cli-only --yes >"${TMP_ROOT}/no-release.out" 2>"$NO_RELEASE_ERR"
NO_RELEASE_STATUS=$?
set -e
if [[ "$NO_RELEASE_STATUS" -eq 0 ]]; then
  echo "missing release artifact path unexpectedly succeeded" >&2
  exit 1
fi
grep -q "release artifact was not found" "$NO_RELEASE_ERR"
grep -q -- "--from-source" "$NO_RELEASE_ERR"
grep -q "REPOGRAMMAR_RELEASE_DIR" "$NO_RELEASE_ERR"
