#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
INSTALLER="${SCRIPT_DIR}/repogrammar-install.sh"
TMP_ROOT="$(mktemp -d)"
RELEASE_BINARY_TO_RESTORE=""
RELEASE_BINARY_BACKUP=""
RELEASE_BINARY_EXISTED=0
ORIGINAL_PATH="${PATH:-}"
SYSTEM_PATH="$(command -p getconf PATH 2>/dev/null || printf '/usr/bin:/bin')"
CARGO_BIN="$(command -v cargo || true)"
NODE_BIN="$(command -v node || true)"
PATH="$SYSTEM_PATH"

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

# Extract one top-level workflow job without depending on a third-party YAML
# parser. GitHub Actions job identifiers are exactly two spaces below `jobs:`;
# stop at the next peer so assertions cannot be accidentally satisfied by an
# unrelated job elsewhere in the workflow.
workflow_job() {
  local workflow="$1"
  local requested_job="$2"
  awk -v requested_job="$requested_job" '
    /^jobs:[[:space:]]*$/ {
      in_jobs = 1
      next
    }
    in_jobs && /^  [A-Za-z0-9_-]+:[[:space:]]*$/ {
      job = $0
      sub(/^  /, "", job)
      sub(/:[[:space:]]*$/, "", job)
      if (in_requested && job != requested_job) {
        exit
      }
      in_requested = (job == requested_job)
    }
    in_requested { print }
  ' "$workflow"
}

# Extract a named step from an already isolated job. A step starts at six
# spaces plus `-`; unnamed `uses:` steps are still peer boundaries and prevent
# command assertions from leaking into a later step.
workflow_named_step() {
  local job_body="$1"
  local requested_step="$2"
  awk -v requested_step="$requested_step" '
    /^      - / {
      if (in_requested) {
        exit
      }
      step = $0
      sub(/^      - name:[[:space:]]*/, "", step)
      in_requested = (step == requested_step)
    }
    in_requested { print }
  ' <<<"$job_body"
}

require_workflow_match() {
  local body="$1"
  local pattern="$2"
  local failure="$3"
  if ! grep -Eq -- "$pattern" <<<"$body"; then
    echo "$failure" >&2
    exit 1
  fi
}

require_workflow_absence() {
  local body="$1"
  local pattern="$2"
  local failure="$3"
  if grep -Eq -- "$pattern" <<<"$body"; then
    echo "$failure" >&2
    exit 1
  fi
}

require_workflow_count_at_least() {
  local body="$1"
  local pattern="$2"
  local minimum="$3"
  local failure="$4"
  local count
  count="$(grep -Ec -- "$pattern" <<<"$body" || true)"
  if [[ "$count" -lt "$minimum" ]]; then
    echo "$failure" >&2
    exit 1
  fi
}

require_workflow_count_exactly() {
  local body="$1"
  local pattern="$2"
  local expected="$3"
  local failure="$4"
  local count
  count="$(grep -Ec -- "$pattern" <<<"$body" || true)"
  if [[ "$count" -ne "$expected" ]]; then
    echo "$failure (expected $expected, found $count)" >&2
    exit 1
  fi
}

TARGET="$("$INSTALLER" --print-target)"

# Linux release targets are glibc-specific. Prove the installer classifies the
# runtime before any network request or managed-path write, using only fake
# offline platform commands.
FAKE_LINUX_BIN="${TMP_ROOT}/fake-linux-bin"
mkdir -p "$FAKE_LINUX_BIN"
cat > "${FAKE_LINUX_BIN}/uname" <<'FAKE_UNAME'
#!/usr/bin/env bash
case "${1:-}" in
  -s) printf 'Linux\n' ;;
  -m) printf 'x86_64\n' ;;
  *) exit 2 ;;
esac
FAKE_UNAME
cat > "${FAKE_LINUX_BIN}/getconf" <<'FAKE_GETCONF'
#!/usr/bin/env bash
if [[ "${1:-}" == "GNU_LIBC_VERSION" ]]; then
  case "${REPOGRAMMAR_TEST_LIBC:-}" in
    glibc) printf 'glibc 2.39\n'; exit 0 ;;
    glibc-low) printf 'glibc 2.34\n'; exit 0 ;;
  esac
fi
exit 1
FAKE_GETCONF
cat > "${FAKE_LINUX_BIN}/ldd" <<'FAKE_LDD'
#!/usr/bin/env bash
case "${REPOGRAMMAR_TEST_LIBC:-}" in
  musl) printf 'musl libc (x86_64)\n' ;;
  unknown) printf 'unclassified libc runtime\n' ;;
  *) printf 'ldd (GNU libc) 2.39\n' ;;
esac
FAKE_LDD
cat > "${FAKE_LINUX_BIN}/curl" <<'FAKE_CURL'
#!/usr/bin/env bash
printf 'called\n' > "${REPOGRAMMAR_TEST_CURL_MARKER:?}"
exit 1
FAKE_CURL
chmod +x "${FAKE_LINUX_BIN}/uname" "${FAKE_LINUX_BIN}/getconf" "${FAKE_LINUX_BIN}/ldd" "${FAKE_LINUX_BIN}/curl"

GLIBC_TARGET="$(PATH="${FAKE_LINUX_BIN}:${SYSTEM_PATH}" REPOGRAMMAR_TEST_LIBC=glibc "$INSTALLER" --print-target)"
if [[ "$GLIBC_TARGET" != "x86_64-unknown-linux-gnu" ]]; then
  echo "glibc Linux target detection returned ${GLIBC_TARGET}" >&2
  exit 1
fi

for LIBC_CASE in musl unknown glibc-low; do
  LIBC_ROOT="${TMP_ROOT}/linux-${LIBC_CASE}"
  LIBC_CURL_MARKER="${LIBC_ROOT}/curl-called"
  mkdir -p "$LIBC_ROOT"
  set +e
  PATH="${FAKE_LINUX_BIN}:${SYSTEM_PATH}" \
  REPOGRAMMAR_TEST_LIBC="$LIBC_CASE" \
  REPOGRAMMAR_TEST_CURL_MARKER="$LIBC_CURL_MARKER" \
  REPOGRAMMAR_COMMAND_DIR="${LIBC_ROOT}/bin" \
  REPOGRAMMAR_INSTALL_DIR="${LIBC_ROOT}/data" \
  "$INSTALLER" --install-cli-only --yes >"${LIBC_ROOT}/out" 2>"${LIBC_ROOT}/err"
  LIBC_STATUS=$?
  set -e
  if [[ "$LIBC_STATUS" -eq 0 ]]; then
    echo "${LIBC_CASE} Linux release install unexpectedly succeeded" >&2
    exit 1
  fi
  grep -q "public preview requires glibc\|unable to confirm glibc\|require glibc 2.35+" "${LIBC_ROOT}/err"
  if [[ -e "$LIBC_CURL_MARKER" || -e "${LIBC_ROOT}/bin/repogrammar" || -e "${LIBC_ROOT}/data/bin/repogrammar" ]]; then
    echo "${LIBC_CASE} rejection must occur before download or managed-path writes" >&2
    exit 1
  fi
done

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

REPOGRAMMAR_RELEASE_DIR="$RELEASE_DIR" \
REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$INSTALL_DIR" \
"$INSTALLER" --install-cli-only --yes >/dev/null

"${COMMAND_DIR}/repogrammar" version | grep -q "repogrammar 0.1.0-test"
test -x "${TMP_ROOT}/share/repogrammar/bin/repogrammar"

# Unmanaged command path with --yes alone must be refused with guidance; the
# foreign file, its contents, and the absence of any managed install must all be
# preserved, and no backup may be created.
NO_REPLACE_COMMAND_DIR="${TMP_ROOT}/no-replace-bin"
NO_REPLACE_INSTALL_DIR="${TMP_ROOT}/no-replace-data"
mkdir -p "$NO_REPLACE_COMMAND_DIR"
printf 'foreign-unmanaged\n' > "${NO_REPLACE_COMMAND_DIR}/repogrammar"
chmod +x "${NO_REPLACE_COMMAND_DIR}/repogrammar"
NO_REPLACE_ERR="${TMP_ROOT}/no-replace.err"
set +e
REPOGRAMMAR_RELEASE_DIR="$RELEASE_DIR" \
REPOGRAMMAR_COMMAND_DIR="$NO_REPLACE_COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$NO_REPLACE_INSTALL_DIR" \
"$INSTALLER" --install-cli-only --yes >"${TMP_ROOT}/no-replace.out" 2>"$NO_REPLACE_ERR"
NO_REPLACE_STATUS=$?
set -e
if [[ "$NO_REPLACE_STATUS" -eq 0 ]]; then
  echo "unmanaged command replacement without the opt-in flag unexpectedly succeeded" >&2
  exit 1
fi
grep -q "refusing to replace unmanaged repogrammar command" "$NO_REPLACE_ERR"
grep -q -- "--replace-unmanaged-command" "$NO_REPLACE_ERR"
grep -q "foreign-unmanaged" "${NO_REPLACE_COMMAND_DIR}/repogrammar"
shopt -s nullglob
NO_REPLACE_BACKUPS=("${NO_REPLACE_COMMAND_DIR}"/repogrammar.unmanaged-backup*)
shopt -u nullglob
if [[ "${#NO_REPLACE_BACKUPS[@]}" -ne 0 ]]; then
  echo "refused unmanaged replacement must not create a backup" >&2
  exit 1
fi
if [[ -e "${NO_REPLACE_INSTALL_DIR}/bin/repogrammar" ]]; then
  echo "refused unmanaged replacement must not install a managed binary" >&2
  exit 1
fi

# A directory at the command path must still fail even with the opt-in flag,
# because a directory cannot be safely backed up and replaced.
DIR_COMMAND_DIR="${TMP_ROOT}/dir-command-bin"
DIR_COMMAND_INSTALL_DIR="${TMP_ROOT}/dir-command-data"
mkdir -p "${DIR_COMMAND_DIR}/repogrammar"
DIR_COMMAND_ERR="${TMP_ROOT}/dir-command.err"
set +e
REPOGRAMMAR_RELEASE_DIR="$RELEASE_DIR" \
REPOGRAMMAR_COMMAND_DIR="$DIR_COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$DIR_COMMAND_INSTALL_DIR" \
"$INSTALLER" --install-cli-only --yes --replace-unmanaged-command >"${TMP_ROOT}/dir-command.out" 2>"$DIR_COMMAND_ERR"
DIR_COMMAND_STATUS=$?
set -e
if [[ "$DIR_COMMAND_STATUS" -eq 0 ]]; then
  echo "directory command path unexpectedly succeeded" >&2
  exit 1
fi
grep -q "is a directory and cannot be replaced automatically" "$DIR_COMMAND_ERR"
if [[ ! -d "${DIR_COMMAND_DIR}/repogrammar" ]]; then
  echo "directory command path must be left in place" >&2
  exit 1
fi

# With the explicit --replace-unmanaged-command opt-in, an unmanaged command is
# backed up and replaced by the managed command.
UNMANAGED_RELEASE_COMMAND_DIR="${TMP_ROOT}/unmanaged-release-bin"
UNMANAGED_RELEASE_INSTALL_DIR="${TMP_ROOT}/unmanaged-release-data"
mkdir -p "$UNMANAGED_RELEASE_COMMAND_DIR"
printf 'foreign-release\n' > "${UNMANAGED_RELEASE_COMMAND_DIR}/repogrammar"
chmod +x "${UNMANAGED_RELEASE_COMMAND_DIR}/repogrammar"
REPOGRAMMAR_RELEASE_DIR="$RELEASE_DIR" \
REPOGRAMMAR_COMMAND_DIR="$UNMANAGED_RELEASE_COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$UNMANAGED_RELEASE_INSTALL_DIR" \
"$INSTALLER" --install-cli-only --yes --replace-unmanaged-command >"${TMP_ROOT}/unmanaged-release.out"
grep -q "Backed up existing unmanaged repogrammar command" "${TMP_ROOT}/unmanaged-release.out"
"${UNMANAGED_RELEASE_COMMAND_DIR}/repogrammar" version | grep -q "repogrammar 0.1.0-test"
shopt -s nullglob
UNMANAGED_RELEASE_BACKUPS=("${UNMANAGED_RELEASE_COMMAND_DIR}"/repogrammar.unmanaged-backup*)
shopt -u nullglob
if [[ "${#UNMANAGED_RELEASE_BACKUPS[@]}" -ne 1 ]]; then
  echo "expected one unmanaged release command backup" >&2
  exit 1
fi
grep -q "foreign-release" "${UNMANAGED_RELEASE_BACKUPS[0]}"

AUTO_PRUNE_COMMAND_DIR="${TMP_ROOT}/auto-prune-bin"
AUTO_PRUNE_INSTALL_DIR="${TMP_ROOT}/auto-prune-data"
AUTO_PRUNE_STALE_DIR="${TMP_ROOT}/auto-prune-stale"
mkdir -p "$AUTO_PRUNE_STALE_DIR"
printf 'stale\n' > "${AUTO_PRUNE_STALE_DIR}/repogrammar"
chmod +x "${AUTO_PRUNE_STALE_DIR}/repogrammar"
PATH="${AUTO_PRUNE_COMMAND_DIR}:${AUTO_PRUNE_STALE_DIR}:${SYSTEM_PATH}" \
REPOGRAMMAR_RELEASE_DIR="$RELEASE_DIR" \
REPOGRAMMAR_COMMAND_DIR="$AUTO_PRUNE_COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$AUTO_PRUNE_INSTALL_DIR" \
"$INSTALLER" --install-cli-only --yes >"${TMP_ROOT}/auto-prune.out"
grep -q "Stale PATH copies" "${TMP_ROOT}/auto-prune.out"
grep -q "Removed" "${TMP_ROOT}/auto-prune.out"
if [[ -e "${AUTO_PRUNE_STALE_DIR}/repogrammar" ]]; then
  echo "install/update path did not automatically prune stale PATH copy" >&2
  exit 1
fi
test -x "${AUTO_PRUNE_COMMAND_DIR}/repogrammar"

if [[ "$(id -u)" -ne 0 ]]; then
  FAIL_PRUNE_COMMAND_DIR="${TMP_ROOT}/fail-prune-bin"
  FAIL_PRUNE_INSTALL_DIR="${TMP_ROOT}/fail-prune-data"
  FAIL_PRUNE_STALE_DIR="${TMP_ROOT}/fail-prune-stale"
  mkdir -p "$FAIL_PRUNE_STALE_DIR"
  printf 'stale\n' > "${FAIL_PRUNE_STALE_DIR}/repogrammar"
  chmod +x "${FAIL_PRUNE_STALE_DIR}/repogrammar"
  chmod 555 "$FAIL_PRUNE_STALE_DIR"
  set +e
  PATH="${FAIL_PRUNE_COMMAND_DIR}:${FAIL_PRUNE_STALE_DIR}:${SYSTEM_PATH}" \
  REPOGRAMMAR_RELEASE_DIR="$RELEASE_DIR" \
  REPOGRAMMAR_COMMAND_DIR="$FAIL_PRUNE_COMMAND_DIR" \
  REPOGRAMMAR_INSTALL_DIR="$FAIL_PRUNE_INSTALL_DIR" \
  "$INSTALLER" --install-cli-only --yes >"${TMP_ROOT}/fail-prune.out" 2>"${TMP_ROOT}/fail-prune.err"
  FAIL_PRUNE_STATUS=$?
  set -e
  chmod 755 "$FAIL_PRUNE_STALE_DIR"
  if [[ "$FAIL_PRUNE_STATUS" -eq 0 ]]; then
    echo "failed stale PATH prune unexpectedly succeeded" >&2
    exit 1
  fi
  grep -q "Failed to remove" "${TMP_ROOT}/fail-prune.err"
  grep -q "failed to remove 1 stale PATH copy/copies" "${TMP_ROOT}/fail-prune.err"
  if [[ ! -e "${FAIL_PRUNE_STALE_DIR}/repogrammar" ]]; then
    echo "failed stale PATH prune should leave the stale copy in place" >&2
    exit 1
  fi
fi

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

REPOGRAMMAR_SOURCE_BINARY="${PACKAGE_DIR}/repogrammar" \
REPOGRAMMAR_COMMAND_DIR="$SOURCE_COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$SOURCE_INSTALL_DIR" \
"$INSTALLER" --install-cli-only --from-source --yes >/dev/null

"${SOURCE_COMMAND_DIR}/repogrammar" version | grep -q "repogrammar 0.1.0-test"
test -x "${SOURCE_INSTALL_DIR}/bin/repogrammar"

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

if [[ -z "$CARGO_BIN" ]]; then
  echo "cargo is required for installer product smoke test" >&2
  exit 1
fi
PATH="$ORIGINAL_PATH" "$CARGO_BIN" build --quiet --bin repogrammar
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
(cd "$PRODUCT_REPO" && PATH="$ORIGINAL_PATH" "${PRODUCT_COMMAND_DIR}/repogrammar" init >/dev/null)
(cd "$PRODUCT_REPO" && PATH="$ORIGINAL_PATH" "${PRODUCT_COMMAND_DIR}/repogrammar" index --progress never >/dev/null)
(cd "$PRODUCT_REPO" && PATH="$ORIGINAL_PATH" "${PRODUCT_COMMAND_DIR}/repogrammar" families --json >/dev/null)

FOREIGN_COMMAND_DIR="${TMP_ROOT}/foreign-bin"
FOREIGN_INSTALL_DIR="${TMP_ROOT}/foreign-data"
mkdir -p "$FOREIGN_COMMAND_DIR"
printf 'foreign\n' > "${FOREIGN_COMMAND_DIR}/repogrammar"
chmod +x "${FOREIGN_COMMAND_DIR}/repogrammar"
REPOGRAMMAR_SOURCE_BINARY="${PACKAGE_DIR}/repogrammar" \
REPOGRAMMAR_COMMAND_DIR="$FOREIGN_COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$FOREIGN_INSTALL_DIR" \
"$INSTALLER" --install-cli-only --from-source --yes --replace-unmanaged-command >"${TMP_ROOT}/foreign.out"
grep -q "Backed up existing unmanaged repogrammar command" "${TMP_ROOT}/foreign.out"
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

# A member whose NAME is whitelisted but whose TYPE is a symlink must be
# rejected before extraction, so a hostile archive cannot redirect extraction.
SYMLINK_RELEASE="${TMP_ROOT}/symlink-release"
SYMLINK_PACKAGE="${TMP_ROOT}/symlink-package"
mkdir -p "$SYMLINK_RELEASE" "$SYMLINK_PACKAGE/workers/python"
ln -s outside-target "${SYMLINK_PACKAGE}/repogrammar"
cp "${PACKAGE_DIR}/workers/python/worker.py" "${SYMLINK_PACKAGE}/workers/python/worker.py"
tar -czf "${SYMLINK_RELEASE}/${ARTIFACT}" -C "$SYMLINK_PACKAGE" repogrammar workers
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$SYMLINK_RELEASE" && sha256sum "$ARTIFACT" > "${ARTIFACT}.sha256")
else
  (cd "$SYMLINK_RELEASE" && shasum -a 256 "$ARTIFACT" > "${ARTIFACT}.sha256")
fi
SYMLINK_ERR="${TMP_ROOT}/symlink.err"
set +e
REPOGRAMMAR_RELEASE_DIR="$SYMLINK_RELEASE" \
REPOGRAMMAR_COMMAND_DIR="${TMP_ROOT}/symlink-bin" \
REPOGRAMMAR_INSTALL_DIR="${TMP_ROOT}/symlink-data" \
"$INSTALLER" --install-cli-only --yes >"${TMP_ROOT}/symlink.out" 2>"$SYMLINK_ERR"
SYMLINK_STATUS=$?
set -e
if [[ "$SYMLINK_STATUS" -eq 0 ]]; then
  echo "symlink release member unexpectedly succeeded" >&2
  exit 1
fi
grep -q "non-regular-file member" "$SYMLINK_ERR"
if [[ -e "${TMP_ROOT}/symlink-bin/repogrammar" || -e "${TMP_ROOT}/symlink-data/bin/repogrammar" ]]; then
  echo "symlink release left a partial command install" >&2
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
if [[ -z "$NODE_BIN" ]]; then
  echo "node is required for release manifest validation" >&2
  exit 1
fi
PACKAGE_VERSION="$("$NODE_BIN" -p "require('${SCRIPT_DIR}/../../package.json').version")"
CARGO_VERSION="$(awk -F' *= *' '
  /^\[/ { section = $0 }
  section == "[package]" && $1 == "version" { gsub(/"/, "", $2); print $2; exit }
' "${SCRIPT_DIR}/../../Cargo.toml")"
if [[ "$PACKAGE_VERSION" != "0.2.0-preview.0" || "$CARGO_VERSION" != "$PACKAGE_VERSION" ]]; then
  echo "public-preview manifests must agree on 0.2.0-preview.0" >&2
  exit 1
fi
PACKAGE_MANIFEST="${SCRIPT_DIR}/../../package.json"
README_FILE="${SCRIPT_DIR}/../../README.md"
grep -q '"repository"' "$PACKAGE_MANIFEST"
grep -q '"homepage"' "$PACKAGE_MANIFEST"
grep -q '"bugs"' "$PACKAGE_MANIFEST"
grep -q 'npm view @sioyooo/repogrammar@0.2.0-preview.0 version' "$README_FILE"
grep -q 'releases/download/v0.2.0-preview.0/install.sh.sha256' "$README_FILE"
grep -q 'npx @sioyooo/repogrammar@0.2.0-preview.0 setup' "$README_FILE"
if grep -Eq '\]\((docs/|CONTRIBUTING\.md|SECURITY\.md|CODE_OF_CONDUCT\.md|LICENSE\))' "$README_FILE"; then
  echo "packed README must not contain relative links to unpackaged repository files" >&2
  exit 1
fi
WORKFLOW_DISPATCH_TRIGGER="$(awk '
  /^  workflow_dispatch:[[:space:]]*$/ { in_dispatch = 1 }
  in_dispatch && /^[A-Za-z0-9_-]+:[[:space:]]*$/ { exit }
  in_dispatch { print }
' "$RELEASE_WORKFLOW")"
VERIFY_JOB="$(workflow_job "$RELEASE_WORKFLOW" verify)"
BUILD_JOB="$(workflow_job "$RELEASE_WORKFLOW" build)"
PUBLISH_RELEASE_JOB="$(workflow_job "$RELEASE_WORKFLOW" publish_release)"
PUBLISH_NPM_JOB="$(workflow_job "$RELEASE_WORKFLOW" publish_npm)"
TAG_VERSION_STEP="$(workflow_named_step "$VERIFY_JOB" "Verify tag and version agreement")"

if [[ -z "$VERIFY_JOB" || -z "$BUILD_JOB" || -z "$PUBLISH_RELEASE_JOB" || -z "$PUBLISH_NPM_JOB" ]]; then
  echo "release workflow is missing the verify/build/publish_release/publish_npm staged jobs" >&2
  exit 1
fi

require_workflow_match "$WORKFLOW_DISPATCH_TRIGGER" 'mode:' \
  "workflow_dispatch must expose an explicit release mode"
require_workflow_match "$WORKFLOW_DISPATCH_TRIGGER" 'default:[[:space:]]+build-only' \
  "workflow_dispatch must default to build-only"
require_workflow_match "$WORKFLOW_DISPATCH_TRIGGER" '^([[:space:]]*)-[[:space:]]+build-only' \
  "workflow_dispatch must not offer an ambiguous publication mode"
require_workflow_match "$TAG_VERSION_STEP" 'EVENT_NAME:[[:space:]]+\$\{\{[[:space:]]*github\.event_name' \
  "tag/version validation must receive the triggering event type"
require_workflow_match "$TAG_VERSION_STEP" 'EVENT_NAME.*=[[:space:]]*"push".*REF_TYPE.*=[[:space:]]*"tag"' \
  "manual dispatch from a tag ref must remain build-only during version validation"

require_workflow_match "$BUILD_JOB" 'needs:[[:space:]]+verify' \
  "release builds must depend on the release verification gate"
require_workflow_match "$BUILD_JOB" 'repogrammar-x86_64-unknown-linux-gnu\.tar\.gz' \
  "release build matrix is missing Linux x86_64"
require_workflow_match "$BUILD_JOB" 'repogrammar-aarch64-unknown-linux-gnu\.tar\.gz' \
  "release build matrix is missing Linux arm64"
require_workflow_match "$BUILD_JOB" 'repogrammar-x86_64-apple-darwin\.tar\.gz' \
  "release build matrix is missing macOS x86_64"
require_workflow_match "$BUILD_JOB" 'repogrammar-aarch64-apple-darwin\.tar\.gz' \
  "release build matrix is missing macOS arm64"
require_workflow_count_exactly "$BUILD_JOB" '^[[:space:]]+target:[[:space:]]+' 4 \
  "public-preview release matrix must contain exactly four supported targets"
require_workflow_absence "$BUILD_JOB" 'windows|Windows|pc-windows|\.zip|pwsh|PowerShell|Compress-Archive' \
  "public-preview release builds must not claim or package Windows support"
require_workflow_match "$BUILD_JOB" 'src/workers/python/worker\.py' \
  "release artifacts must package the Python worker"
require_workflow_match "$BUILD_JOB" '\.sha256' \
  "every release archive must have a checksum artifact"

# Tag publication credentials are a prerequisite, not an optional npm step.
# The check must happen in `verify`, before either publication job can write
# external state. workflow_dispatch remains build-only because only tag refs
# can reach the two staged publication jobs.
CREDENTIAL_PREFLIGHT="$(workflow_named_step "$VERIFY_JOB" "Require npm credentials before tag publication")"
TAG_AUTHORITY="$(workflow_named_step "$VERIFY_JOB" "Verify publication tag authority")"
if [[ -z "$CREDENTIAL_PREFLIGHT" ]]; then
  echo "release verification must include a named tag publication credential preflight" >&2
  exit 1
fi
require_workflow_match "$CREDENTIAL_PREFLIGHT" 'NODE_AUTH_TOKEN:[[:space:]]+\$\{\{[[:space:]]*secrets\.NPM_TOKEN[[:space:]]*\}\}' \
  "the pre-publication verify job must receive NPM_TOKEN"
require_workflow_match "$CREDENTIAL_PREFLIGHT" 'github\.ref_type.*tag' \
  "the npm credential preflight must be scoped to tag publication"
require_workflow_match "$CREDENTIAL_PREFLIGHT" 'github\.event_name.*push' \
  "manual dispatch must never run the npm credential preflight, even from a tag ref"
require_workflow_match "$CREDENTIAL_PREFLIGHT" 'NODE_AUTH_TOKEN' \
  "the tag publication credential preflight must inspect NODE_AUTH_TOKEN"
require_workflow_match "$CREDENTIAL_PREFLIGHT" 'if[[:space:]].*-z.*NODE_AUTH_TOKEN' \
  "the tag publication credential preflight must classify an absent token"
require_workflow_match "$CREDENTIAL_PREFLIGHT" 'exit[[:space:]]+1' \
  "missing npm publication credentials must fail the tag release gate"
require_workflow_match "$CREDENTIAL_PREFLIGHT" 'npm[[:space:]]+whoami' \
  "tag publication preflight must verify that npm accepts the configured token"
require_workflow_absence "$CREDENTIAL_PREFLIGHT" 'exit[[:space:]]+0|NPM_TOKEN.*skipp|skipp.*NPM_TOKEN' \
  "the tag release gate must not describe missing npm credentials as skippable"

if [[ -z "$TAG_AUTHORITY" ]]; then
  echo "release verification must prove the pushed tag commit belongs to origin/main" >&2
  exit 1
fi
require_workflow_match "$TAG_AUTHORITY" 'github\.event_name.*push' \
  "tag authority verification must run only for pushed publication tags"
require_workflow_match "$TAG_AUTHORITY" 'github\.ref_type.*tag' \
  "tag authority verification must require a tag ref"
require_workflow_match "$TAG_AUTHORITY" 'git[[:space:]]+fetch.*origin[[:space:]]+main:refs/remotes/origin/main' \
  "tag authority verification must fetch origin/main"
require_workflow_match "$TAG_AUTHORITY" 'git[[:space:]]+merge-base[[:space:]]+--is-ancestor[[:space:]]+HEAD[[:space:]]+origin/main' \
  "tag authority verification must require the tag commit to be contained in origin/main"
require_workflow_match "$VERIFY_JOB" 'fetch-depth:[[:space:]]+0' \
  "release verification must check out complete history for the tag ancestry gate"

require_workflow_match "$PUBLISH_RELEASE_JOB" 'needs:[[:space:]]+build' \
  "GitHub prerelease assets must be staged after verified artifact builds"
require_workflow_match "$PUBLISH_RELEASE_JOB" "if:[[:space:]]+github\.event_name[[:space:]]*==[[:space:]]*'push'.*github\.ref_type[[:space:]]*==[[:space:]]*'tag'" \
  "GitHub prerelease publication must require a pushed tag, never manual dispatch"
require_workflow_match "$PUBLISH_RELEASE_JOB" 'softprops/action-gh-release' \
  "publish_release must create the GitHub prerelease"
require_workflow_match "$PUBLISH_RELEASE_JOB" 'install\.sh' \
  "GitHub prerelease assets must include install.sh"
require_workflow_absence "$PUBLISH_RELEASE_JOB" 'install\.ps1|windows|Windows' \
  "GitHub prerelease assets must not publish the unsupported Windows installer"
require_workflow_match "$PUBLISH_RELEASE_JOB" '\.sha256' \
  "GitHub prerelease assets must include installer checksums"

require_workflow_match "$PUBLISH_NPM_JOB" 'needs:[[:space:]]+publish_release' \
  "npm publication must explicitly follow GitHub prerelease asset publication"
require_workflow_match "$PUBLISH_NPM_JOB" "if:[[:space:]]+github\.event_name[[:space:]]*==[[:space:]]*'push'.*github\.ref_type[[:space:]]*==[[:space:]]*'tag'" \
  "npm publication must require a pushed tag so workflow_dispatch is always build-only"
require_workflow_match "$PUBLISH_NPM_JOB" 'npm publish --access public --tag preview' \
  "publish_npm must publish the launcher instead of reporting a skipped success"
require_workflow_absence "$PUBLISH_NPM_JOB" 'exit[[:space:]]+0|skipping npm publish' \
  "publish_npm must not turn absent credentials into a green skipped publication"

# The release matrix must smoke the exact archive it uploads. Source-tree
# binaries do not prove that an archive is executable or contains the runtime
# worker. The live no-agent setup also exercises the product tools/list
# self-test; its JSON evidence must say that the product self-test passed.
PACKAGED_ARTIFACT_SMOKE="$(workflow_named_step "$BUILD_JOB" "Smoke packaged artifact")"
if [[ -z "$PACKAGED_ARTIFACT_SMOKE" ]]; then
  echo "release build must have a named packaged-artifact smoke step" >&2
  exit 1
fi
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" 'tar[[:space:]].*-x' \
  "packaged smoke must extract the exact archive it will upload"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" 'binary=.*unpacked/repogrammar' \
  "packaged smoke must bind the executable from the extracted archive"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" 'unpacked/workers/python/worker\.py' \
  "packaged smoke must verify the worker inside the extracted archive"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" 'sys\.version_info[[:space:]]*>=[[:space:]]*\(3,[[:space:]]*10\)' \
  "packaged smoke must enforce the Python 3.10+ runtime contract"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" '\$\{binary\}.*version' \
  "packaged smoke must run version from the extracted binary"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" 'expected_version=.*package\.json' \
  "packaged smoke must derive the expected version from the release manifest"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" 'repogrammar.*expected_version' \
  "packaged smoke must require exact binary/manifest version agreement"
require_workflow_count_at_least "$PACKAGED_ARTIFACT_SMOKE" '\$\{binary\}.*setup' 2 \
  "packaged smoke must run both dry-run and live setup from the extracted binary"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" '--dry-run' \
  "packaged smoke must run setup --dry-run --json"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" '--json' \
  "packaged smoke must validate machine-readable setup and query output"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" 'command -v git' \
  "packaged smoke must preserve git in its isolated tool PATH"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" 'command -v python3' \
  "packaged smoke must preserve the Python worker runtime in its isolated tool PATH"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" 'PATH=.*tool_path' \
  "packaged smoke must isolate setup from real agent configuration"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" 'HOME=.*home' \
  "packaged smoke must use a fresh isolated HOME"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" 'product_self_test_state' \
  "packaged smoke must inspect product MCP self-test evidence"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" 'product_self_test_state.*passed' \
  "packaged smoke must require a passed product MCP self-test"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" '\$\{binary\}.*find' \
  "packaged smoke must exercise find from the extracted binary"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" '\$\{binary\}.*check' \
  "packaged smoke must exercise check from the extracted binary"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" 'CONTEXT_ONLY' \
  "packaged check smoke must preserve advisory context semantics"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" 'advisory_status.*UNKNOWN' \
  "packaged check smoke must preserve typed UNKNOWN truthfulness"
require_workflow_match "$BUILD_JOB" 'os:[[:space:]]+ubuntu-22\.04' \
  "Linux x86_64 release builds must pin the declared glibc 2.35 floor runner"
require_workflow_match "$BUILD_JOB" 'os:[[:space:]]+ubuntu-24\.04-arm' \
  "Linux arm64 release builds must pin the declared glibc 2.39 floor runner"
LINUX_ABI_STEP="$(workflow_named_step "$BUILD_JOB" "Inspect Linux glibc ABI floor")"
if [[ -z "$LINUX_ABI_STEP" ]]; then
  echo "release builds must inspect Linux glibc symbol requirements" >&2
  exit 1
fi
require_workflow_match "$LINUX_ABI_STEP" 'objdump[[:space:]]+-T' \
  "Linux release ABI inspection must read dynamic symbol versions"
require_workflow_match "$LINUX_ABI_STEP" '2\.35' \
  "Linux ABI inspection must enforce the x86_64 glibc floor"
require_workflow_match "$LINUX_ABI_STEP" '2\.39' \
  "Linux ABI inspection must enforce the arm64 glibc floor"

CI_WORKFLOW="${SCRIPT_DIR}/../../.github/workflows/ci.yml"
MACOS_SMOKE_JOB="$(workflow_job "$CI_WORKFLOW" macos-product-smoke)"
WINDOWS_SOURCE_JOB="$(workflow_job "$CI_WORKFLOW" windows-source-installer-contract)"
if [[ -z "$MACOS_SMOKE_JOB" ]]; then
  echo "CI must include the macos-product-smoke job" >&2
  exit 1
fi
require_workflow_match "$MACOS_SMOKE_JOB" 'runs-on:[[:space:]]+macos-' \
  "macOS product smoke must run on a macOS runner"
require_workflow_match "$MACOS_SMOKE_JOB" 'cargo test --workspace --all-features' \
  "macOS coverage must exercise the Rust workspace rather than compilation only"
require_workflow_match "$MACOS_SMOKE_JOB" '\$\{binary\}.*version' \
  "macOS coverage must run the product version path"
require_workflow_match "$MACOS_SMOKE_JOB" '\$\{binary\}.*setup' \
  "macOS coverage must invoke isolated product setup"
require_workflow_match "$MACOS_SMOKE_JOB" '--dry-run' \
  "macOS coverage must run isolated setup JSON smoke"
require_workflow_match "$MACOS_SMOKE_JOB" '--json' \
  "macOS coverage must validate setup JSON"
require_workflow_match "$MACOS_SMOKE_JOB" 'command -v git' \
  "macOS coverage must preserve git in its isolated tool PATH"
require_workflow_match "$MACOS_SMOKE_JOB" 'command -v python3' \
  "macOS coverage must preserve the Python worker runtime in its isolated tool PATH"
require_workflow_match "$MACOS_SMOKE_JOB" 'tool_path=.*tools' \
  "macOS coverage must build a dedicated tool-only PATH"
require_workflow_match "$MACOS_SMOKE_JOB" 'PATH=.*tool_path' \
  "macOS coverage must isolate setup from real agent CLIs"
require_workflow_match "$MACOS_SMOKE_JOB" 'product_self_test_state' \
  "macOS coverage must validate product MCP self-test evidence"

if [[ -z "$WINDOWS_SOURCE_JOB" ]]; then
  echo "CI must include the Windows source-only installer contract job" >&2
  exit 1
fi
require_workflow_match "$WINDOWS_SOURCE_JOB" 'name:[[:space:]]+Windows source-only installer contract' \
  "Windows CI must remain explicitly source-only"
require_workflow_match "$WINDOWS_SOURCE_JOB" 'runs-on:[[:space:]]+windows-' \
  "Windows source-only installer tests must run on a native Windows runner"
require_workflow_match "$WINDOWS_SOURCE_JOB" 'dtolnay/rust-toolchain@stable' \
  "Windows source-only installer tests must provision Rust"
require_workflow_match "$WINDOWS_SOURCE_JOB" 'install\.ps1\.test\.ps1' \
  "Windows source-only installer tests must execute the native PowerShell contract"
require_workflow_absence "$WINDOWS_SOURCE_JOB" 'upload-artifact|release|repogrammar-.*windows|npm publish' \
  "Windows source-only CI must not imply a release artifact or publication claim"

WINDOWS_INSTALLER="${SCRIPT_DIR}/install.ps1"
# Windows remains a source-checkout contributor path, not a public-preview
# release artifact or npm platform claim.
grep -q "FromSource" "$WINDOWS_INSTALLER"
grep -q "REPOGRAMMAR_SOURCE_BINARY" "$WINDOWS_INSTALLER"
grep -q "cargo build --release" "$WINDOWS_INSTALLER"
grep -q "Windows is not supported by the public preview" "$WINDOWS_INSTALLER"
grep -q "installation requires explicit -FromSource" "$WINDOWS_INSTALLER"
if grep -Eq 'repogrammar-x86_64-pc-windows-msvc|Install-CliFromRelease|Get-WindowsArtifactName|Invoke-WebRequest' "$WINDOWS_INSTALLER"; then
  echo "Windows contributor installer still contains a release-download path" >&2
  exit 1
fi
WINDOWS_INSTALLER_TEST="${SCRIPT_DIR}/install.ps1.test.ps1"
grep -q "Windows default release install unexpectedly succeeded" "$WINDOWS_INSTALLER_TEST"
grep -q "refused Windows release install created command or install state" "$WINDOWS_INSTALLER_TEST"
