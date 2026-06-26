#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
REPOGRAMMAR_BIN="${REPO_ROOT}/target/release/repogrammar"
COMMAND_DIR="${REPOGRAMMAR_COMMAND_DIR:-${HOME:-}/.local/bin}"
COMMAND_PATH="${COMMAND_DIR}/repogrammar"

prompt_default_no() {
  local prompt="$1"
  local reply
  printf "%s [y/N] " "$prompt"
  IFS= read -r reply || return 1
  case "$(printf "%s" "$reply" | tr '[:upper:]' '[:lower:]')" in
    y|yes) return 0 ;;
    *) return 1 ;;
  esac
}

ensure_release_binary() {
  if [[ -x "$REPOGRAMMAR_BIN" ]]; then
    return 0
  fi

  printf "RepoGrammar release binary is not built yet.\n"
  if ! prompt_default_no "Build it now with cargo build --release?"; then
    printf "Cancelled. Build manually with: cargo build --release\n"
    return 1
  fi
  (cd "$REPO_ROOT" && cargo build --release)
}

run_repogrammar_install() {
  ensure_release_binary
  mkdir -p "$COMMAND_DIR"
  REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" "$REPOGRAMMAR_BIN" install
  print_command_status
}

select_agent_target() {
  printf "\nSelect connected coding-agent integrations:\n" >&2
  printf "  1 = Codex\n" >&2
  printf "  2 = Claude Code\n" >&2
  printf "  3 = both\n" >&2
  printf "  q = cancel\n\n" >&2
  printf "Selection [3]: " >&2
  local reply
  IFS= read -r reply || return 1
  case "${reply:-3}" in
    1) printf "codex" ;;
    2) printf "claude-code" ;;
    3) printf "all" ;;
    q|Q) return 1 ;;
    *) printf "Invalid selection.\n" >&2; return 2 ;;
  esac
}

uninstall_connected_agents() {
  ensure_release_binary
  local target
  target="$(select_agent_target)" || {
    printf "Cancelled. No connected coding-agent integrations were removed.\n"
    return 0
  }
  if ! prompt_default_no "Remove RepoGrammar-owned ${target} MCP integration?"; then
    printf "Cancelled. No connected coding-agent integrations were removed.\n"
    return 0
  fi
  REPOGRAMMAR_COMMAND_DIR="$COMMAND_DIR" "$REPOGRAMMAR_BIN" uninstall \
    --target "$target" \
    --scope global \
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
  if command -v repogrammar >/dev/null 2>&1; then
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
  printf "This script installs or uninstalls the local RepoGrammar command and\n"
  printf "machine-level Codex / Claude Code MCP integrations.\n\n"
  printf "It does not index this repository.\n"
  printf "It does not create or modify .repogrammar/.\n"
  printf "It does not edit instruction files.\n"
  printf "Telemetry remains controlled by repogrammar install prompts and flags.\n\n"
  printf "Command directory: %s\n\n" "$COMMAND_DIR"
  printf "Choose an action:\n"
  printf "  1 = install or repair repogrammar and configure coding agents\n"
  printf "  2 = uninstall connected coding-agent integrations\n"
  printf "  3 = uninstall repogrammar command only\n"
  printf "  4 = uninstall connected agents and repogrammar command\n"
  printf "  q = cancel\n\n"
  printf "Selection [1]: "
}

main() {
  local choice
  main_menu
  IFS= read -r choice || exit 1
  case "${choice:-1}" in
    1) run_repogrammar_install ;;
    2) uninstall_connected_agents ;;
    3) uninstall_command ;;
    4) uninstall_connected_agents; uninstall_command ;;
    q|Q) printf "Cancelled. No changes made.\n" ;;
    *) printf "Invalid selection.\n" >&2; exit 2 ;;
  esac
}

main "$@"
