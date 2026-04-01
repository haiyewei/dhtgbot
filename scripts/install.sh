#!/usr/bin/env bash
set -euo pipefail

BIN_NAME="dhtgbot"
INSTALLED_BIN_NAME="dhtgbot-real"
DEFAULT_SOURCE_MODE="${DHTGBOT_INSTALL_SOURCE:-auto}"
INSTALL_LAYOUT="${DHTGBOT_INSTALL_LAYOUT:-workspace}"
REMOTE_REPO_OWNER="${DHTGBOT_REMOTE_REPO_OWNER:-haiyewei}"
REMOTE_REPO_NAME="${DHTGBOT_REMOTE_REPO_NAME:-dhtgbot}"
REMOTE_VERSION="${DHTGBOT_INSTALL_VERSION:-latest}"
REMOTE_TARGET="${DHTGBOT_INSTALL_TARGET:-auto}"
REMOTE_BASE_URL="${DHTGBOT_REMOTE_BASE_URL:-}"
INSTALL_DIR="${DHTGBOT_INSTALL_DIR:-}"
APP_HOME="${DHTGBOT_HOME:-}"
WORKSPACE_DIR="${DHTGBOT_WORKSPACE_DIR:-}"
ENTER_SHELL_MODE="${DHTGBOT_INSTALL_ENTER:-auto}"
SKIP_DEPS=0
PROXY_PREFIX=""

AMAGI_REPO_OWNER="${AMAGI_REMOTE_REPO_OWNER:-bandange}"
AMAGI_REPO_NAME="${AMAGI_REMOTE_REPO_NAME:-amagi-rs}"
AMAGI_VERSION="${AMAGI_INSTALL_VERSION:-latest}"
AMAGI_BASE_URL="${AMAGI_REMOTE_BASE_URL:-}"
AMAGI_INSTALL_DIR="${AMAGI_INSTALL_DIR:-${HOME}/.local/bin}"

TDLR_REPO_OWNER="${TDLR_REMOTE_REPO_OWNER:-haiyewei}"
TDLR_REPO_NAME="${TDLR_REMOTE_REPO_NAME:-tdlr}"
TDLR_VERSION="${TDLR_INSTALL_VERSION:-latest}"
TDLR_BASE_URL="${TDLR_REMOTE_BASE_URL:-}"
TDLR_INSTALL_DIR="${TDLR_INSTALL_DIR:-${HOME}/.local/bin}"

ARIA2_REPO_OWNER="${ARIA2_REMOTE_REPO_OWNER:-}"
ARIA2_REPO_NAME="${ARIA2_REMOTE_REPO_NAME:-}"
ARIA2_VERSION="${ARIA2_INSTALL_VERSION:-1.37.0}"
ARIA2_TAG="${ARIA2_INSTALL_TAG:-${ARIA2_VERSION}}"
ARIA2_ASSET_NAME="${ARIA2_REMOTE_ASSET_NAME:-}"
ARIA2_ARCHIVE_KIND="${ARIA2_REMOTE_ARCHIVE_KIND:-zip}"
ARIA2_BASE_URL="${ARIA2_REMOTE_BASE_URL:-}"
ARIA2_INSTALL_DIR="${ARIA2_INSTALL_DIR:-${HOME}/.local/bin}"
OVERWRITE_POLICY="${DHTGBOT_INSTALL_OVERWRITE:-prompt}"
LAST_INSTALL_ACTION=""

SCRIPT_PATH="${BASH_SOURCE[0]:-}"
SCRIPT_DIR=""
REPO_ROOT=""
TEMP_PATHS=()

if [[ -n "${SCRIPT_PATH}" ]]; then
  SCRIPT_DIR="$(cd -- "$(dirname -- "${SCRIPT_PATH}")" && pwd)"
  REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." 2>/dev/null && pwd || true)"
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      DEFAULT_SOURCE_MODE="$2"
      shift 2
      ;;
    --layout)
      INSTALL_LAYOUT="$2"
      shift 2
      ;;
    --version)
      REMOTE_VERSION="$2"
      shift 2
      ;;
    --target)
      REMOTE_TARGET="$2"
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
    --workspace-dir)
      WORKSPACE_DIR="$2"
      shift 2
      ;;
    --skip-dependencies)
      SKIP_DEPS=1
      shift
      ;;
    --no-enter-shell)
      ENTER_SHELL_MODE="never"
      shift
      ;;
    --proxy)
      PROXY_PREFIX="https://mirror.ghproxy.com/"
      shift
      ;;
    *)
      printf '[dhtgbot] unknown flag: %s\n' "$1" >&2
      exit 1
      ;;
  esac
done

default_app_home() {
  printf '%s\n' "${HOME}/.local/share/dhtgbot"
}

default_workspace_dir() {
  printf '%s\n' "$(pwd)/${REMOTE_REPO_NAME}"
}

resolve_profile_file() {
  if [[ -n "${DHTGBOT_PROFILE_FILE:-}" ]]; then
    printf '%s\n' "${DHTGBOT_PROFILE_FILE}"
    return 0
  fi

  case "${SHELL##*/}" in
    bash)
      printf '%s\n' "${HOME}/.bashrc"
      ;;
    zsh)
      printf '%s\n' "${HOME}/.zshrc"
      ;;
    *)
      printf '%s\n' "${HOME}/.profile"
      ;;
  esac
}

sanitize_path_token() {
  printf '%s' "$1" | sed 's/[^A-Za-z0-9._-]/_/g'
}

should_persist_fish_path() {
  if [[ "${SHELL##*/}" == "fish" ]]; then
    return 0
  fi

  if command -v fish >/dev/null 2>&1; then
    return 0
  fi

  [[ -d "${HOME}/.config/fish" ]]
}

resolve_fish_path_file() {
  local install_dir="$1"
  local token

  token="$(sanitize_path_token "${install_dir}")"
  printf '%s\n' "${HOME}/.config/fish/conf.d/dhtgbot-path-${token}.fish"
}

absolute_file_path() {
  local path="$1"
  local dir

  dir="$(cd -- "$(dirname -- "${path}")" && pwd)"
  printf '%s/%s\n' "${dir}" "$(basename -- "${path}")"
}

absolute_dir_path() {
  local path="$1"
  (
    cd -- "${path}"
    pwd
  )
}

persist_path_entry() {
  local install_dir="$1"
  local profile_file
  local profile_dir
  local path_line

  if [[ -z "${install_dir}" ]]; then
    return 0
  fi

  profile_file="$(resolve_profile_file)"
  profile_dir="$(dirname -- "${profile_file}")"
  path_line="export PATH=\"${install_dir}:\$PATH\""

  mkdir -p "${profile_dir}"
  touch "${profile_file}"

  if grep -Fqx "${path_line}" "${profile_file}"; then
    printf '[dhtgbot] PATH entry already exists in %s\n' "${profile_file}"
  else
    {
      printf '\n# dhtgbot installer\n'
      printf '%s\n' "${path_line}"
    } >> "${profile_file}"
    printf '[dhtgbot] added install directory to %s\n' "${profile_file}"
  fi

  if [[ ":${PATH}:" != *":${install_dir}:"* ]]; then
    export PATH="${install_dir}:${PATH}"
    printf '[dhtgbot] updated PATH in the current shell session\n'
  fi

  persist_fish_path_entry "${install_dir}"
}

persist_fish_path_entry() {
  local install_dir="$1"
  local fish_path_file
  local fish_dir
  local path_line

  if [[ -z "${install_dir}" ]] || ! should_persist_fish_path; then
    return 0
  fi

  fish_path_file="$(resolve_fish_path_file "${install_dir}")"
  fish_dir="$(dirname -- "${fish_path_file}")"
  path_line="    set -gx PATH \"${install_dir}\" \$PATH"

  mkdir -p "${fish_dir}"

  if [[ -f "${fish_path_file}" ]] && grep -Fqx "${path_line}" "${fish_path_file}"; then
    printf '[dhtgbot] PATH entry already exists in %s\n' "${fish_path_file}"
    return 0
  fi

  {
    printf '# dhtgbot installer\n'
    printf 'if not contains -- "%s" $PATH\n' "${install_dir}"
    printf '    set -gx PATH "%s" $PATH\n' "${install_dir}"
    printf 'end\n'
  } > "${fish_path_file}"
  printf '[dhtgbot] added install directory to %s\n' "${fish_path_file}"
}

add_unique_path_entry() {
  local value="$1"
  local existing

  if [[ -z "${value}" ]]; then
    return 0
  fi

  for existing in "${PATH_ENTRIES_TO_PERSIST[@]:-}"; do
    if [[ "${existing}" == "${value}" ]]; then
      return 0
    fi
  done

  PATH_ENTRIES_TO_PERSIST+=("${value}")
}

persist_dependency_path_entries() {
  PATH_ENTRIES_TO_PERSIST=()
  add_unique_path_entry "${AMAGI_INSTALL_DIR}"
  add_unique_path_entry "${TDLR_INSTALL_DIR}"
  if [[ "$(uname -s)" != "Darwin" ]]; then
    add_unique_path_entry "${ARIA2_INSTALL_DIR}"
  fi

  local entry
  for entry in "${PATH_ENTRIES_TO_PERSIST[@]:-}"; do
    persist_path_entry "${entry}"
  done
}

normalize_overwrite_policy() {
  case "${OVERWRITE_POLICY}" in
    prompt|always|never)
      printf '%s\n' "${OVERWRITE_POLICY}"
      ;;
    *)
      printf '[dhtgbot] unsupported overwrite policy: %s\n' "${OVERWRITE_POLICY}" >&2
      exit 1
      ;;
  esac
}

confirm_overwrite() {
  local display_name="$1"
  local existing_path="$2"
  local target_path="$3"
  local policy
  local answer

  policy="$(normalize_overwrite_policy)"

  case "${policy}" in
    always)
      printf '[dhtgbot] overwrite policy is always; replacing existing %s\n' "${display_name}"
      return 0
      ;;
    never)
      printf '[dhtgbot] overwrite policy is never; keeping existing %s at %s\n' "${display_name}" "${existing_path}"
      printf '[dhtgbot] use DHTGBOT_INSTALL_OVERWRITE=always or scripts/upgrade.sh if you want to replace it\n'
      return 1
      ;;
    prompt)
      if [[ ! -t 0 ]]; then
        printf '[dhtgbot] existing %s detected at %s; non-interactive mode defaults to skip overwrite\n' "${display_name}" "${existing_path}" >&2
        printf '[dhtgbot] use DHTGBOT_INSTALL_OVERWRITE=always or scripts/upgrade.sh if you want to replace it\n' >&2
        return 1
      fi

      printf '[dhtgbot] existing %s detected\n' "${display_name}"
      printf '  current: %s\n' "${existing_path}"
      printf '  target : %s\n' "${target_path}"
      printf 'Overwrite existing %s? [y/N]: ' "${display_name}"
      read -r answer
      case "${answer}" in
        y|Y|yes|YES)
          return 0
          ;;
        *)
          return 1
          ;;
      esac
      ;;
  esac
}

register_temp_path() {
  TEMP_PATHS+=("$1")
}

cleanup_temp_paths() {
  local path
  for path in "${TEMP_PATHS[@]:-}"; do
    if [[ -z "${path}" ]]; then
      continue
    fi
    rm -rf "${path}" 2>/dev/null || true
  done
}

trap cleanup_temp_paths EXIT

normalize_install_layout() {
  case "$1" in
    workspace|runtime)
      printf '%s\n' "$1"
      ;;
    *)
      printf '[dhtgbot] unsupported install layout: %s\n' "$1" >&2
      exit 1
      ;;
  esac
}

normalize_remote_target() {
  case "$1" in
    auto|x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|x86_64-unknown-linux-musl|aarch64-unknown-linux-musl|x86_64-apple-darwin|aarch64-apple-darwin)
      printf '%s\n' "$1"
      ;;
    *)
      printf '[dhtgbot] unsupported remote target selector: %s\n' "$1" >&2
      exit 1
      ;;
  esac
}

detect_linux_libc() {
  if command -v ldd >/dev/null 2>&1; then
    local ldd_output
    ldd_output="$(ldd --version 2>&1 || true)"

    if printf '%s' "${ldd_output}" | grep -qi 'musl'; then
      printf 'musl\n'
      return 0
    fi

    if printf '%s' "${ldd_output}" | grep -qiE 'glibc|gnu libc'; then
      printf 'gnu\n'
      return 0
    fi
  fi

  if command -v getconf >/dev/null 2>&1 && getconf GNU_LIBC_VERSION >/dev/null 2>&1; then
    printf 'gnu\n'
    return 0
  fi

  printf 'gnu\n'
}

resolve_remote_target() {
  local requested_target
  requested_target="$(normalize_remote_target "${REMOTE_TARGET}")"

  if [[ "${requested_target}" != "auto" ]]; then
    printf '%s\n' "${requested_target}"
    return 0
  fi

  case "$(uname -s)" in
    Linux)
      local libc
      libc="$(detect_linux_libc)"
      case "$(uname -m)" in
        x86_64|amd64)
          printf 'x86_64-unknown-linux-%s\n' "${libc}"
          ;;
        aarch64|arm64)
          printf 'aarch64-unknown-linux-%s\n' "${libc}"
          ;;
        *)
          printf '[dhtgbot] unsupported architecture for remote install: %s\n' "$(uname -m)" >&2
          exit 1
          ;;
      esac
      ;;
    Darwin)
      case "$(uname -m)" in
        x86_64|amd64)
          printf 'x86_64-apple-darwin\n'
          ;;
        aarch64|arm64)
          printf 'aarch64-apple-darwin\n'
          ;;
        *)
          printf '[dhtgbot] unsupported architecture for remote install: %s\n' "$(uname -m)" >&2
          exit 1
          ;;
      esac
      ;;
    *)
      printf '[dhtgbot] unsupported operating system for remote install: %s\n' "$(uname -s)" >&2
      exit 1
      ;;
  esac
}

arch_slug() {
  case "$(uname -m)" in
    x86_64|amd64)
      printf 'x86_64\n'
      ;;
    aarch64|arm64)
      printf 'aarch64\n'
      ;;
    *)
      printf '[dhtgbot] unsupported architecture: %s\n' "$(uname -m)" >&2
      exit 1
      ;;
  esac
}

github_release_url() {
  local owner="$1"
  local repo="$2"
  local version="$3"
  local asset_name="$4"
  local base_url="${5:-}"

  if [[ -n "${base_url}" ]]; then
    printf '%s/%s\n' "${base_url%/}" "${asset_name}"
    return 0
  fi

  if [[ "${version}" == "latest" ]]; then
    printf '%shttps://github.com/%s/%s/releases/latest/download/%s\n' \
      "${PROXY_PREFIX}" "${owner}" "${repo}" "${asset_name}"
  else
    printf '%shttps://github.com/%s/%s/releases/download/%s/%s\n' \
      "${PROXY_PREFIX}" "${owner}" "${repo}" "${version}" "${asset_name}"
  fi
}

download_file() {
  local url="$1"
  local output_path="$2"
  local retry_count="${DHTGBOT_DOWNLOAD_RETRIES:-5}"

  printf '[dhtgbot] downloading %s\n' "${url}" >&2

  rm -f "${output_path}"

  if command -v curl >/dev/null 2>&1; then
    if ! curl -fL --retry "${retry_count}" --retry-delay 2 --retry-all-errors "${url}" -o "${output_path}"; then
      rm -f "${output_path}"
      printf '[dhtgbot] failed to download %s\n' "${url}" >&2
      return 1
    fi
  elif command -v wget >/dev/null 2>&1; then
    if ! wget -q --tries="${retry_count}" -O "${output_path}" "${url}"; then
      rm -f "${output_path}"
      printf '[dhtgbot] failed to download %s\n' "${url}" >&2
      return 1
    fi
  else
    printf '[dhtgbot] curl or wget is required for remote install.\n' >&2
    exit 1
  fi
}

download_and_extract_archive() {
  local url="$1"
  local asset_name="$2"
  local archive_kind="$3"
  local tmp_dir
  local archive_path

  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/dhtgbot-install.XXXXXX")"
  register_temp_path "${tmp_dir}"
  archive_path="${tmp_dir}/${asset_name}"
  if ! download_file "${url}" "${archive_path}"; then
    return 1
  fi

  case "${archive_kind}" in
    tar.gz)
      if ! tar -xzf "${archive_path}" -C "${tmp_dir}"; then
        rm -f "${archive_path}"
        printf '[dhtgbot] failed to extract %s\n' "${asset_name}" >&2
        return 1
      fi
      ;;
    tar.xz)
      if ! tar -xJf "${archive_path}" -C "${tmp_dir}"; then
        rm -f "${archive_path}"
        printf '[dhtgbot] failed to extract %s\n' "${asset_name}" >&2
        return 1
      fi
      ;;
    zip)
      if command -v unzip >/dev/null 2>&1; then
        if ! unzip -q "${archive_path}" -d "${tmp_dir}"; then
          rm -f "${archive_path}"
          printf '[dhtgbot] failed to extract %s\n' "${asset_name}" >&2
          return 1
        fi
      elif command -v python3 >/dev/null 2>&1; then
        if ! python3 - "${archive_path}" "${tmp_dir}" <<'PY'
import sys
from zipfile import ZipFile

archive_path, target_dir = sys.argv[1], sys.argv[2]
with ZipFile(archive_path) as archive:
    archive.extractall(target_dir)
PY
        then
          rm -f "${archive_path}"
          printf '[dhtgbot] failed to extract %s\n' "${asset_name}" >&2
          return 1
        fi
      else
        rm -f "${archive_path}"
        printf '[dhtgbot] unzip or python3 is required to extract %s\n' "${asset_name}" >&2
        exit 1
      fi
      ;;
    *)
      printf '[dhtgbot] unsupported archive kind: %s\n' "${archive_kind}" >&2
      exit 1
      ;;
  esac

  rm -f "${archive_path}"

  printf '%s\n' "${tmp_dir}"
}

resolve_execution_mode() {
  case "${DEFAULT_SOURCE_MODE}" in
    auto|local|remote)
      ;;
    *)
      printf '[dhtgbot] unsupported install source mode: %s\n' "${DEFAULT_SOURCE_MODE}" >&2
      exit 1
      ;;
  esac

  if [[ "${DEFAULT_SOURCE_MODE}" != "auto" ]]; then
    printf '%s\n' "${DEFAULT_SOURCE_MODE}"
    return 0
  fi

  if [[ -n "${REPO_ROOT}" && -f "${REPO_ROOT}/${BIN_NAME}" ]]; then
    printf 'local\n'
    return 0
  fi

  if [[ -n "${SCRIPT_DIR}" && -f "${SCRIPT_DIR}/${BIN_NAME}" ]]; then
    printf 'local\n'
    return 0
  fi

  if [[ -n "${REPO_ROOT}" && -f "${REPO_ROOT}/target/release/${BIN_NAME}" ]]; then
    printf 'local\n'
    return 0
  fi

  if [[ -n "${REPO_ROOT}" && -f "${REPO_ROOT}/target/debug/${BIN_NAME}" ]]; then
    printf 'local\n'
    return 0
  fi

  printf 'remote\n'
}

resolve_local_binary() {
  local candidates=()

  if [[ -n "${REPO_ROOT}" ]]; then
    candidates+=("${REPO_ROOT}/${BIN_NAME}")
    candidates+=("${REPO_ROOT}/target/release/${BIN_NAME}")
    candidates+=("${REPO_ROOT}/target/debug/${BIN_NAME}")
  fi

  if [[ -n "${SCRIPT_DIR}" ]]; then
    candidates+=("${SCRIPT_DIR}/${BIN_NAME}")
  fi

  local candidate
  for candidate in "${candidates[@]}"; do
    if [[ -f "${candidate}" ]]; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  done

  return 1
}

resolve_local_template() {
  local candidates=()

  if [[ -n "${REPO_ROOT}" ]]; then
    candidates+=("${REPO_ROOT}/config.example.yaml")
  fi

  if [[ -n "${SCRIPT_DIR}" ]]; then
    candidates+=("${SCRIPT_DIR}/config.example.yaml")
  fi

  local candidate
  for candidate in "${candidates[@]}"; do
    if [[ -f "${candidate}" ]]; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  done

  return 1
}

resolve_local_scripts_dir() {
  if [[ -n "${REPO_ROOT}" && -d "${REPO_ROOT}/scripts" ]]; then
    printf '%s\n' "${REPO_ROOT}/scripts"
    return 0
  fi

  return 1
}

dhtgbot_asset_name() {
  printf '%s-%s.tar.gz\n' "${BIN_NAME}" "$(resolve_remote_target)"
}

amagi_asset_name() {
  local arch
  arch="$(arch_slug)"

  case "$(uname -s)" in
    Linux)
      printf 'amagi-%s-unknown-linux-musl.tar.gz\n' "${arch}"
      ;;
    Darwin)
      printf 'amagi-%s-apple-darwin.tar.gz\n' "${arch}"
      ;;
    *)
      printf '[dhtgbot] unsupported platform for amagi install: %s\n' "$(uname -s)" >&2
      exit 1
      ;;
  esac
}

tdlr_asset_name() {
  printf 'tdlr-%s.tar.gz\n' "$(resolve_remote_target)"
}

aria2_asset_name() {
  if [[ -n "${ARIA2_ASSET_NAME}" ]]; then
    printf '%s\n' "${ARIA2_ASSET_NAME}"
    return 0
  fi

  case "$(uname -s)" in
    Linux)
      case "$(arch_slug)" in
        x86_64|aarch64)
          printf 'aria2-%s-linux-musl_static.zip\n' "$(arch_slug)"
          ;;
        *)
          printf '[dhtgbot] unsupported architecture for aria2 install: %s\n' "$(uname -m)" >&2
          exit 1
          ;;
      esac
      ;;
    Darwin)
      printf '[dhtgbot] macOS installs aria2 through Homebrew instead of a GitHub release asset.\n' >&2
      exit 1
      ;;
    *)
      printf '[dhtgbot] unsupported platform for aria2 install: %s\n' "$(uname -s)" >&2
      exit 1
      ;;
  esac
}

aria2_release_owner() {
  if [[ -n "${ARIA2_REPO_OWNER}" ]]; then
    printf '%s\n' "${ARIA2_REPO_OWNER}"
    return 0
  fi

  case "$(uname -s)" in
    Linux)
      printf 'abcfy2\n'
      ;;
    *)
      printf 'aria2\n'
      ;;
  esac
}

aria2_release_repo() {
  if [[ -n "${ARIA2_REPO_NAME}" ]]; then
    printf '%s\n' "${ARIA2_REPO_NAME}"
    return 0
  fi

  case "$(uname -s)" in
    Linux)
      printf 'aria2-static-build\n'
      ;;
    *)
      printf 'aria2\n'
      ;;
  esac
}

install_release_binary_from_archive() {
  local url="$1"
  local asset_name="$2"
  local archive_kind="$3"
  local binary_name="$4"
  local install_dir="$5"
  local display_name="$6"
  local tmp_dir
  local binary_path
  local existing_path

  mkdir -p "${install_dir}"
  existing_path=""
  if [[ -e "${install_dir}/${binary_name}" ]]; then
    existing_path="${install_dir}/${binary_name}"
  fi

  if [[ -n "${existing_path}" ]]; then
    if ! confirm_overwrite "${display_name}" "${existing_path}" "${install_dir}/${binary_name}"; then
      printf '[dhtgbot] kept existing %s\n' "${display_name}"
      LAST_INSTALL_ACTION="skipped"
      return 0
    fi
  fi

  if ! tmp_dir="$(download_and_extract_archive "${url}" "${asset_name}" "${archive_kind}")"; then
    exit 1
  fi
  binary_path="${tmp_dir}/${binary_name}"

  if [[ ! -f "${binary_path}" ]]; then
    printf '[dhtgbot] downloaded package did not contain %s\n' "${binary_name}" >&2
    exit 1
  fi

  cp "${binary_path}" "${install_dir}/${binary_name}"
  chmod 755 "${install_dir}/${binary_name}"
  LAST_INSTALL_ACTION="installed"
}

install_amagi() {
  local asset_name
  local url

  asset_name="$(amagi_asset_name)"
  url="$(github_release_url "${AMAGI_REPO_OWNER}" "${AMAGI_REPO_NAME}" "${AMAGI_VERSION}" "${asset_name}" "${AMAGI_BASE_URL}")"
  install_release_binary_from_archive "${url}" "${asset_name}" "tar.gz" "amagi" "${AMAGI_INSTALL_DIR}" "amagi"
  if [[ "${LAST_INSTALL_ACTION}" == "installed" ]]; then
    printf '[dhtgbot] amagi installed to %s/amagi\n' "${AMAGI_INSTALL_DIR}"
  fi
}

install_tdlr() {
  local asset_name
  local url

  asset_name="$(tdlr_asset_name)"
  url="$(github_release_url "${TDLR_REPO_OWNER}" "${TDLR_REPO_NAME}" "${TDLR_VERSION}" "${asset_name}" "${TDLR_BASE_URL}")"
  install_release_binary_from_archive "${url}" "${asset_name}" "tar.gz" "tdlr" "${TDLR_INSTALL_DIR}" "tdlr"
  if [[ "${LAST_INSTALL_ACTION}" == "installed" ]]; then
    printf '[dhtgbot] tdlr installed to %s/tdlr\n' "${TDLR_INSTALL_DIR}"
  fi
}

install_aria2() {
  local brew_path=""
  local existing_path=""
  local target_path=""
  local outdated_formula=""
  local asset_name
  local url

  if [[ "$(uname -s)" == "Darwin" ]]; then
    brew_path="$(command -v brew 2>/dev/null || true)"
    if [[ -z "${brew_path}" ]]; then
      printf '[dhtgbot] macOS aria2 install requires Homebrew. Install Homebrew first: https://brew.sh/\n' >&2
      exit 1
    fi

    existing_path="$(command -v aria2c 2>/dev/null || true)"
    if "${brew_path}" list --versions aria2 >/dev/null 2>&1; then
      target_path="$("${brew_path}" --prefix aria2)/bin/aria2c"
    else
      target_path="Homebrew formula aria2"
    fi

    if [[ -n "${existing_path}" ]]; then
      if ! confirm_overwrite "aria2" "${existing_path}" "${target_path}"; then
        printf '[dhtgbot] kept existing aria2\n'
        LAST_INSTALL_ACTION="skipped"
        return 0
      fi
    fi

    if "${brew_path}" list --versions aria2 >/dev/null 2>&1; then
      outdated_formula="$("${brew_path}" outdated --formula aria2 2>/dev/null || true)"
      if [[ -n "${outdated_formula}" ]]; then
        "${brew_path}" upgrade aria2
      else
        printf '[dhtgbot] aria2 is already up to date in Homebrew\n'
      fi
    else
      "${brew_path}" install aria2
    fi

    target_path="$("${brew_path}" --prefix aria2)/bin/aria2c"
    if [[ ! -x "${target_path}" ]]; then
      printf '[dhtgbot] Homebrew finished but %s was not found.\n' "${target_path}" >&2
      exit 1
    fi

    LAST_INSTALL_ACTION="installed"
    printf '[dhtgbot] aria2 installed via Homebrew at %s\n' "${target_path}"
    return 0
  fi

  asset_name="$(aria2_asset_name)"
  url="$(github_release_url "$(aria2_release_owner)" "$(aria2_release_repo)" "${ARIA2_TAG}" "${asset_name}" "${ARIA2_BASE_URL}")"
  install_release_binary_from_archive "${url}" "${asset_name}" "${ARIA2_ARCHIVE_KIND}" "aria2c" "${ARIA2_INSTALL_DIR}" "aria2"
  if [[ "${LAST_INSTALL_ACTION}" == "installed" ]]; then
    printf '[dhtgbot] aria2 installed to %s/aria2c\n' "${ARIA2_INSTALL_DIR}"
  fi
}

write_launcher() {
  local launcher_path="$1"
  local app_home="$2"

  cat > "${launcher_path}" <<EOF
#!/usr/bin/env bash
set -euo pipefail
APP_HOME='${app_home}'
cd "\${APP_HOME}"
exec "\${APP_HOME}/bin/${INSTALLED_BIN_NAME}" "\$@"
EOF
  chmod 755 "${launcher_path}"
}

install_support_scripts() {
  local source_scripts_dir="$1"
  local target_root="$2"
  local target_scripts_dir="${target_root}/scripts"

  if [[ -z "${source_scripts_dir}" || ! -d "${source_scripts_dir}" ]]; then
    return 0
  fi

  rm -rf "${target_scripts_dir}"
  mkdir -p "${target_scripts_dir}"
  cp -R "${source_scripts_dir}/." "${target_scripts_dir}/"
  chmod +x "${target_scripts_dir}/"*.sh 2>/dev/null || true
}

install_runtime_layout() {
  local source_binary="$1"
  local source_template="$2"
  local source_scripts_dir="$3"
  local runtime_bin_dir="${APP_HOME}/bin"
  local installed_binary="${runtime_bin_dir}/${INSTALLED_BIN_NAME}"
  local launcher_path="${INSTALL_DIR}/${BIN_NAME}"
  local template_path="${APP_HOME}/config.example.yaml"
  local config_path="${APP_HOME}/config.yaml"

  mkdir -p "${runtime_bin_dir}" "${APP_HOME}/data" "${APP_HOME}/logs" "${INSTALL_DIR}"
  cp "${source_binary}" "${installed_binary}"
  chmod 755 "${installed_binary}"

  if [[ -n "${source_template}" && -f "${source_template}" ]]; then
    cp "${source_template}" "${template_path}"
    if [[ -f "${config_path}" ]]; then
      printf '[dhtgbot] kept existing config at %s\n' "${config_path}"
    else
      printf '[dhtgbot] config file not created automatically; copy %s to %s\n' "${template_path}" "${config_path}"
    fi
  fi

  install_support_scripts "${source_scripts_dir}" "${APP_HOME}"
  write_launcher "${launcher_path}" "${APP_HOME}"
}

copy_file_if_needed() {
  local source_path="$1"
  local target_path="$2"

  if [[ -z "${source_path}" || ! -f "${source_path}" ]]; then
    return 0
  fi

  mkdir -p "$(dirname -- "${target_path}")"
  if [[ -e "${target_path}" && "$(absolute_file_path "${source_path}")" == "$(absolute_file_path "${target_path}")" ]]; then
    return 0
  fi

  cp "${source_path}" "${target_path}"
}

install_workspace_layout() {
  local source_package_dir="$1"
  local source_binary="$2"
  local source_template="$3"
  local source_scripts_dir="$4"
  local workspace_dir="$5"
  local template_path="${workspace_dir}/config.example.yaml"
  local config_path="${workspace_dir}/config.yaml"

  mkdir -p "${workspace_dir}" "${workspace_dir}/data" "${workspace_dir}/data/downloads" "${workspace_dir}/logs"

  if [[ -n "${source_package_dir}" && -d "${source_package_dir}" ]]; then
    if [[ "$(absolute_dir_path "${source_package_dir}")" != "$(absolute_dir_path "${workspace_dir}")" ]]; then
      cp -R "${source_package_dir}/." "${workspace_dir}/"
    fi
  else
    copy_file_if_needed "${source_binary}" "${workspace_dir}/${BIN_NAME}"
    copy_file_if_needed "${source_template}" "${template_path}"
    install_support_scripts "${source_scripts_dir}" "${workspace_dir}"
  fi

  chmod 755 "${workspace_dir}/${BIN_NAME}" 2>/dev/null || true
  chmod +x "${workspace_dir}/scripts/"*.sh 2>/dev/null || true

  if [[ -f "${template_path}" ]]; then
    if [[ -f "${config_path}" ]]; then
      printf '[dhtgbot] kept existing config at %s\n' "${config_path}"
    else
      printf '[dhtgbot] config file not created automatically; copy %s to %s\n' "${template_path}" "${config_path}"
    fi
  fi
}

print_workspace_summary() {
  local workspace_dir="$1"
  local template_path="${workspace_dir}/config.example.yaml"
  local config_path="${workspace_dir}/config.yaml"

  printf '[dhtgbot] extracted project to %s\n' "${workspace_dir}"
  printf '[dhtgbot] copy the example config before the first real run:\n'
  printf '  cp "%s" "%s"\n' "${template_path}" "${config_path}"
  printf '[dhtgbot] then edit %s\n' "${config_path}"
  printf '[dhtgbot] confirm services.amagi.start_command, services.tdlr.start_command, and services.aria2.start_command in config.yaml\n'
  printf '[dhtgbot] if you use X polling, fill bots.xdl.twitter.cookies in config.yaml\n'
  printf '[dhtgbot] dependencies are installed as environment commands:\n'
  printf '  amagi -> %s\n' "${AMAGI_INSTALL_DIR}"
  printf '  tdlr  -> %s\n' "${TDLR_INSTALL_DIR}"
  if [[ "$(uname -s)" == "Darwin" ]]; then
    printf '  aria2 -> Homebrew formula aria2\n'
  else
    printf '  aria2 -> %s\n' "${ARIA2_INSTALL_DIR}"
  fi
  printf '[dhtgbot] if dependency installation fails later, the project directory is already ready; you can still configure it and install missing dependencies manually\n'
  printf '[dhtgbot] run the bot from the project directory:\n'
  printf '  cd "%s"\n' "${workspace_dir}"
  printf '  ./%s\n' "${BIN_NAME}"
}

maybe_enter_workspace_shell() {
  local workspace_dir="$1"

  case "${ENTER_SHELL_MODE}" in
    never)
      return 0
      ;;
    auto)
      if [[ ! -t 1 || ! -t 2 ]]; then
        return 0
      fi
      ;;
    always)
      ;;
    *)
      printf '[dhtgbot] unsupported enter-shell mode: %s\n' "${ENTER_SHELL_MODE}" >&2
      exit 1
      ;;
  esac

  printf '[dhtgbot] opening an interactive shell in %s\n' "${workspace_dir}"
  printf '[dhtgbot] exit this shell when you finish copying and editing config.yaml\n'
  cd "${workspace_dir}"
  exec "${SHELL:-/bin/sh}" -i
}

INSTALL_LAYOUT="$(normalize_install_layout "${INSTALL_LAYOUT}")"

INSTALL_MODE="$(resolve_execution_mode)"
SOURCE_BINARY=""
SOURCE_TEMPLATE=""
SOURCE_SCRIPTS_DIR=""
SOURCE_PACKAGE_DIR=""

if [[ "${INSTALL_MODE}" == "local" ]]; then
  SOURCE_BINARY="$(resolve_local_binary || true)"
  SOURCE_TEMPLATE="$(resolve_local_template || true)"
  SOURCE_SCRIPTS_DIR="$(resolve_local_scripts_dir || true)"

  if [[ -z "${SOURCE_BINARY}" ]]; then
    printf '[dhtgbot] no local prebuilt binary found. Place dhtgbot in the package/root directory, or use --source remote.\n' >&2
    exit 1
  fi

  if [[ -n "${REPO_ROOT}" && ! -f "${REPO_ROOT}/Cargo.toml" && -f "${REPO_ROOT}/${BIN_NAME}" ]]; then
    SOURCE_PACKAGE_DIR="${REPO_ROOT}"
  fi
else
  remote_asset_name="$(dhtgbot_asset_name)"
  remote_url="$(github_release_url "${REMOTE_REPO_OWNER}" "${REMOTE_REPO_NAME}" "${REMOTE_VERSION}" "${remote_asset_name}" "${REMOTE_BASE_URL}")"
  if ! remote_tmp_dir="$(download_and_extract_archive "${remote_url}" "${remote_asset_name}" "tar.gz")"; then
    exit 1
  fi
  SOURCE_PACKAGE_DIR="${remote_tmp_dir}"
  SOURCE_BINARY="${remote_tmp_dir}/${BIN_NAME}"
  SOURCE_TEMPLATE="${remote_tmp_dir}/config.example.yaml"
  SOURCE_SCRIPTS_DIR="${remote_tmp_dir}/scripts"

  if [[ ! -f "${SOURCE_BINARY}" ]]; then
    printf '[dhtgbot] downloaded package did not contain %s\n' "${BIN_NAME}" >&2
    exit 1
  fi
fi

if [[ "${INSTALL_LAYOUT}" == "runtime" ]]; then
  if [[ -z "${APP_HOME}" ]]; then
    APP_HOME="$(default_app_home)"
  fi

  if [[ -z "${INSTALL_DIR}" ]]; then
    INSTALL_DIR="${APP_HOME}"
  fi

  if [[ "${SKIP_DEPS}" -ne 1 ]]; then
    install_amagi
    install_tdlr
    install_aria2
    persist_dependency_path_entries
  fi

  install_runtime_layout "${SOURCE_BINARY}" "${SOURCE_TEMPLATE}" "${SOURCE_SCRIPTS_DIR}"

  printf '[dhtgbot] installed launcher to %s/%s\n' "${INSTALL_DIR}" "${BIN_NAME}"
  printf '[dhtgbot] application home: %s\n' "${APP_HOME}"
  printf '[dhtgbot] copy the example config before the first real run:\n'
  printf '  cp "%s/config.example.yaml" "%s/config.yaml"\n' "${APP_HOME}" "${APP_HOME}"
  printf '[dhtgbot] then edit %s/config.yaml\n' "${APP_HOME}"
  printf '[dhtgbot] confirm services.amagi.start_command, services.tdlr.start_command, and services.aria2.start_command in config.yaml\n'
  printf '[dhtgbot] if you use X polling, fill bots.xdl.twitter.cookies in config.yaml\n'
  printf '[dhtgbot] dependencies are installed as environment commands:\n'
  printf '  amagi -> %s\n' "${AMAGI_INSTALL_DIR}"
  printf '  tdlr  -> %s\n' "${TDLR_INSTALL_DIR}"
  if [[ "$(uname -s)" == "Darwin" ]]; then
    printf '  aria2 -> Homebrew formula aria2\n'
  else
    printf '  aria2 -> %s\n' "${ARIA2_INSTALL_DIR}"
  fi
  printf '[dhtgbot] run the bot from the application directory:\n'
  printf '  cd "%s"\n' "${APP_HOME}"
  printf '  ./%s\n' "${BIN_NAME}"
else
  if [[ -z "${WORKSPACE_DIR}" ]]; then
    WORKSPACE_DIR="$(default_workspace_dir)"
  fi

  install_workspace_layout "${SOURCE_PACKAGE_DIR}" "${SOURCE_BINARY}" "${SOURCE_TEMPLATE}" "${SOURCE_SCRIPTS_DIR}" "${WORKSPACE_DIR}"

  if [[ "${SKIP_DEPS}" -ne 1 ]]; then
    install_amagi
    install_tdlr
    install_aria2
    persist_dependency_path_entries
  fi

  print_workspace_summary "${WORKSPACE_DIR}"

  maybe_enter_workspace_shell "${WORKSPACE_DIR}"
fi
