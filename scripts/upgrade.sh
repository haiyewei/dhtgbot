#!/usr/bin/env bash
set -euo pipefail

SCRIPT_PATH="${BASH_SOURCE[0]:-}"
SCRIPT_DIR=""
if [[ -n "${SCRIPT_PATH}" ]]; then
  SCRIPT_DIR="$(cd -- "$(dirname -- "${SCRIPT_PATH}")" && pwd)"
fi

REPO_OWNER="${DHTGBOT_REMOTE_REPO_OWNER:-haiyewei}"
REPO_NAME="${DHTGBOT_REMOTE_REPO_NAME:-dhtgbot}"
RAW_BRANCH="${DHTGBOT_INSTALL_SCRIPT_BRANCH:-master}"
INSTALL_SCRIPT_URL="${DHTGBOT_INSTALL_SCRIPT_URL:-https://raw.githubusercontent.com/${REPO_OWNER}/${REPO_NAME}/${RAW_BRANCH}/scripts/install.sh}"
INSTALL_SCRIPT=""
TEMP_INSTALL_SCRIPT=""
UPGRADE_LAYOUT=""

UPGRADE_VERSION="${DHTGBOT_INSTALL_VERSION:-latest}"
UPGRADE_TARGET="${DHTGBOT_INSTALL_TARGET:-auto}"
REQUESTED_LAYOUT="${DHTGBOT_INSTALL_LAYOUT:-auto}"
WORKSPACE_DIR="${DHTGBOT_WORKSPACE_DIR:-}"
APP_HOME="${DHTGBOT_HOME:-}"
INSTALL_DIR="${DHTGBOT_INSTALL_DIR:-}"
SKIP_DEPS=0
USE_PROXY=0

if [[ -n "${SCRIPT_DIR}" ]]; then
  INSTALL_SCRIPT="${SCRIPT_DIR}/install.sh"
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      UPGRADE_VERSION="$2"
      shift 2
      ;;
    --target)
      UPGRADE_TARGET="$2"
      shift 2
      ;;
    --layout)
      REQUESTED_LAYOUT="$2"
      shift 2
      ;;
    --workspace-dir)
      WORKSPACE_DIR="$2"
      shift 2
      ;;
    --home-dir)
      APP_HOME="$2"
      shift 2
      ;;
    --install-dir)
      INSTALL_DIR="$2"
      shift 2
      ;;
    --skip-dependencies)
      SKIP_DEPS=1
      shift
      ;;
    --proxy)
      USE_PROXY=1
      shift
      ;;
    *)
      printf '[dhtgbot] unknown flag: %s\n' "$1" >&2
      exit 1
      ;;
  esac
done

is_runtime_root() {
  local root="$1"
  [[ -n "${root}" ]] \
    && [[ -d "${root}" ]] \
    && [[ -f "${root}/config.example.yaml" ]] \
    && [[ -f "${root}/bin/dhtgbot-real" ]]
}

is_workspace_root() {
  local root="$1"

  if [[ -z "${root}" || ! -d "${root}" || ! -f "${root}/config.example.yaml" ]]; then
    return 1
  fi

  [[ -f "${root}/dhtgbot" || -f "${root}/Cargo.toml" || -f "${root}/target/release/dhtgbot" || -f "${root}/target/debug/dhtgbot" ]]
}

add_candidate_root() {
  local candidate="$1"
  if [[ -z "${candidate}" || ! -d "${candidate}" ]]; then
    return 0
  fi

  local existing
  for existing in "${CANDIDATE_ROOTS[@]:-}"; do
    if [[ "${existing}" == "${candidate}" ]]; then
      return 0
    fi
  done

  CANDIDATE_ROOTS+=("${candidate}")
}

resolve_home_from_command_path() {
  local command_path="$1"
  local line

  if [[ -z "${command_path}" ]]; then
    return 1
  fi

  if command -v realpath >/dev/null 2>&1; then
    command_path="$(realpath "${command_path}" 2>/dev/null || printf '%s' "${command_path}")"
  elif command -v readlink >/dev/null 2>&1; then
    command_path="$(readlink -f "${command_path}" 2>/dev/null || printf '%s' "${command_path}")"
  fi

  if [[ ! -e "${command_path}" ]]; then
    return 1
  fi

  if [[ -f "${command_path}" ]]; then
    while IFS= read -r line || [[ -n "${line}" ]]; do
      line="${line%$'\r'}"
      if [[ "${line}" =~ ^APP_HOME=\'([^\']+)\'$ ]]; then
        printf '%s\n' "${BASH_REMATCH[1]}"
        return 0
      fi
    done < "${command_path}"
  fi

  dirname -- "${command_path}"
}

resolve_install_layout() {
  local parent_root=""
  local candidate=""
  local command_root=""
  CANDIDATE_ROOTS=()

  add_candidate_root "${WORKSPACE_DIR}"
  add_candidate_root "${APP_HOME}"
  if [[ -n "${SCRIPT_DIR}" && "$(basename -- "${SCRIPT_DIR}")" == "scripts" ]]; then
    parent_root="$(cd -- "${SCRIPT_DIR}/.." 2>/dev/null && pwd || true)"
    add_candidate_root "${parent_root}"
  fi
  add_candidate_root "${PWD}"
  add_candidate_root "${HOME}/.local/share/dhtgbot"
  command_root="$(resolve_home_from_command_path "$(command -v dhtgbot 2>/dev/null || true)" || true)"
  add_candidate_root "${command_root}"

  case "${REQUESTED_LAYOUT}" in
    auto)
      for candidate in "${CANDIDATE_ROOTS[@]}"; do
        if is_runtime_root "${candidate}"; then
          APP_HOME="${candidate}"
          UPGRADE_LAYOUT="runtime"
          return 0
        fi
        if is_workspace_root "${candidate}"; then
          WORKSPACE_DIR="${candidate}"
          UPGRADE_LAYOUT="workspace"
          return 0
        fi
      done
      ;;
    runtime)
      for candidate in "${CANDIDATE_ROOTS[@]}"; do
        if is_runtime_root "${candidate}"; then
          APP_HOME="${candidate}"
          UPGRADE_LAYOUT="runtime"
          return 0
        fi
      done
      ;;
    workspace)
      if [[ -n "${WORKSPACE_DIR}" ]] && is_workspace_root "${WORKSPACE_DIR}"; then
        UPGRADE_LAYOUT="workspace"
        return 0
      fi
      for candidate in "${CANDIDATE_ROOTS[@]}"; do
        if is_workspace_root "${candidate}"; then
          WORKSPACE_DIR="${candidate}"
          UPGRADE_LAYOUT="workspace"
          return 0
        fi
      done
      ;;
    *)
      printf '[dhtgbot] unsupported upgrade layout: %s\n' "${REQUESTED_LAYOUT}" >&2
      exit 1
      ;;
  esac

  printf '[dhtgbot] no existing installation was detected automatically.\n' >&2
  printf '[dhtgbot] run this script from an existing workspace/app home, or pass --workspace-dir / --home-dir.\n' >&2
  exit 1
}

download_install_script() {
  local tmp_file
  local retry_count="${DHTGBOT_DOWNLOAD_RETRIES:-5}"

  tmp_file="$(mktemp "${TMPDIR:-/tmp}/dhtgbot-install.XXXXXX.sh")"
  if command -v curl >/dev/null 2>&1; then
    curl -fL --retry "${retry_count}" --retry-delay 2 --retry-all-errors "${INSTALL_SCRIPT_URL}" -o "${tmp_file}"
  elif command -v wget >/dev/null 2>&1; then
    wget -q --tries="${retry_count}" -O "${tmp_file}" "${INSTALL_SCRIPT_URL}"
  else
    printf '[dhtgbot] curl or wget is required to download install.sh.\n' >&2
    exit 1
  fi

  chmod +x "${tmp_file}"
  TEMP_INSTALL_SCRIPT="${tmp_file}"
  INSTALL_SCRIPT="${tmp_file}"
}

cleanup_temp_install_script() {
  if [[ -n "${TEMP_INSTALL_SCRIPT}" && -f "${TEMP_INSTALL_SCRIPT}" ]]; then
    rm -f "${TEMP_INSTALL_SCRIPT}" || true
  fi
}

trap cleanup_temp_install_script EXIT

run_upgrade() {
  local layout="$1"
  local -a args
  local previous_overwrite=""
  local had_overwrite=0

  args=(--source remote --version "${UPGRADE_VERSION}" --target "${UPGRADE_TARGET}" --no-enter-shell)

  if [[ "${layout}" == "runtime" ]]; then
    if [[ -z "${INSTALL_DIR}" ]]; then
      INSTALL_DIR="${APP_HOME}"
    fi
    args+=(--layout runtime --home-dir "${APP_HOME}" --install-dir "${INSTALL_DIR}")
    printf '[dhtgbot] upgrading runtime layout in %s\n' "${APP_HOME}"
  else
    args+=(--layout workspace --workspace-dir "${WORKSPACE_DIR}")
    printf '[dhtgbot] upgrading workspace layout in %s\n' "${WORKSPACE_DIR}"
  fi

  if [[ "${SKIP_DEPS}" -eq 1 ]]; then
    args+=(--skip-dependencies)
    printf '[dhtgbot] upgrading only dhtgbot (dependency upgrades skipped)\n'
  else
    printf '[dhtgbot] upgrading binaries: dhtgbot, amagi, tdlr, aria2\n'
  fi

  if [[ "${USE_PROXY}" -eq 1 ]]; then
    args+=(--proxy)
  fi

  if [[ -n "${DHTGBOT_INSTALL_OVERWRITE+x}" ]]; then
    previous_overwrite="${DHTGBOT_INSTALL_OVERWRITE}"
    had_overwrite=1
  fi
  export DHTGBOT_INSTALL_OVERWRITE=always

  bash "${INSTALL_SCRIPT}" "${args[@]}"

  if [[ "${had_overwrite}" -eq 1 ]]; then
    export DHTGBOT_INSTALL_OVERWRITE="${previous_overwrite}"
  else
    unset DHTGBOT_INSTALL_OVERWRITE
  fi
}

if [[ -z "${INSTALL_SCRIPT}" || ! -f "${INSTALL_SCRIPT}" ]]; then
  download_install_script
fi

resolve_install_layout
run_upgrade "${UPGRADE_LAYOUT}"
