#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
REPOGRAMMAR_REPO="${REPOGRAMMAR_REPO:-SioYooo/RepoGrammar}"
REPOGRAMMAR_VERSION="${REPOGRAMMAR_VERSION:-latest}"
REPOGRAMMAR_BIN="${REPOGRAMMAR_SOURCE_BINARY:-${REPO_ROOT}/target/release/repogrammar}"
COMMAND_DIR="${REPOGRAMMAR_COMMAND_DIR:-${HOME:-}/.local/bin}"
COMMAND_PATH="${COMMAND_DIR}/repogrammar"
if [[ -n "${REPOGRAMMAR_INSTALL_DIR:-}" ]]; then
  DATA_DIR="$REPOGRAMMAR_INSTALL_DIR"
elif [[ -n "${XDG_DATA_HOME:-}" ]]; then
  DATA_DIR="${XDG_DATA_HOME%/}/repogrammar"
else
  DATA_DIR="${HOME:-}/.local/share/repogrammar"
fi
DATA_BIN_DIR="${DATA_DIR}/bin"
INSTALLED_EXECUTABLE="${DATA_BIN_DIR}/repogrammar"
WORKER_ROOT="${REPOGRAMMAR_WORKER_ROOT:-}"
ACTION="menu"
ASSUME_YES=0
TARGET_SELECTION="all"
INSTALL_SCOPE="global"
USE_SOURCE_BUILD="${REPOGRAMMAR_USE_SOURCE_BUILD:-0}"
TMP_DIRS=()

cleanup() {
  set +u
  local dir
  for dir in "${TMP_DIRS[@]}"; do
    rm -rf "$dir"
  done
}
trap cleanup EXIT

die() {
  printf "error: %s\n" "$1" >&2
  exit 1
}

usage() {
  cat <<'USAGE'
RepoGrammar installer

Usage:
  repogrammar-install.sh                         # interactive setup menu
  repogrammar-install.sh --install-and-configure # install CLI, then run agent wizard
  repogrammar-install.sh --install-cli-only      # install CLI only
  repogrammar-install.sh --uninstall-agents      # remove RepoGrammar-owned agent wiring
  repogrammar-install.sh --uninstall-command     # remove local repogrammar command
  repogrammar-install.sh --uninstall-all         # remove agent wiring and local command

Options:
  --yes                  Do not prompt for installer confirmations
  --target <agents>      auto, all, none, or comma-separated agent ids
  --scope <scope>        global or local/project for delegated agent actions
  --location <scope>     Alias for --scope
  --version <tag>        Release tag to install; default: latest
  --command-dir <dir>    Directory for the repogrammar command
  --install-dir <dir>    Directory for RepoGrammar-managed install state
  --from-source          Contributor path: build/copy target/release/repogrammar
  --print-target         Print detected release target and exit
  -h, --help             Show this help

Environment:
  REPOGRAMMAR_RELEASE_DIR    Local directory containing release artifacts, used by tests
  REPOGRAMMAR_RELEASE_BASE   Override release asset URL base
  REPOGRAMMAR_COMMAND_DIR    Directory for the repogrammar command
  REPOGRAMMAR_INSTALL_DIR    Directory for RepoGrammar-managed install state
  REPOGRAMMAR_SOURCE_BINARY  Prebuilt source-checkout binary for dogfood tests
  REPOGRAMMAR_WORKER_ROOT    Directory for bundled worker assets
  REPOGRAMMAR_VERSION        Release tag, or latest
  REPOGRAMMAR_USE_SOURCE_BUILD=1  Build from source instead of downloading
USAGE
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --install-and-configure) ACTION="install_and_configure"; shift ;;
      --install-cli-only) ACTION="install_cli_only"; shift ;;
      --configure-agents) ACTION="configure_agents"; shift ;;
      --uninstall-agents) ACTION="uninstall_agents"; shift ;;
      --uninstall-command) ACTION="uninstall_command"; shift ;;
      --uninstall-all) ACTION="uninstall_all"; shift ;;
      --yes) ASSUME_YES=1; shift ;;
      --target)
        [[ $# -ge 2 ]] || die "--target requires a value"
        TARGET_SELECTION="$2"
        shift 2
        ;;
      --scope|--location)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        INSTALL_SCOPE="$2"
        shift 2
        ;;
      --version)
        [[ $# -ge 2 ]] || die "--version requires a value"
        REPOGRAMMAR_VERSION="$2"
        shift 2
        ;;
      --command-dir)
        [[ $# -ge 2 ]] || die "--command-dir requires a value"
        COMMAND_DIR="$2"
        COMMAND_PATH="${COMMAND_DIR}/repogrammar"
        shift 2
        ;;
      --install-dir)
        [[ $# -ge 2 ]] || die "--install-dir requires a value"
        DATA_DIR="$2"
        DATA_BIN_DIR="${DATA_DIR}/bin"
        INSTALLED_EXECUTABLE="${DATA_BIN_DIR}/repogrammar"
        shift 2
        ;;
      --from-source) USE_SOURCE_BUILD=1; shift ;;
      --print-target) ACTION="print_target"; shift ;;
      -h|--help) usage; exit 0 ;;
      *) die "unknown option: $1" ;;
    esac
  done
}

prompt_default_no() {
  local prompt="$1"
  local reply
  if [[ "$ASSUME_YES" -eq 1 ]]; then
    return 0
  fi
  printf "%s [y/N] " "$prompt"
  IFS= read -r reply || return 1
  case "$(printf "%s" "$reply" | tr '[:upper:]' '[:lower:]')" in
    y|yes) return 0 ;;
    *) return 1 ;;
  esac
}

detect_target() {
  local os
  local arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    arm64|aarch64) arch="aarch64" ;;
    *) die "unsupported architecture: $arch" ;;
  esac
  case "$os" in
    Darwin) printf "%s-apple-darwin" "$arch" ;;
    Linux) printf "%s-unknown-linux-gnu" "$arch" ;;
    *) die "unsupported OS for this installer: $os; use install.ps1 on Windows" ;;
  esac
}

artifact_name() {
  printf "repogrammar-%s.tar.gz" "$(detect_target)"
}

release_asset_base() {
  if [[ -n "${REPOGRAMMAR_RELEASE_BASE:-}" ]]; then
    printf "%s" "${REPOGRAMMAR_RELEASE_BASE%/}"
  elif [[ "$REPOGRAMMAR_VERSION" == "latest" ]]; then
    printf "https://github.com/%s/releases/latest/download" "$REPOGRAMMAR_REPO"
  else
    printf "https://github.com/%s/releases/download/%s" "$REPOGRAMMAR_REPO" "$REPOGRAMMAR_VERSION"
  fi
}

fetch_asset() {
  local name="$1"
  local dest="$2"
  if [[ -n "${REPOGRAMMAR_RELEASE_DIR:-}" ]]; then
    local local_asset="${REPOGRAMMAR_RELEASE_DIR%/}/${name}"
    if [[ ! -f "$local_asset" ]]; then
      die "release artifact not found in REPOGRAMMAR_RELEASE_DIR: ${name}"
    fi
    cp "$local_asset" "$dest"
    return
  fi

  local url
  url="$(release_asset_base)/${name}"
  if command -v curl >/dev/null 2>&1; then
    if ! curl -fsSL "$url" -o "$dest" 2>/dev/null; then
      release_asset_not_found "$url"
    fi
  elif command -v wget >/dev/null 2>&1; then
    if ! wget -q "$url" -O "$dest" 2>/dev/null; then
      release_asset_not_found "$url"
    fi
  else
    die "curl or wget is required to download release artifacts"
  fi
}

release_asset_not_found() {
  local url="$1"
  printf "error: release artifact was not found: %s\n" "$url" >&2
  if has_source_checkout; then
    printf "This looks like a RepoGrammar source checkout; rerun with --from-source to build and install locally.\n" >&2
  fi
  printf "For local artifact testing, set REPOGRAMMAR_RELEASE_DIR to a directory containing the archive and .sha256 file.\n" >&2
  exit 1
}

sha256_file() {
  local path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$path" | awk '{print $1}'
  else
    die "sha256sum or shasum is required for checksum verification"
  fi
}

verify_checksum() {
  local archive="$1"
  local checksum_file="$2"
  local expected
  local actual
  expected="$(awk '{print $1}' "$checksum_file")"
  actual="$(sha256_file "$archive")"
  if [[ -z "$expected" || "$expected" != "$actual" ]]; then
    die "checksum verification failed for $(basename "$archive")"
  fi
}

command_path_is_managed() {
  if [[ ! -e "$COMMAND_PATH" && ! -L "$COMMAND_PATH" ]]; then
    return 0
  fi
  if [[ -L "$COMMAND_PATH" ]]; then
    local link_target
    link_target="$(readlink "$COMMAND_PATH" 2>/dev/null || true)"
    [[ "$link_target" == "$INSTALLED_EXECUTABLE" ]]
    return
  fi
  [[ -f "$COMMAND_PATH" && -f "$INSTALLED_EXECUTABLE" ]] && cmp -s "$COMMAND_PATH" "$INSTALLED_EXECUTABLE"
}

install_managed_cli_binary() {
  local source="$1"
  if ! command_path_is_managed; then
    die "repogrammar command path already exists and is not managed by RepoGrammar; move it aside or choose --command-dir"
  fi
  mkdir -p "$DATA_BIN_DIR"
  local tmp_executable="${INSTALLED_EXECUTABLE}.tmp.$$"
  cp "$source" "$tmp_executable"
  chmod 755 "$tmp_executable"
  mv "$tmp_executable" "$INSTALLED_EXECUTABLE"

  mkdir -p "$COMMAND_DIR"
  if [[ -e "$COMMAND_PATH" || -L "$COMMAND_PATH" ]]; then
    rm -f "$COMMAND_PATH"
  fi
  if ! ln -s "$INSTALLED_EXECUTABLE" "$COMMAND_PATH" 2>/dev/null; then
    local tmp_command="${COMMAND_PATH}.tmp.$$"
    cp "$INSTALLED_EXECUTABLE" "$tmp_command"
    chmod 755 "$tmp_command"
    mv "$tmp_command" "$COMMAND_PATH"
  fi
}

default_worker_root() {
  if [[ -n "$WORKER_ROOT" ]]; then
    printf "%s" "$WORKER_ROOT"
    return
  fi
  printf "%s/workers" "$DATA_DIR"
}

command_worker_root() {
  printf "%s/repogrammar-workers" "$COMMAND_DIR"
}

install_worker_asset() {
  local worker_source="$1"
  if [[ ! -f "$worker_source" ]]; then
    return
  fi
  local worker_dest_root
  worker_dest_root="$(default_worker_root)"
  mkdir -p "${worker_dest_root}/python"
  cp "$worker_source" "${worker_dest_root}/python/worker.py"
  if [[ -z "$WORKER_ROOT" ]]; then
    local command_dest_root
    command_dest_root="$(command_worker_root)"
    if [[ "$command_dest_root" != "$worker_dest_root" ]]; then
      mkdir -p "${command_dest_root}/python"
      cp "$worker_source" "${command_dest_root}/python/worker.py"
    fi
  fi
}

has_source_checkout() {
  [[ -f "${REPO_ROOT}/Cargo.toml" && -d "${REPO_ROOT}/src/rust" ]]
}

install_cli_from_release() {
  local target
  local artifact
  local tmpdir
  target="$(detect_target)"
  artifact="$(artifact_name)"
  tmpdir="$(mktemp -d)"
  TMP_DIRS+=("$tmpdir")

  printf "Installing RepoGrammar %s for %s\n" "$REPOGRAMMAR_VERSION" "$target"
  fetch_asset "$artifact" "${tmpdir}/${artifact}"
  fetch_asset "${artifact}.sha256" "${tmpdir}/${artifact}.sha256"
  verify_checksum "${tmpdir}/${artifact}" "${tmpdir}/${artifact}.sha256"
  tar -xzf "${tmpdir}/${artifact}" -C "$tmpdir"
  [[ -x "${tmpdir}/repogrammar" ]] || die "release artifact did not contain executable repogrammar"
  install_managed_cli_binary "${tmpdir}/repogrammar"
  install_worker_asset "${tmpdir}/workers/python/worker.py"
  printf "Installed %s\n" "$COMMAND_PATH"
}

install_cli_from_source() {
  if ! has_source_checkout; then
    die "source build requires running this script from a RepoGrammar source checkout"
  fi
  if [[ ! -x "$REPOGRAMMAR_BIN" ]]; then
    printf "RepoGrammar release binary is not built yet.\n"
    if ! prompt_default_no "Build it now with cargo build --release?"; then
      printf "Cancelled. Build manually with: cargo build --release\n"
      return 1
    fi
    (cd "$REPO_ROOT" && cargo build --release)
  fi
  install_managed_cli_binary "$REPOGRAMMAR_BIN"
  install_worker_asset "${REPO_ROOT}/src/workers/python/worker.py"
  printf "Installed %s from source build\n" "$COMMAND_PATH"
}

install_cli_binary() {
  if [[ "$USE_SOURCE_BUILD" == "1" ]]; then
    install_cli_from_source
  else
    install_cli_from_release
  fi
}

resolve_repogrammar_command() {
  if [[ -x "$COMMAND_PATH" ]]; then
    printf "%s" "$COMMAND_PATH"
  elif command -v repogrammar >/dev/null 2>&1; then
    command -v repogrammar
  else
    return 1
  fi
}

run_agent_install() {
  local command_path
  local executable_path
  command_path="$(resolve_repogrammar_command)" || die "repogrammar command is not installed; choose install first"
  if [[ -x "$INSTALLED_EXECUTABLE" ]]; then
    executable_path="$INSTALLED_EXECUTABLE"
  else
    executable_path="$command_path"
  fi
  if [[ "$ASSUME_YES" -eq 1 ]]; then
    REPOGRAMMAR_INSTALL_DIR="$DATA_DIR" \
    REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
    REPOGRAMMAR_EXECUTABLE="$executable_path" \
    "$command_path" install \
      --target "$TARGET_SELECTION" \
      --scope "$INSTALL_SCOPE" \
      --yes \
      --no-telemetry
  else
    REPOGRAMMAR_INSTALL_DIR="$DATA_DIR" \
    REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
    REPOGRAMMAR_EXECUTABLE="$executable_path" \
    "$command_path" install
  fi
}

install_and_configure() {
  install_cli_binary
  run_agent_install
  print_command_status
}

select_agent_target() {
  printf "\nSelect connected coding-agent integrations:\n" >&2
  printf "  1 = Codex\n" >&2
  printf "  2 = Claude Code\n" >&2
  printf "  3 = both\n" >&2
  printf "  a = all first-class RepoGrammar agents\n" >&2
  printf "  q = cancel\n\n" >&2
  printf "Selection [3]: " >&2
  local reply
  IFS= read -r reply || return 1
  case "${reply:-3}" in
    1) printf "codex" ;;
    2) printf "claude-code" ;;
    3|a|A|all) printf "all" ;;
    q|Q) return 1 ;;
    *) printf "Invalid selection.\n" >&2; return 2 ;;
  esac
}

uninstall_connected_agents() {
  local command_path
  command_path="$(resolve_repogrammar_command)" || die "repogrammar command is not installed; cannot uninstall managed agent integrations"
  local target="$TARGET_SELECTION"
  if [[ "$ASSUME_YES" -eq 0 ]]; then
    target="$(select_agent_target)" || {
      printf "Cancelled. No connected coding-agent integrations were removed.\n"
      return 0
    }
  fi
  if ! prompt_default_no "Remove RepoGrammar-owned ${target} MCP integration?"; then
    printf "Cancelled. No connected coding-agent integrations were removed.\n"
    return 0
  fi
  REPOGRAMMAR_INSTALL_DIR="$DATA_DIR" \
  REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" \
  REPOGRAMMAR_EXECUTABLE="$command_path" \
  "$command_path" uninstall \
    --target "$target" \
    --scope "$INSTALL_SCOPE" \
    --yes
}

uninstall_command() {
  if [[ ! -e "$COMMAND_PATH" ]]; then
    printf "No repogrammar command found at %s\n" "$COMMAND_PATH"
    return 0
  fi
  if [[ ! -L "$COMMAND_PATH" && ! -f "$COMMAND_PATH" ]]; then
    printf "Refusing to remove non-file command path: %s\n" "$COMMAND_PATH"
    return 1
  fi
  if ! prompt_default_no "Remove repogrammar command at ${COMMAND_PATH}?"; then
    printf "Cancelled. The repogrammar command was not removed.\n"
    return 0
  fi
  rm -f "$COMMAND_PATH"
  printf "Removed %s\n" "$COMMAND_PATH"
}

print_command_status() {
  printf "\nCommand status:\n"
  if [[ -x "$COMMAND_PATH" ]]; then
    printf "  repogrammar: %s\n" "$COMMAND_PATH"
    "$COMMAND_PATH" version || true
  elif command -v repogrammar >/dev/null 2>&1; then
    printf "  repogrammar: %s\n" "$(command -v repogrammar)"
    repogrammar version || true
  elif [[ ":${PATH}:" == *":${COMMAND_DIR}:"* ]]; then
    printf "  repogrammar was not found on PATH. Try: hash -r\n"
  else
    printf "  %s is not on PATH.\n" "$COMMAND_DIR"
    printf "  Add this to your shell profile:\n"
    printf "    export PATH=\"%s:\$PATH\"\n" "$COMMAND_DIR"
  fi
}

main_menu() {
  printf "RepoGrammar setup\n\n"
  printf "This script installs or uninstalls the RepoGrammar command and\n"
  printf "machine-level Codex / Claude Code MCP integrations.\n\n"
  printf "Default install downloads a prebuilt release binary; Rust/Cargo is only\n"
  printf "needed when you choose the contributor source-build path.\n\n"
  printf "It does not index this repository.\n"
  printf "It does not create or modify .repogrammar/.\n"
  printf "It does not edit instruction files.\n"
  printf "Telemetry remains controlled by repogrammar install prompts and flags.\n\n"
  printf "Command directory: %s\n\n" "$COMMAND_DIR"
  printf "Choose an action:\n"
  if has_source_checkout; then
    printf "  1 = build/install from this source checkout and configure coding agents\n"
    printf "  2 = build/install command from this source checkout only\n"
  else
    printf "  1 = install or update repogrammar and configure coding agents\n"
    printf "  2 = install or update repogrammar command only\n"
  fi
  printf "  3 = configure coding agents only\n"
  printf "  4 = uninstall connected coding-agent integrations\n"
  printf "  5 = uninstall repogrammar command only\n"
  printf "  6 = uninstall connected agents and repogrammar command\n"
  if has_source_checkout; then
    printf "  7 = install/update from release artifact instead\n"
  fi
  printf "  q = cancel\n\n"
  printf "Selection [1]: "
}

run_menu() {
  local choice
  main_menu
  IFS= read -r choice || exit 1
  case "${choice:-1}" in
    1)
      if has_source_checkout; then
        USE_SOURCE_BUILD=1
      fi
      install_and_configure
      ;;
    2)
      if has_source_checkout; then
        USE_SOURCE_BUILD=1
      fi
      install_cli_binary
      print_command_status
      ;;
    3) run_agent_install ;;
    4) uninstall_connected_agents ;;
    5) uninstall_command ;;
    6) uninstall_connected_agents; uninstall_command ;;
    7) USE_SOURCE_BUILD=0; install_cli_binary; print_command_status ;;
    q|Q) printf "Cancelled. No changes made.\n" ;;
    *) printf "Invalid selection.\n" >&2; exit 2 ;;
  esac
}

main() {
  parse_args "$@"
  case "$ACTION" in
    print_target) detect_target; printf "\n" ;;
    install_cli_only) install_cli_binary; print_command_status ;;
    install_and_configure) install_and_configure ;;
    configure_agents) run_agent_install ;;
    uninstall_agents) uninstall_connected_agents ;;
    uninstall_command) uninstall_command ;;
    uninstall_all) uninstall_connected_agents; uninstall_command ;;
    menu) run_menu ;;
    *) die "unknown action: $ACTION" ;;
  esac
}

main "$@"
