#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  printf '[dhtgbot] install-systemd.sh only supports Linux.\n' >&2
  exit 1
fi

SCRIPT_PATH="${BASH_SOURCE[0]:-}"
SCRIPT_DIR="$(cd -- "$(dirname -- "${SCRIPT_PATH}")" && pwd)"
BIN_NAME="dhtgbot"
LOCAL_WORKSPACE_ROOT=""
SERVICE_WORKING_DIR=""
SERVICE_EXEC_START=""
REPO_OWNER="${DHTGBOT_REMOTE_REPO_OWNER:-haiyewei}"
REPO_NAME="${DHTGBOT_REMOTE_REPO_NAME:-dhtgbot}"
RAW_BRANCH="${DHTGBOT_INSTALL_SCRIPT_BRANCH:-master}"
INSTALL_SCRIPT_URL="${DHTGBOT_INSTALL_SCRIPT_URL:-https://raw.githubusercontent.com/${REPO_OWNER}/${REPO_NAME}/${RAW_BRANCH}/scripts/install.sh}"
INSTALL_SCRIPT="${SCRIPT_DIR}/install.sh"
TEMP_INSTALL_SCRIPT=""

SERVICE_NAME="${DHTGBOT_SYSTEMD_SERVICE_NAME:-dhtgbot}"
SERVICE_USER="${DHTGBOT_SERVICE_USER:-${SUDO_USER:-${USER}}}"
INSTALL_SOURCE="${DHTGBOT_INSTALL_SOURCE:-auto}"
INSTALL_VERSION="${DHTGBOT_INSTALL_VERSION:-latest}"
INSTALL_TARGET="${DHTGBOT_INSTALL_TARGET:-auto}"
INSTALL_DIR="${DHTGBOT_INSTALL_DIR:-}"
APP_HOME="${DHTGBOT_HOME:-}"
AMAGI_INSTALL_DIR="${AMAGI_INSTALL_DIR:-}"
TDLR_INSTALL_DIR="${TDLR_INSTALL_DIR:-}"
ARIA2_INSTALL_DIR="${ARIA2_INSTALL_DIR:-}"
SKIP_DEPS=0
USE_PROXY=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --service-name)
      SERVICE_NAME="$2"
      shift 2
      ;;
    --service-user)
      SERVICE_USER="$2"
      shift 2
      ;;
    --source)
      INSTALL_SOURCE="$2"
      shift 2
      ;;
    --version)
      INSTALL_VERSION="$2"
      shift 2
      ;;
    --target)
      INSTALL_TARGET="$2"
      shift 2
      ;;
    --install-dir)
      INSTALL_DIR="$2"
      shift 2
      ;;
    --home-dir)
      APP_HOME="$2"
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

home_for_user() {
  local user_name="$1"
  getent passwd "${user_name}" | cut -d: -f6
}

normalize_install_source() {
  case "${INSTALL_SOURCE}" in
    auto|local|remote)
      printf '%s\n' "${INSTALL_SOURCE}"
      ;;
    *)
      printf '[dhtgbot] unsupported install source mode: %s\n' "${INSTALL_SOURCE}" >&2
      exit 1
      ;;
  esac
}

resolve_local_workspace_root() {
  local candidate_root

  candidate_root="$(cd -- "${SCRIPT_DIR}/.." 2>/dev/null && pwd || true)"
  if [[ -z "${candidate_root}" || ! -d "${candidate_root}" ]]; then
    return 1
  fi

  if [[ ! -f "${candidate_root}/config.example.yaml" ]]; then
    return 1
  fi

  if [[ -f "${candidate_root}/${BIN_NAME}" || -f "${candidate_root}/target/release/${BIN_NAME}" || -f "${candidate_root}/target/debug/${BIN_NAME}" ]]; then
    printf '%s\n' "${candidate_root}"
    return 0
  fi

  return 1
}

resolve_local_workspace_binary() {
  local workspace_root="$1"
  local candidate
  local -a candidates=(
    "${workspace_root}/${BIN_NAME}"
    "${workspace_root}/target/release/${BIN_NAME}"
    "${workspace_root}/target/debug/${BIN_NAME}"
  )

  for candidate in "${candidates[@]}"; do
    if [[ -f "${candidate}" ]]; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  done

  return 1
}

resolve_systemd_install_mode() {
  local normalized_source

  normalized_source="$(normalize_install_source)"
  case "${normalized_source}" in
    local)
      if [[ -n "${LOCAL_WORKSPACE_ROOT}" ]]; then
        printf 'local\n'
        return 0
      fi

      printf '[dhtgbot] local systemd mode requested but no existing workspace was detected next to %s.\n' "${SCRIPT_DIR}" >&2
      exit 1
      ;;
    remote)
      printf 'remote\n'
      ;;
    auto)
      if [[ -n "${LOCAL_WORKSPACE_ROOT}" ]]; then
        printf 'local\n'
      else
        printf 'remote\n'
      fi
      ;;
  esac
}

run_with_sudo() {
  if [[ "$(id -u)" -eq 0 ]]; then
    "$@"
    return 0
  fi

  if command -v sudo >/dev/null 2>&1; then
    sudo "$@"
    return 0
  fi

  printf '[dhtgbot] sudo is required for systemd installation.\n' >&2
  exit 1
}

download_install_script() {
  local tmp_file

  tmp_file="$(mktemp "${TMPDIR:-/tmp}/dhtgbot-install.XXXXXX.sh")"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "${INSTALL_SCRIPT_URL}" -o "${tmp_file}"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "${tmp_file}" "${INSTALL_SCRIPT_URL}"
  else
    printf '[dhtgbot] curl or wget is required to download install.sh.\n' >&2
    exit 1
  fi

  chmod +x "${tmp_file}"
  TEMP_INSTALL_SCRIPT="${tmp_file}"
  INSTALL_SCRIPT="${tmp_file}"
}

run_install_as_service_user() {
  local -a args
  local -a env_args
  args=(--source "${INSTALL_SOURCE}" --version "${INSTALL_VERSION}" --target "${INSTALL_TARGET}" --layout runtime --no-enter-shell)
  env_args=(
    "AMAGI_INSTALL_DIR=${AMAGI_INSTALL_DIR}"
    "TDLR_INSTALL_DIR=${TDLR_INSTALL_DIR}"
    "ARIA2_INSTALL_DIR=${ARIA2_INSTALL_DIR}"
  )

  if [[ -n "${INSTALL_DIR}" ]]; then
    args+=(--install-dir "${INSTALL_DIR}")
  fi

  if [[ -n "${APP_HOME}" ]]; then
    args+=(--home-dir "${APP_HOME}")
  fi

  if [[ "${SKIP_DEPS}" -eq 1 ]]; then
    args+=(--skip-dependencies)
  fi

  if [[ "${USE_PROXY}" -eq 1 ]]; then
    args+=(--proxy)
  fi

  if [[ "$(id -u)" -eq 0 && "${SERVICE_USER}" != "root" ]]; then
    run_with_sudo -u "${SERVICE_USER}" -H env "${env_args[@]}" bash "${INSTALL_SCRIPT}" "${args[@]}"
  else
    env "${env_args[@]}" bash "${INSTALL_SCRIPT}" "${args[@]}"
  fi
}

add_unique_service_path() {
  local value="$1"
  local existing

  if [[ -z "${value}" ]]; then
    return 0
  fi

  for existing in "${SERVICE_PATH_ENTRIES[@]:-}"; do
    if [[ "${existing}" == "${value}" ]]; then
      return 0
    fi
  done

  SERVICE_PATH_ENTRIES+=("${value}")
}

build_service_path() {
  SERVICE_PATH_ENTRIES=()
  add_unique_service_path "${AMAGI_INSTALL_DIR}"
  add_unique_service_path "${TDLR_INSTALL_DIR}"
  add_unique_service_path "${ARIA2_INSTALL_DIR}"
  add_unique_service_path "/usr/local/sbin"
  add_unique_service_path "/usr/local/bin"
  add_unique_service_path "/usr/sbin"
  add_unique_service_path "/usr/bin"
  add_unique_service_path "/bin"

  local joined=""
  local entry
  for entry in "${SERVICE_PATH_ENTRIES[@]:-}"; do
    if [[ -z "${joined}" ]]; then
      joined="${entry}"
    else
      joined="${joined}:${entry}"
    fi
  done

  printf '%s\n' "${joined}"
}

cleanup_temp_install_script() {
  if [[ -n "${TEMP_INSTALL_SCRIPT}" && -f "${TEMP_INSTALL_SCRIPT}" ]]; then
    rm -f "${TEMP_INSTALL_SCRIPT}" || true
  fi
}

trap cleanup_temp_install_script EXIT

if [[ -z "${SERVICE_USER}" ]]; then
  printf '[dhtgbot] service user is empty.\n' >&2
  exit 1
fi

LOCAL_WORKSPACE_ROOT="$(resolve_local_workspace_root || true)"
INSTALL_MODE="$(resolve_systemd_install_mode)"

SERVICE_HOME="$(home_for_user "${SERVICE_USER}")"
if [[ -z "${SERVICE_HOME}" ]]; then
  printf '[dhtgbot] could not resolve home directory for user %s.\n' "${SERVICE_USER}" >&2
  exit 1
fi

if [[ -z "${AMAGI_INSTALL_DIR}" ]]; then
  AMAGI_INSTALL_DIR="${SERVICE_HOME}/.local/bin"
fi

if [[ -z "${TDLR_INSTALL_DIR}" ]]; then
  TDLR_INSTALL_DIR="${SERVICE_HOME}/.local/bin"
fi

if [[ -z "${ARIA2_INSTALL_DIR}" ]]; then
  ARIA2_INSTALL_DIR="${SERVICE_HOME}/.local/bin"
fi

SERVICE_PATH_VALUE="$(build_service_path)"

if [[ "${INSTALL_MODE}" == "local" ]]; then
  if [[ -z "${APP_HOME}" ]]; then
    APP_HOME="${LOCAL_WORKSPACE_ROOT}"
  fi

  SERVICE_WORKING_DIR="${APP_HOME}"
  SERVICE_EXEC_START="$(resolve_local_workspace_binary "${LOCAL_WORKSPACE_ROOT}" || true)"
  if [[ -z "${SERVICE_EXEC_START}" ]]; then
    printf '[dhtgbot] no local %s binary was found in %s.\n' "${BIN_NAME}" "${LOCAL_WORKSPACE_ROOT}" >&2
    exit 1
  fi

  printf '[dhtgbot] detected local workspace at %s\n' "${LOCAL_WORKSPACE_ROOT}"
  printf '[dhtgbot] systemd will reuse the existing workspace and binary\n'
else
  if [[ ! -f "${INSTALL_SCRIPT}" ]]; then
    download_install_script
  fi

  if [[ -z "${INSTALL_DIR}" ]]; then
    INSTALL_DIR="${SERVICE_HOME}/.local/bin"
  fi

  if [[ -z "${APP_HOME}" ]]; then
    APP_HOME="${SERVICE_HOME}/.local/share/dhtgbot"
  fi

  run_install_as_service_user
  SERVICE_WORKING_DIR="${APP_HOME}"
  SERVICE_EXEC_START="${APP_HOME}/bin/dhtgbot-real"
fi

SERVICE_PATH="/etc/systemd/system/${SERVICE_NAME}.service"
TMP_SERVICE_FILE="$(mktemp)"
cat > "${TMP_SERVICE_FILE}" <<EOF
[Unit]
Description=dhtgbot
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=${SERVICE_USER}
WorkingDirectory=${SERVICE_WORKING_DIR}
Environment=HOME=${SERVICE_HOME}
Environment=DHTGBOT_HOME=${SERVICE_WORKING_DIR}
Environment=PATH=${SERVICE_PATH_VALUE}
ExecStart=${SERVICE_EXEC_START}
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

run_with_sudo cp "${TMP_SERVICE_FILE}" "${SERVICE_PATH}"
rm -f "${TMP_SERVICE_FILE}"

run_with_sudo systemctl daemon-reload
run_with_sudo systemctl enable --now "${SERVICE_NAME}.service"

printf '[dhtgbot] installed systemd service %s\n' "${SERVICE_NAME}.service"
printf '[dhtgbot] status: sudo systemctl status %s.service\n' "${SERVICE_NAME}"
