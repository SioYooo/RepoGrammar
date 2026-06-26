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
"$INSTALLER" --install-cli-only --yes >/dev/null

"${COMMAND_DIR}/repogrammar" version | grep -q "repogrammar 0.1.0-test"
test -f "${TMP_ROOT}/share/repogrammar/workers/python/worker.py"

REPOGRAMMAR_RELEASE_DIR="$RELEASE_DIR" \
REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
REPOGRAMMAR_FAKE_LOG="$LOG_FILE" \
"$INSTALLER" --install-and-configure --yes --target codex >/dev/null

grep -q "install --target codex --scope global --yes --no-telemetry" "$LOG_FILE"

REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
REPOGRAMMAR_FAKE_LOG="$LOG_FILE" \
"$INSTALLER" --uninstall-agents --yes --target all >/dev/null

grep -q "uninstall --target all --scope global --yes" "$LOG_FILE"

REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
"$INSTALLER" --uninstall-command --yes >/dev/null

if [[ -e "${COMMAND_DIR}/repogrammar" ]]; then
  echo "repogrammar command was not removed" >&2
  exit 1
fi
