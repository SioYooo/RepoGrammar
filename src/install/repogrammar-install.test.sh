#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
INSTALLER="${SCRIPT_DIR}/repogrammar-install.sh"
TMP_ROOT="$(mktemp -d)"
RELEASE_BINARY_TO_RESTORE=""
RELEASE_BINARY_BACKUP=""
RELEASE_BINARY_EXISTED=0

restore_release_binary() {
  if [[ -z "$RELEASE_BINARY_TO_RESTORE" ]]; then
    return
  fi
  if [[ "$RELEASE_BINARY_EXISTED" -eq 1 ]]; then
    cp "$RELEASE_BINARY_BACKUP" "$RELEASE_BINARY_TO_RESTORE"
  else
    rm -f "$RELEASE_BINARY_TO_RESTORE"
  fi
  RELEASE_BINARY_TO_RESTORE=""
}

trap 'restore_release_binary; rm -rf "$TMP_ROOT"' EXIT

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
test -f "${COMMAND_DIR}/repogrammar-workers/python/worker.py"
test -x "${TMP_ROOT}/share/repogrammar/bin/repogrammar"

STATE_REPO="${TMP_ROOT}/state-boundary-repo"
STATE_COMMAND_DIR="${TMP_ROOT}/state-boundary-bin"
STATE_INSTALL_DIR="${TMP_ROOT}/state-boundary-data"
mkdir -p "${STATE_REPO}/.repogrammar"
printf 'keep\n' > "${STATE_REPO}/.repogrammar/sentinel"
(
  cd "$STATE_REPO"
  REPOGRAMMAR_RELEASE_DIR="$RELEASE_DIR" \
  REPOGRAMMAR_COMMAND_DIR="$STATE_COMMAND_DIR" \
  REPOGRAMMAR_INSTALL_DIR="$STATE_INSTALL_DIR" \
  "$INSTALLER" --install-cli-only --yes >/dev/null
)
grep -q "keep" "${STATE_REPO}/.repogrammar/sentinel"

REPOGRAMMAR_RELEASE_DIR="$RELEASE_DIR" \
REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$INSTALL_DIR" \
REPOGRAMMAR_FAKE_LOG="$LOG_FILE" \
"$INSTALLER" --install-and-configure --yes --target codex >/dev/null

grep -q "install --target codex --scope global --yes --no-telemetry" "$LOG_FILE"

REPOGRAMMAR_RELEASE_DIR="$RELEASE_DIR" \
REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$INSTALL_DIR" \
REPOGRAMMAR_FAKE_LOG="$LOG_FILE" \
"$INSTALLER" --configure-agents --yes --target "codex,claude-code" --scope local >/dev/null

grep -q "install --target codex,claude-code --scope local --yes --no-telemetry" "$LOG_FILE"

REPOGRAMMAR_RELEASE_DIR="$RELEASE_DIR" \
REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$INSTALL_DIR" \
REPOGRAMMAR_FAKE_LOG="$LOG_FILE" \
"$INSTALLER" --configure-agents --yes --target none >/dev/null

grep -q "install --target none --scope global --yes --no-telemetry" "$LOG_FILE"

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
test -f "${SOURCE_INSTALL_DIR}/workers/python/worker.py"
test -f "${SOURCE_COMMAND_DIR}/repogrammar-workers/python/worker.py"

DEFAULT_SOURCE_COMMAND_DIR="${TMP_ROOT}/default-source-bin"
DEFAULT_SOURCE_INSTALL_DIR="${TMP_ROOT}/default-source-data"
DEFAULT_SOURCE_CARGO_LOG="${TMP_ROOT}/default-source-cargo.log"
FAKE_CARGO_DIR="${TMP_ROOT}/fake-cargo"
RELEASE_BINARY_TO_RESTORE="${SCRIPT_DIR}/../../target/release/repogrammar"
RELEASE_BINARY_BACKUP="${TMP_ROOT}/repogrammar.release.backup"
if [[ -e "$RELEASE_BINARY_TO_RESTORE" ]]; then
  cp "$RELEASE_BINARY_TO_RESTORE" "$RELEASE_BINARY_BACKUP"
  RELEASE_BINARY_EXISTED=1
else
  RELEASE_BINARY_EXISTED=0
fi
mkdir -p "$FAKE_CARGO_DIR"
cat > "${FAKE_CARGO_DIR}/cargo" <<'FAKE_CARGO'
#!/usr/bin/env sh
printf '%s\n' "$*" >> "$REPOGRAMMAR_FAKE_CARGO_LOG"
if [ "${1:-}" = "build" ] && [ "${2:-}" = "--release" ]; then
  mkdir -p "$(dirname "$REPOGRAMMAR_FAKE_RELEASE_BINARY")"
  cp "$REPOGRAMMAR_FAKE_SOURCE_BINARY" "$REPOGRAMMAR_FAKE_RELEASE_BINARY"
  exit 0
fi
exit 1
FAKE_CARGO
chmod +x "${FAKE_CARGO_DIR}/cargo"
PATH="${FAKE_CARGO_DIR}:$PATH" \
REPOGRAMMAR_FAKE_CARGO_LOG="$DEFAULT_SOURCE_CARGO_LOG" \
REPOGRAMMAR_FAKE_SOURCE_BINARY="${PACKAGE_DIR}/repogrammar" \
REPOGRAMMAR_FAKE_RELEASE_BINARY="$RELEASE_BINARY_TO_RESTORE" \
REPOGRAMMAR_COMMAND_DIR="$DEFAULT_SOURCE_COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$DEFAULT_SOURCE_INSTALL_DIR" \
"$INSTALLER" --install-cli-only --from-source --yes >/dev/null

grep -q "build --release" "$DEFAULT_SOURCE_CARGO_LOG"
"${DEFAULT_SOURCE_COMMAND_DIR}/repogrammar" version | grep -q "repogrammar 0.1.0-test"
test -x "${DEFAULT_SOURCE_INSTALL_DIR}/bin/repogrammar"
restore_release_binary

REPOGRAMMAR_SOURCE_BINARY="${PACKAGE_DIR}/repogrammar" \
REPOGRAMMAR_COMMAND_DIR="$SOURCE_COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$SOURCE_INSTALL_DIR" \
REPOGRAMMAR_FAKE_LOG="$SOURCE_LOG" \
"$INSTALLER" --install-and-configure --from-source --yes --target all >/dev/null

grep -q "install --target all --scope global --yes --no-telemetry" "$SOURCE_LOG"

cargo build --quiet --bin repogrammar
PRODUCT_COMMAND_DIR="${TMP_ROOT}/product-bin"
PRODUCT_INSTALL_DIR="${TMP_ROOT}/product-data"
PRODUCT_REPO="${TMP_ROOT}/product-repo"
mkdir -p "$PRODUCT_REPO"
cat > "${PRODUCT_REPO}/app.py" <<'PY_FIXTURE'
def hello():
    return "ok"
PY_FIXTURE
REPOGRAMMAR_SOURCE_BINARY="${SCRIPT_DIR}/../../target/debug/repogrammar" \
REPOGRAMMAR_COMMAND_DIR="$PRODUCT_COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$PRODUCT_INSTALL_DIR" \
"$INSTALLER" --install-cli-only --from-source --yes >/dev/null

test -x "${PRODUCT_INSTALL_DIR}/bin/repogrammar"
test -f "${PRODUCT_INSTALL_DIR}/workers/python/worker.py"
test -f "${PRODUCT_COMMAND_DIR}/repogrammar-workers/python/worker.py"
(cd "$PRODUCT_REPO" && "${PRODUCT_COMMAND_DIR}/repogrammar" init >/dev/null)
(cd "$PRODUCT_REPO" && "${PRODUCT_COMMAND_DIR}/repogrammar" index --progress never >/dev/null)
(cd "$PRODUCT_REPO" && "${PRODUCT_COMMAND_DIR}/repogrammar" families --json >/dev/null)

FOREIGN_COMMAND_DIR="${TMP_ROOT}/foreign-bin"
FOREIGN_INSTALL_DIR="${TMP_ROOT}/foreign-data"
mkdir -p "$FOREIGN_COMMAND_DIR"
printf 'foreign\n' > "${FOREIGN_COMMAND_DIR}/repogrammar"
chmod +x "${FOREIGN_COMMAND_DIR}/repogrammar"
REPOGRAMMAR_SOURCE_BINARY="${PACKAGE_DIR}/repogrammar" \
REPOGRAMMAR_COMMAND_DIR="$FOREIGN_COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$FOREIGN_INSTALL_DIR" \
"$INSTALLER" --install-cli-only --from-source --yes >"${TMP_ROOT}/foreign.out"
"${FOREIGN_COMMAND_DIR}/repogrammar" version | grep -q "repogrammar 0.1.0-test"
shopt -s nullglob
FOREIGN_BACKUPS=("${FOREIGN_COMMAND_DIR}"/repogrammar.unmanaged-backup*)
shopt -u nullglob
if [[ "${#FOREIGN_BACKUPS[@]}" -ne 1 ]]; then
  echo "expected one unmanaged command backup" >&2
  exit 1
fi
grep -q "foreign" "${FOREIGN_BACKUPS[0]}"

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
grep -q -- "--version v0.2.0-preview.0" "$NO_RELEASE_ERR"
grep -q -- "--from-source" "$NO_RELEASE_ERR"
grep -q "REPOGRAMMAR_RELEASE_DIR" "$NO_RELEASE_ERR"

UNEXPECTED_RELEASE="${TMP_ROOT}/unexpected-release"
UNEXPECTED_PACKAGE="${TMP_ROOT}/unexpected-package"
mkdir -p "$UNEXPECTED_RELEASE" "$UNEXPECTED_PACKAGE/workers/python"
cp "${PACKAGE_DIR}/repogrammar" "${UNEXPECTED_PACKAGE}/repogrammar"
cp "${PACKAGE_DIR}/workers/python/worker.py" "${UNEXPECTED_PACKAGE}/workers/python/worker.py"
printf 'unexpected\n' > "${UNEXPECTED_PACKAGE}/unexpected.txt"
tar -czf "${UNEXPECTED_RELEASE}/${ARTIFACT}" -C "$UNEXPECTED_PACKAGE" repogrammar workers unexpected.txt
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$UNEXPECTED_RELEASE" && sha256sum "$ARTIFACT" > "${ARTIFACT}.sha256")
else
  (cd "$UNEXPECTED_RELEASE" && shasum -a 256 "$ARTIFACT" > "${ARTIFACT}.sha256")
fi
UNEXPECTED_ERR="${TMP_ROOT}/unexpected.err"
set +e
REPOGRAMMAR_RELEASE_DIR="$UNEXPECTED_RELEASE" \
REPOGRAMMAR_COMMAND_DIR="${TMP_ROOT}/unexpected-bin" \
REPOGRAMMAR_INSTALL_DIR="${TMP_ROOT}/unexpected-data" \
"$INSTALLER" --install-cli-only --yes >"${TMP_ROOT}/unexpected.out" 2>"$UNEXPECTED_ERR"
UNEXPECTED_STATUS=$?
set -e
if [[ "$UNEXPECTED_STATUS" -eq 0 ]]; then
  echo "unexpected release path unexpectedly succeeded" >&2
  exit 1
fi
grep -q "unsafe or unexpected path" "$UNEXPECTED_ERR"
if [[ -e "${TMP_ROOT}/unexpected-bin/repogrammar" || -e "${TMP_ROOT}/unexpected-data/bin/repogrammar" ]]; then
  echo "unexpected release left a partial command install" >&2
  exit 1
fi

MISSING_WORKER_RELEASE="${TMP_ROOT}/missing-worker-release"
MISSING_WORKER_PACKAGE="${TMP_ROOT}/missing-worker-package"
mkdir -p "$MISSING_WORKER_RELEASE" "$MISSING_WORKER_PACKAGE"
cp "${PACKAGE_DIR}/repogrammar" "${MISSING_WORKER_PACKAGE}/repogrammar"
tar -czf "${MISSING_WORKER_RELEASE}/${ARTIFACT}" -C "$MISSING_WORKER_PACKAGE" repogrammar
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$MISSING_WORKER_RELEASE" && sha256sum "$ARTIFACT" > "${ARTIFACT}.sha256")
else
  (cd "$MISSING_WORKER_RELEASE" && shasum -a 256 "$ARTIFACT" > "${ARTIFACT}.sha256")
fi
MISSING_WORKER_ERR="${TMP_ROOT}/missing-worker.err"
set +e
REPOGRAMMAR_RELEASE_DIR="$MISSING_WORKER_RELEASE" \
REPOGRAMMAR_COMMAND_DIR="${TMP_ROOT}/missing-worker-bin" \
REPOGRAMMAR_INSTALL_DIR="${TMP_ROOT}/missing-worker-data" \
"$INSTALLER" --install-cli-only --yes >"${TMP_ROOT}/missing-worker.out" 2>"$MISSING_WORKER_ERR"
MISSING_WORKER_STATUS=$?
set -e
if [[ "$MISSING_WORKER_STATUS" -eq 0 ]]; then
  echo "missing worker release unexpectedly succeeded" >&2
  exit 1
fi
grep -q "bundled Python worker" "$MISSING_WORKER_ERR"
if [[ -e "${TMP_ROOT}/missing-worker-bin/repogrammar" || -e "${TMP_ROOT}/missing-worker-data/bin/repogrammar" ]]; then
  echo "missing worker release left a partial command install" >&2
  exit 1
fi

RELEASE_WORKFLOW="${SCRIPT_DIR}/../../.github/workflows/release.yml"
grep -q "repogrammar-x86_64-unknown-linux-gnu.tar.gz" "$RELEASE_WORKFLOW"
grep -q "repogrammar-aarch64-unknown-linux-gnu.tar.gz" "$RELEASE_WORKFLOW"
grep -q "repogrammar-x86_64-apple-darwin.tar.gz" "$RELEASE_WORKFLOW"
grep -q "repogrammar-aarch64-apple-darwin.tar.gz" "$RELEASE_WORKFLOW"
grep -q "repogrammar-x86_64-pc-windows-msvc.zip" "$RELEASE_WORKFLOW"
grep -q "src/workers/python/worker.py" "$RELEASE_WORKFLOW"
grep -q "install.sh" "$RELEASE_WORKFLOW"
grep -q "install.ps1" "$RELEASE_WORKFLOW"
grep -q ".sha256" "$RELEASE_WORKFLOW"

WINDOWS_INSTALLER="${SCRIPT_DIR}/install.ps1"
grep -q "repogrammar-x86_64-pc-windows-msvc.zip" "$WINDOWS_INSTALLER"
grep -q "Get-FileHash -Algorithm SHA256" "$WINDOWS_INSTALLER"
grep -q "Assert-SafeArchiveEntries" "$WINDOWS_INSTALLER"
grep -q "release artifact was not found" "$WINDOWS_INSTALLER"
grep -q "v0.2.0-preview.0" "$WINDOWS_INSTALLER"
grep -q "FromSource" "$WINDOWS_INSTALLER"
grep -q "REPOGRAMMAR_SOURCE_BINARY" "$WINDOWS_INSTALLER"
grep -q "cargo build --release" "$WINDOWS_INSTALLER"
