#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  printf '[dhtgbot] install-systemd.sh only supports Linux.\n' >&2
  exit 1
fi

SCRIPT_PATH="${BASH_SOURCE[0]:-}"
SCRIPT_DIR="$(cd -- "$(dirname -- "${SCRIPT_PATH}")" && pwd)"
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
  args=(--source "${INSTALL_SOURCE}" --version "${INSTALL_VERSION}" --target "${INSTALL_TARGET}" --layout runtime --no-enter-shell)

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
    run_with_sudo -u "${SERVICE_USER}" -H bash "${INSTALL_SCRIPT}" "${args[@]}"
  else
    bash "${INSTALL_SCRIPT}" "${args[@]}"
  fi
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

if [[ ! -f "${INSTALL_SCRIPT}" ]]; then
  download_install_script
fi

SERVICE_HOME="$(home_for_user "${SERVICE_USER}")"
if [[ -z "${SERVICE_HOME}" ]]; then
  printf '[dhtgbot] could not resolve home directory for user %s.\n' "${SERVICE_USER}" >&2
  exit 1
fi

if [[ -z "${INSTALL_DIR}" ]]; then
  INSTALL_DIR="${SERVICE_HOME}/.local/bin"
fi

if [[ -z "${APP_HOME}" ]]; then
  APP_HOME="${SERVICE_HOME}/.local/share/dhtgbot"
fi

run_install_as_service_user

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
WorkingDirectory=${APP_HOME}
Environment=DHTGBOT_HOME=${APP_HOME}
ExecStart=${APP_HOME}/bin/dhtgbot-real
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
