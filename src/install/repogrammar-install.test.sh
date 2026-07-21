#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
INSTALLER="${SCRIPT_DIR}/repogrammar-install.sh"
TMP_ROOT="$(mktemp -d)"
TMP_ROOT="$(cd -- "$TMP_ROOT" && pwd -P)"
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
  grep -q "requires glibc\|release binaries require glibc\|unable to confirm glibc\|require glibc 2.35+" "${LIBC_ROOT}/err"
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
  install|disconnect|uninstall)
    command_name="$1"
    if [ -n "${REPOGRAMMAR_FAKE_LOG:-}" ]; then
      printf '%s' "$1" >> "$REPOGRAMMAR_FAKE_LOG"
      shift
      for arg in "$@"; do
        printf ' %s' "$arg" >> "$REPOGRAMMAR_FAKE_LOG"
      done
      printf '\n' >> "$REPOGRAMMAR_FAKE_LOG"
    fi
    if [ "$command_name" = "uninstall" ]; then
      if [ "${REPOGRAMMAR_FAKE_UNINSTALL_FAIL:-0}" = "1" ]; then
        echo "status=partial report_path=/tmp/fake-partial-report.json" >&2
        exit 23
      fi
      echo "status=finalizer_pending report_path=/tmp/fake-uninstall-report.json"
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

CUSTOM_WORKER_ERR="${TMP_ROOT}/custom-worker.err"
set +e
REPOGRAMMAR_SOURCE_BINARY="${PACKAGE_DIR}/repogrammar" \
REPOGRAMMAR_COMMAND_DIR="${TMP_ROOT}/custom-worker-bin" \
REPOGRAMMAR_INSTALL_DIR="${TMP_ROOT}/custom-worker-data" \
REPOGRAMMAR_WORKER_ROOT="${TMP_ROOT}/custom-workers" \
"$INSTALLER" --install-cli-only --from-source --yes >"${TMP_ROOT}/custom-worker.out" 2>"$CUSTOM_WORKER_ERR"
CUSTOM_WORKER_STATUS=$?
set -e
if [[ "$CUSTOM_WORKER_STATUS" -eq 0 ]]; then
  echo "custom worker-root install unexpectedly succeeded" >&2
  exit 1
fi
grep -q "custom REPOGRAMMAR_WORKER_ROOT is not supported" "$CUSTOM_WORKER_ERR"
if [[ -e "${TMP_ROOT}/custom-worker-bin/repogrammar" || -e "${TMP_ROOT}/custom-worker-data/bin/repogrammar" || -e "${TMP_ROOT}/custom-workers" ]]; then
  echo "rejected custom worker-root install wrote managed files" >&2
  exit 1
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

FRESH_STATE_REPO="${TMP_ROOT}/fresh-state-boundary-repo"
mkdir -p "$FRESH_STATE_REPO"
(
  cd "$FRESH_STATE_REPO"
  REPOGRAMMAR_RELEASE_DIR="$RELEASE_DIR" \
  REPOGRAMMAR_COMMAND_DIR="${TMP_ROOT}/fresh-state-bin" \
  REPOGRAMMAR_INSTALL_DIR="${TMP_ROOT}/fresh-state-data" \
  "$INSTALLER" --install-cli-only --yes >/dev/null
)
if [[ -e "${FRESH_STATE_REPO}/.repogrammar" ]]; then
  echo "install-cli-only must not initialize repository state" >&2
  exit 1
fi

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

grep -q "disconnect --target all --scope global --yes" "$LOG_FILE"

UNINSTALL_COMMAND_OUT="${TMP_ROOT}/uninstall-command.out"
set +e
REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$INSTALL_DIR" \
"$INSTALLER" --uninstall-command --yes >"$UNINSTALL_COMMAND_OUT" 2>&1
UNINSTALL_COMMAND_STATUS=$?
set -e
if [[ "$UNINSTALL_COMMAND_STATUS" -eq 0 ]]; then
  echo "deprecated --uninstall-command unexpectedly succeeded" >&2
  exit 1
fi
grep -q -- "--uninstall-command is deprecated" "$UNINSTALL_COMMAND_OUT"
grep -q "repogrammar uninstall --yes" "$UNINSTALL_COMMAND_OUT"

if [[ ! -e "${COMMAND_DIR}/repogrammar" ]]; then
  echo "deprecated --uninstall-command removed the command" >&2
  exit 1
fi

UNINSTALL_REPO="${TMP_ROOT}/wrapper-uninstall-repo"
UNINSTALL_EXTRA_DIR="${TMP_ROOT}/wrapper-uninstall-extra"
mkdir -p "${UNINSTALL_REPO}/.repogrammar" "$UNINSTALL_EXTRA_DIR"
printf 'keep\n' > "${UNINSTALL_REPO}/.repogrammar/sentinel"
printf 'unmanaged\n' > "${UNINSTALL_EXTRA_DIR}/repogrammar"
UNINSTALL_OUT="${TMP_ROOT}/uninstall-all.out"
(
  cd "$UNINSTALL_REPO"
  PATH="${UNINSTALL_EXTRA_DIR}:$PATH" \
  REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
  REPOGRAMMAR_INSTALL_DIR="$INSTALL_DIR" \
  REPOGRAMMAR_FAKE_LOG="$LOG_FILE" \
  "$INSTALLER" --uninstall-all --yes >"$UNINSTALL_OUT"
)
grep -q '^uninstall --yes$' "$LOG_FILE"
grep -q 'status=finalizer_pending' "$UNINSTALL_OUT"
grep -q 'report_path=' "$UNINSTALL_OUT"
grep -q 'keep' "${UNINSTALL_REPO}/.repogrammar/sentinel"
grep -q 'unmanaged' "${UNINSTALL_EXTRA_DIR}/repogrammar"

UNINSTALL_TARGET_OUT="${TMP_ROOT}/uninstall-target.out"
set +e
REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$INSTALL_DIR" \
"$INSTALLER" --uninstall-all --target codex --yes >"$UNINSTALL_TARGET_OUT" 2>&1
UNINSTALL_TARGET_STATUS=$?
set -e
if [[ "$UNINSTALL_TARGET_STATUS" -eq 0 ]]; then
  echo "product wrapper silently accepted --target" >&2
  exit 1
fi
grep -q -- "use --uninstall-agents --target" "$UNINSTALL_TARGET_OUT"

DRY_RUN_OUT="${TMP_ROOT}/uninstall-dry-run.out"
REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$INSTALL_DIR" \
REPOGRAMMAR_FAKE_LOG="$LOG_FILE" \
"$INSTALLER" --uninstall-all --dry-run --yes >"$DRY_RUN_OUT"
grep -q '^uninstall --dry-run --yes$' "$LOG_FILE"
test -x "${COMMAND_DIR}/repogrammar"

FAIL_UNINSTALL_OUT="${TMP_ROOT}/uninstall-failure.out"
set +e
REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
REPOGRAMMAR_INSTALL_DIR="$INSTALL_DIR" \
REPOGRAMMAR_FAKE_LOG="$LOG_FILE" \
REPOGRAMMAR_FAKE_UNINSTALL_FAIL=1 \
"$INSTALLER" --uninstall-all --yes >"$FAIL_UNINSTALL_OUT" 2>&1
FAIL_UNINSTALL_STATUS=$?
set -e
if [[ "$FAIL_UNINSTALL_STATUS" -ne 23 ]]; then
  echo "wrapper swallowed product uninstall failure: ${FAIL_UNINSTALL_STATUS}" >&2
  exit 1
fi
grep -q 'status=partial' "$FAIL_UNINSTALL_OUT"
grep -q 'report_path=' "$FAIL_UNINSTALL_OUT"
test -x "${COMMAND_DIR}/repogrammar"

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
test -f "${PRODUCT_INSTALL_DIR}/receipts/product-install.json"
test -f "${PRODUCT_INSTALL_DIR}/workers/python/worker.py"
test -f "${PRODUCT_COMMAND_DIR}/repogrammar-workers/python/worker.py"
(cd "$PRODUCT_REPO" && PATH="$ORIGINAL_PATH" "${PRODUCT_COMMAND_DIR}/repogrammar" init --state-only >/dev/null)
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
grep -q -- "--version v0.4.2" "$NO_RELEASE_ERR"
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
if [[ "$PACKAGE_VERSION" != "0.4.2" || "$CARGO_VERSION" != "$PACKAGE_VERSION" ]]; then
  echo "stable source manifests must agree on 0.4.2" >&2
  exit 1
fi
PACKAGE_MANIFEST="${SCRIPT_DIR}/../../package.json"
README_FILE="${SCRIPT_DIR}/../../README.md"
grep -q '"repository"' "$PACKAGE_MANIFEST"
grep -q '"homepage"' "$PACKAGE_MANIFEST"
grep -q '"bugs"' "$PACKAGE_MANIFEST"
README_QUICK_START="$(awk '
  /^## Quick start$/ { in_quick_start = 1; next }
  in_quick_start && /^## / { exit }
  in_quick_start { print }
' "$README_FILE")"
grep -q 'releases/download/v0\.4\.2/install.sh' <<<"$README_QUICK_START"
grep -q '^bash install.sh --version v0.4.2 --install-cli-only --yes$' <<<"$README_QUICK_START"
grep -q '^export PATH="\$HOME/.local/bin:\$PATH"$' <<<"$README_QUICK_START"
grep -q '^repogrammar install --target auto --scope global --yes --no-telemetry$' <<<"$README_QUICK_START"
grep -q '^repogrammar init --project "\$PWD" --yes$' <<<"$README_QUICK_START"
grep -q 'There is no global repository scanner' <<<"$README_QUICK_START"
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
CLASSIFY_JOB="$(workflow_job "$RELEASE_WORKFLOW" classify)"
PACKAGE_NPM_JOB="$(workflow_job "$RELEASE_WORKFLOW" package_npm)"
PACKAGE_INSTALLER_JOB="$(workflow_job "$RELEASE_WORKFLOW" package_installer)"
BUILD_JOB="$(workflow_job "$RELEASE_WORKFLOW" build)"
PREPARE_RELEASE_JOB="$(workflow_job "$RELEASE_WORKFLOW" prepare_github_release)"
STAGE_PREVIEW_JOB="$(workflow_job "$RELEASE_WORKFLOW" stage_npm_preview)"
STAGE_STABLE_JOB="$(workflow_job "$RELEASE_WORKFLOW" stage_npm_stable)"
TAG_VERSION_STEP="$(workflow_named_step "$CLASSIFY_JOB" "Verify release source")"

if [[ -z "$CLASSIFY_JOB" || -z "$VERIFY_JOB" || -z "$PACKAGE_NPM_JOB" || -z "$PACKAGE_INSTALLER_JOB" || -z "$BUILD_JOB" || -z "$PREPARE_RELEASE_JOB" || -z "$STAGE_PREVIEW_JOB" || -z "$STAGE_STABLE_JOB" ]]; then
  echo "release workflow is missing a classified build/package/draft/stage job" >&2
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
require_workflow_match "$TAG_VERSION_STEP" 'PUSH_REF_NAME:[[:space:]]+\$\{\{[[:space:]]*github\.ref_name' \
  "tag/version validation must receive the triggering ref name"
require_workflow_match "$TAG_VERSION_STEP" 'repo-guard[[:space:]]+--[[:space:]]+release-source' \
  "release source and channel classification must delegate to repo-guard"
require_workflow_match "$TAG_VERSION_STEP" '--event-name[[:space:]]+"\$\{EVENT_NAME\}"' \
  "release-source must receive the event name explicitly"
require_workflow_match "$TAG_VERSION_STEP" '--ref-name[[:space:]]+"\$\{PUSH_REF_NAME\}"' \
  "release-source must receive the ref name explicitly"
require_workflow_match "$CLASSIFY_JOB" 'git[[:space:]]+fetch[[:space:]]+--no-tags[[:space:]]+origin[[:space:]]+main:refs/remotes/origin/main' \
  "tag authority verification must fetch origin/main"
require_workflow_match "$CLASSIFY_JOB" 'fetch-depth:[[:space:]]+0' \
  "release classification must check out complete history for exact main/tag authority"

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
  "release matrix must contain exactly four supported targets"
require_workflow_absence "$BUILD_JOB" 'windows|Windows|pc-windows|\.zip|pwsh|PowerShell|Compress-Archive' \
  "release builds must not claim or package Windows support"
require_workflow_match "$BUILD_JOB" 'src/workers/python/worker\.py' \
  "release artifacts must package the Python worker"
require_workflow_match "$BUILD_JOB" '\.sha256' \
  "every release archive must have a checksum artifact"

# Build-only and tag runs retain the installer and one exact npm candidate.
require_workflow_match "$PACKAGE_INSTALLER_JOB" 'needs:[[:space:]]+verify' \
  "installer packaging must follow the verification gate"
require_workflow_match "$PACKAGE_INSTALLER_JOB" 'src/install/repogrammar-install\.sh' \
  "installer packaging must retain the documented source installer"
require_workflow_match "$PACKAGE_INSTALLER_JOB" 'install\.sh\.sha256' \
  "installer packaging must retain its exact checksum"
require_workflow_match "$PACKAGE_INSTALLER_JOB" 'name:[[:space:]]+repogrammar-installer' \
  "installer packaging must upload a stable artifact name"

require_workflow_match "$PACKAGE_NPM_JOB" 'needs:[[:space:]]+\[classify,[[:space:]]*verify\]' \
  "npm candidate packaging must follow classification and verification"
require_workflow_count_exactly "$PACKAGE_NPM_JOB" '^[[:space:]]+npm pack --json --ignore-scripts --pack-destination npm-candidate' 1 \
  "npm candidate must be packed exactly once"
require_workflow_match "$PACKAGE_NPM_JOB" 'smoke-npm-package' \
  "the exact npm candidate must pass the offline package smoke"
require_workflow_match "$PACKAGE_NPM_JOB" 'verify-npm-pack-evidence' \
  "npm pack metadata and SRI must be checked by repo-guard"
require_workflow_match "$PACKAGE_NPM_JOB" 'name:[[:space:]]+npm-package-\$\{\{[[:space:]]*needs\.classify\.outputs\.version' \
  "the exact npm candidate must be retained as a versioned artifact"

require_workflow_match "$PREPARE_RELEASE_JOB" 'needs:[[:space:]]+\[classify,[[:space:]]*build,[[:space:]]*package_installer,[[:space:]]*package_npm\]' \
  "the draft release must wait for every retained candidate"
require_workflow_match "$PREPARE_RELEASE_JOB" "if:[[:space:]]+github\.event_name[[:space:]]*==[[:space:]]*'push'.*github\.ref_type[[:space:]]*==[[:space:]]*'tag'" \
  "manual dispatch must not create a GitHub release"
require_workflow_match "$PREPARE_RELEASE_JOB" 'name:[[:space:]]+repogrammar-installer' \
  "the draft release must consume the retained installer artifact"
require_workflow_match "$PREPARE_RELEASE_JOB" 'npm-candidate-manifest\.json' \
  "the draft release must retain the exact npm candidate manifest as a public asset"
require_workflow_match "$PREPARE_RELEASE_JOB" 'find release-assets.*=[[:space:]]*"11"' \
  "the draft release must contain exactly eleven supported assets"
require_workflow_match "$PREPARE_RELEASE_JOB" 'overwrite_files:[[:space:]]+false' \
  "a rerun must never overwrite an existing draft candidate"
require_workflow_match "$PREPARE_RELEASE_JOB" 'fail_on_unmatched_files:[[:space:]]+true' \
  "missing candidate assets must fail release preparation"
require_workflow_match "$PREPARE_RELEASE_JOB" 'softprops/action-gh-release@3d0d9888cb7fd7b750713d6e236d1fcb99157228' \
  "the privileged GitHub release action must be pinned to the reviewed commit"
require_workflow_match "$PREPARE_RELEASE_JOB" 'draft:[[:space:]]+true' \
  "both release channels must remain draft-only before npm staging"
require_workflow_match "$PREPARE_RELEASE_JOB" 'prerelease:[[:space:]]+\$\{\{[[:space:]]*needs\.classify\.outputs\.channel[[:space:]]*==[[:space:]]*.*preview' \
  "GitHub prerelease truth must follow the typed release channel"
require_workflow_absence "$PREPARE_RELEASE_JOB" 'install\.ps1|windows|Windows' \
  "GitHub release assets must not publish unsupported Windows files"

for STAGE_JOB in "$STAGE_PREVIEW_JOB" "$STAGE_STABLE_JOB"; do
  require_workflow_match "$STAGE_JOB" "if:[[:space:]]+github\.event_name[[:space:]]*==[[:space:]]*'push'.*github\.ref_type[[:space:]]*==[[:space:]]*'tag'" \
    "npm staging must require a pushed tag"
  require_workflow_match "$STAGE_JOB" 'environment:[[:space:]]+npm-release' \
    "npm staging must use the protected trusted-publisher environment"
  require_workflow_match "$STAGE_JOB" 'id-token:[[:space:]]+write' \
    "npm staging must request short-lived OIDC authority"
  require_workflow_match "$STAGE_JOB" 'actions/download-artifact' \
    "npm staging must download the retained candidate"
  require_workflow_match "$STAGE_JOB" 'name:[[:space:]]+npm-package-\$\{\{[[:space:]]*needs\.classify\.outputs\.version' \
    "npm staging must consume the versioned candidate artifact"
  require_workflow_match "$STAGE_JOB" 'verify-npm-pack-evidence' \
    "npm staging must re-verify candidate SRI without repacking"
  require_workflow_absence "$STAGE_JOB" '^[[:space:]]+npm pack|npm[[:space:]]+publish|NPM_TOKEN|NODE_AUTH_TOKEN' \
    "npm staging must not repack, directly publish, or use a traditional token"
done
require_workflow_match "$STAGE_PREVIEW_JOB" '^[[:space:]]+npm stage publish.*--tag[[:space:]]+preview.*--provenance' \
  "preview must stage the retained package with preview and provenance"
require_workflow_match "$STAGE_PREVIEW_JOB" '^[[:space:]]+package_file="\./npm-candidate/sioyooo-repogrammar-\$\{\{ needs\.classify\.outputs\.version \}\}\.tgz"' \
  "preview staging must use an explicit relative local tarball path"
require_workflow_match "$STAGE_STABLE_JOB" '^[[:space:]]+npm stage publish \./npm-candidate/sioyooo-repogrammar-0\.4\.2\.tgz --access public --tag latest --provenance' \
  "stable must use the one exact registered staging command"
require_workflow_absence "$RELEASE_WORKFLOW" 'NPM_TOKEN|NODE_AUTH_TOKEN|npm[[:space:]]+publish|npm[[:space:]]+stage[[:space:]]+(approve|reject)|npm[[:space:]]+dist-tag' \
  "release automation must remain token-free, stage-only, and unable to approve or mutate tags"

NPM_TAG_WORKFLOW="${SCRIPT_DIR}/../../.github/workflows/npm-tag-reconcile.yml"
STABLE_FINALIZER="${SCRIPT_DIR}/../../.github/workflows/stable-release-finalize.yml"
NPM_TAG_WORKFLOW_BODY="$(<"$NPM_TAG_WORKFLOW")"
STABLE_FINALIZER_BODY="$(<"$STABLE_FINALIZER")"
NPM_TAG_VERIFY_JOB="$(workflow_job "$NPM_TAG_WORKFLOW" verify)"
STABLE_FINALIZER_JOB="$(workflow_job "$STABLE_FINALIZER" verify_public_release)"
if [[ -z "$NPM_TAG_VERIFY_JOB" || -z "$STABLE_FINALIZER_JOB" ]]; then
  echo "release verification workflows are missing their read-only jobs" >&2
  exit 1
fi
require_workflow_match "$NPM_TAG_WORKFLOW_BODY" 'workflow_dispatch:' \
  "npm tag verification must remain manually dispatchable"
require_workflow_match "$NPM_TAG_VERIFY_JOB" 'release-dist-tag-action' \
  "npm tag verification must delegate exact channel policy to repo-guard"
require_workflow_match "$NPM_TAG_VERIFY_JOB" 'stable:stable_latest_verified' \
  "stable verification must require the exact registered dist-tag state"
require_workflow_match "$NPM_TAG_VERIFY_JOB" '--preview[[:space:]]+"\$\{preview\}"' \
  "npm tag verification must pass the observed preview tag to repo-guard"
require_workflow_match "$NPM_TAG_VERIFY_JOB" '--latest[[:space:]]+"\$\{latest\}"' \
  "npm tag verification must pass the observed latest tag to repo-guard"
require_workflow_match "$NPM_TAG_VERIFY_JOB" '--tags-json[[:space:]]+"\$\{tags\}"' \
  "npm tag verification must pass the complete dist-tag object to repo-guard"
require_workflow_match "$NPM_TAG_VERIFY_JOB" '--versions-json[[:space:]]+"\$\{versions_json\}"' \
  "npm tag verification must pass the complete version inventory to repo-guard"
require_workflow_absence "$NPM_TAG_WORKFLOW_BODY" 'npm[[:space:]]+(publish|stage|dist-tag)|NPM_TOKEN|NODE_AUTH_TOKEN|id-token:[[:space:]]+write' \
  "npm tag verification must be read-only"

require_workflow_match "$STABLE_FINALIZER_BODY" 'candidate_run_id:' \
  "stable finalization must bind the candidate tag workflow run"
require_workflow_match "$STABLE_FINALIZER_BODY" 'candidate_run_attempt:' \
  "stable finalization must bind an exact immutable workflow attempt"
require_workflow_match "$STABLE_FINALIZER_BODY" 'contents:[[:space:]]+read' \
  "stable finalization must have read-only repository authority"
require_workflow_match "$STABLE_FINALIZER_BODY" 'actions:[[:space:]]+read' \
  "stable finalization must have read-only artifact authority"
require_workflow_match "$STABLE_FINALIZER_JOB" '^[[:space:]]+gh release verify v0\.4\.2' \
  "stable finalization must verify the immutable release attestation"
require_workflow_match "$STABLE_FINALIZER_JOB" '^[[:space:]]+gh release verify-asset v0\.4\.2' \
  "stable finalization must verify every downloaded release asset"
require_workflow_match "$STABLE_FINALIZER_JOB" '^[[:space:]]+npm audit signatures --json --include-attestations' \
  "stable finalization must collect registry signature and provenance evidence"
require_workflow_match "$STABLE_FINALIZER_JOB" 'actions/runs/\$\{CANDIDATE_RUN_ID\}/attempts/\$\{CANDIDATE_RUN_ATTEMPT\}' \
  "stable finalization must collect the exact candidate workflow attempt"
require_workflow_match "$STABLE_FINALIZER_JOB" 'git rev-parse HEAD.*expected-head-sha\.txt' \
  "stable finalization must bind the candidate run to the checked-out tag commit"
require_workflow_match "$STABLE_FINALIZER_JOB" 'github-assets/npm-candidate-manifest\.json' \
  "stable finalization must consume the immutable npm manifest from the public release"
require_workflow_match "$STABLE_FINALIZER_JOB" 'retained-candidate-manifest\.json' \
  "stable finalization must preserve the public candidate manifest as release evidence"
require_workflow_match "$STABLE_FINALIZER_JOB" 'public-npm-pack\.json' \
  "stable finalization must preserve public pack/SRI evidence"
require_workflow_match "$STABLE_FINALIZER_JOB" 'verify-npm-pack-evidence' \
  "stable finalization must verify public npm bytes before executing them"
require_workflow_match "$STABLE_FINALIZER_JOB" 'public-native-smoke\.txt' \
  "stable finalization must smoke the verified public native artifact"
require_workflow_match "$STABLE_FINALIZER_JOB" 'public-installer-version\.txt' \
  "stable finalization must smoke the verified public installer"
require_workflow_match "$STABLE_FINALIZER_JOB" 'npm-tags\.json' \
  "stable finalization must collect public dist-tag evidence"
require_workflow_match "$STABLE_FINALIZER_JOB" 'npm-versions\.json' \
  "stable finalization must collect the complete published-version inventory"
require_workflow_match "$STABLE_FINALIZER_JOB" '^[[:space:]]+run:[[:space:]]+cargo run --quiet --locked --bin repo-guard -- verify-stable-release-evidence --evidence-dir evidence' \
  "stable finalization must delegate the final verdict to repo-guard"
require_workflow_match "$STABLE_FINALIZER_JOB" '@sioyooo/repogrammar@0\.4\.2' \
  "stable finalization must smoke the exact stable npm version"
require_workflow_match "$STABLE_FINALIZER_JOB" '@sioyooo/repogrammar@preview' \
  "stable finalization must preserve and smoke the preview channel"
STABLE_FINALIZER_NPM_SMOKE="$(workflow_named_step "$STABLE_FINALIZER_JOB" "Collect public npm launcher smoke evidence")"
if [[ -z "$STABLE_FINALIZER_NPM_SMOKE" ]]; then
  echo "stable finalization must have a named public npm launcher smoke step" >&2
  exit 1
fi
require_workflow_match "$STABLE_FINALIZER_JOB" "if: github\.ref != 'refs/heads/main'" \
  "stable finalization must fail visibly when dispatched from a verifier definition outside main"
require_workflow_match "$STABLE_FINALIZER_NPM_SMOKE" \
  '^.*smoke_root="\$\{RUNNER_TEMP\}/public-release-smoke"[[:space:]]*$' \
  "stable finalization must root public npm launcher work outside the checkout"
require_workflow_absence "$STABLE_FINALIZER_NPM_SMOKE" \
  'smoke_root="\$\{GITHUB_WORKSPACE\}/' \
  "stable finalization must not place public npm launcher work under the checkout"
require_workflow_match "$STABLE_FINALIZER_NPM_SMOKE" \
  '^.*for tool in .* git .*; do[[:space:]]*$' \
  "stable finalization must make git available to repository initialization in the tool-only PATH"
for STABLE_NPM_LANE in pinned latest preview; do
  require_workflow_match "$STABLE_FINALIZER_NPM_SMOKE" \
    "\"\\\${smoke_root}/${STABLE_NPM_LANE}/work\"" \
    "stable finalization must create an external work directory for the ${STABLE_NPM_LANE} npm lane"
done
require_workflow_count_exactly "$STABLE_FINALIZER_NPM_SMOKE" \
  '^[[:space:]]+cd "\$\{smoke_root\}/\$\{lane\}/work"[[:space:]]*$' 1 \
  "the npm launcher helper must enter the selected lane's external work directory exactly once"
require_workflow_match "$STABLE_FINALIZER_NPM_SMOKE" '^[[:space:]]+\([[:space:]]*$' \
  "the npm launcher helper must isolate its working-directory change in a child shell"
require_workflow_match "$STABLE_FINALIZER_NPM_SMOKE" '^[[:space:]]+\)[[:space:]]*$' \
  "the npm launcher helper must close its isolated child shell"
require_workflow_absence "$STABLE_FINALIZER_BODY" 'npm[[:space:]]+(publish|stage|dist-tag)|NPM_TOKEN|NODE_AUTH_TOKEN|id-token:[[:space:]]+write|contents:[[:space:]]+write|actions:[[:space:]]+write' \
  "stable finalization must remain read-only"

# The release matrix must smoke the exact archive it uploads. Source-tree
# binaries do not prove that an archive is executable or contains the runtime
# worker. The smoke initializes repository state explicitly, then runs the
# combined compatibility journey only to exercise the product tools/list
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
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" 'expected_version=.*package\.json' \
  "packaged smoke must derive the expected version from the release manifest"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" 'cargo run --quiet --locked --bin repo-guard -- smoke-packaged-artifact' \
  "packaged smoke must delegate product lifecycle assertions to repo-guard"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" '--binary.*binary' \
  "packaged smoke must pass the extracted binary to repo-guard"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" '--worker.*worker' \
  "packaged smoke must pass the extracted worker to repo-guard"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" 'src/fixtures/python/release/v0_1/pydantic-basic/schemas\.py' \
  "packaged smoke must use the exact committed Pydantic release fixture"
require_workflow_match "$PACKAGED_ARTIFACT_SMOKE" '--expected-version.*expected_version' \
  "packaged smoke must require exact binary/manifest version agreement"
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
LINUX_PACKAGED_SMOKE_JOB="$(workflow_job "$CI_WORKFLOW" linux-packaged-product-smoke)"
WINDOWS_SOURCE_JOB="$(workflow_job "$CI_WORKFLOW" windows-source-installer-contract)"
if [[ -z "$MACOS_SMOKE_JOB" ]]; then
  echo "CI must include the macos-product-smoke job" >&2
  exit 1
fi
require_workflow_match "$MACOS_SMOKE_JOB" 'runs-on:[[:space:]]+macos-' \
  "macOS product smoke must run on a macOS runner"
require_workflow_match "$MACOS_SMOKE_JOB" 'cargo test --locked --workspace --all-features' \
  "macOS coverage must exercise the Rust workspace rather than compilation only"
require_workflow_match "$MACOS_SMOKE_JOB" 'tar[[:space:]].*-x' \
  "macOS coverage must extract its candidate package"
require_workflow_match "$MACOS_SMOKE_JOB" 'smoke-packaged-artifact' \
  "macOS coverage must exercise the packaged product lifecycle"
require_workflow_match "$MACOS_SMOKE_JOB" 'src/fixtures/python/release/v0_1/pydantic-basic/schemas\.py' \
  "macOS packaged coverage must use the committed Pydantic fixture"

if [[ -z "$LINUX_PACKAGED_SMOKE_JOB" ]]; then
  echo "CI must include the linux-packaged-product-smoke job" >&2
  exit 1
fi
require_workflow_match "$LINUX_PACKAGED_SMOKE_JOB" 'runs-on:[[:space:]]+ubuntu-22\.04' \
  "Linux packaged smoke must run on the declared x86_64 release floor"
require_workflow_match "$LINUX_PACKAGED_SMOKE_JOB" 'tar[[:space:]].*-x' \
  "Linux packaged smoke must extract its candidate package"
require_workflow_match "$LINUX_PACKAGED_SMOKE_JOB" 'smoke-packaged-artifact' \
  "Linux coverage must exercise the packaged product lifecycle"
require_workflow_match "$LINUX_PACKAGED_SMOKE_JOB" 'src/fixtures/python/release/v0_1/pydantic-basic/schemas\.py' \
  "Linux packaged coverage must use the committed Pydantic fixture"

if [[ -z "$WINDOWS_SOURCE_JOB" ]]; then
  echo "CI must include the Windows source-only installer contract job" >&2
  exit 1
fi
require_workflow_match "$WINDOWS_SOURCE_JOB" 'name:[[:space:]]+Windows source-only installer contract' \
  "Windows CI must remain explicitly source-only"
require_workflow_match "$WINDOWS_SOURCE_JOB" 'runs-on:[[:space:]]+windows-' \
  "Windows source-only installer tests must run on a native Windows runner"
require_workflow_match "$WINDOWS_SOURCE_JOB" 'dtolnay/rust-toolchain@4cda84d5c5c54efe2404f9d843567869ab1699d4' \
  "Windows source-only installer tests must use the reviewed Rust action pin"
require_workflow_match "$WINDOWS_SOURCE_JOB" 'toolchain:[[:space:]]+stable' \
  "Windows source-only installer tests must request stable Rust explicitly"
require_workflow_match "$WINDOWS_SOURCE_JOB" 'install\.ps1\.test\.ps1' \
  "Windows source-only installer tests must execute the native PowerShell contract"
require_workflow_absence "$WINDOWS_SOURCE_JOB" 'upload-artifact|release|repogrammar-.*windows|npm publish' \
  "Windows source-only CI must not imply a release artifact or publication claim"

WINDOWS_INSTALLER="${SCRIPT_DIR}/install.ps1"
# Windows remains a source-checkout contributor path, not a supported release
# release artifact or npm platform claim.
grep -q "FromSource" "$WINDOWS_INSTALLER"
grep -q "REPOGRAMMAR_SOURCE_BINARY" "$WINDOWS_INSTALLER"
grep -q "cargo build --release" "$WINDOWS_INSTALLER"
grep -q "Windows has no supported RepoGrammar release artifact" "$WINDOWS_INSTALLER"
grep -q "installation requires explicit -FromSource" "$WINDOWS_INSTALLER"
if grep -Eq 'repogrammar-x86_64-pc-windows-msvc|Install-CliFromRelease|Get-WindowsArtifactName|Invoke-WebRequest' "$WINDOWS_INSTALLER"; then
  echo "Windows contributor installer still contains a release-download path" >&2
  exit 1
fi
WINDOWS_INSTALLER_TEST="${SCRIPT_DIR}/install.ps1.test.ps1"
grep -q "Windows default release install unexpectedly succeeded" "$WINDOWS_INSTALLER_TEST"
grep -q "refused Windows release install created command or install state" "$WINDOWS_INSTALLER_TEST"
