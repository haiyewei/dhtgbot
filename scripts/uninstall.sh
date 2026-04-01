#!/usr/bin/env bash
set -euo pipefail

BIN_NAME="dhtgbot"
APP_HOME="${DHTGBOT_HOME:-}"
INSTALL_DIR="${DHTGBOT_INSTALL_DIR:-}"
KEEP_PATH=0
REMOVE_DATA=0
KEEP_DATA=0
SCRIPT_PATH="${BASH_SOURCE[0]:-}"
SCRIPT_DIR=""

if [[ -n "${SCRIPT_PATH}" ]]; then
  SCRIPT_DIR="$(cd -- "$(dirname -- "${SCRIPT_PATH}")" && pwd)"
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --home-dir)
      APP_HOME="$2"
      shift 2
      ;;
    --install-dir)
      INSTALL_DIR="$2"
      shift 2
      ;;
    --keep-path)
      KEEP_PATH=1
      shift
      ;;
    --remove-data)
      REMOVE_DATA=1
      shift
      ;;
    --keep-data)
      KEEP_DATA=1
      shift
      ;;
    *)
      printf '[dhtgbot] unknown flag: %s\n' "$1" >&2
      exit 1
      ;;
  esac
done

if [[ "${REMOVE_DATA}" -eq 1 && "${KEEP_DATA}" -eq 1 ]]; then
  printf '[dhtgbot] cannot specify both --remove-data and --keep-data\n' >&2
  exit 1
fi

default_app_home() {
  printf '%s\n' "${HOME}/.local/share/dhtgbot"
}

normalize_dir() {
  local value="$1"

  [[ -n "${value}" ]] || return 1
  value="${value%/}"
  [[ -n "${value}" ]] || value="/"

  if [[ -d "${value}" ]]; then
    (
      cd -- "${value}"
      pwd
    )
  else
    printf '%s\n' "${value}"
  fi
}

is_runtime_root() {
  local root="$1"
  [[ -n "${root}" ]] \
    && [[ -d "${root}" ]] \
    && [[ -f "${root}/config.example.yaml" ]] \
    && [[ -f "${root}/bin/dhtgbot-real" ]]
}

resolve_profile_files() {
  if [[ -n "${DHTGBOT_PROFILE_FILE:-}" ]]; then
    printf '%s\n' "${DHTGBOT_PROFILE_FILE}"
    return 0
  fi

  printf '%s\n' "${HOME}/.bashrc"
  printf '%s\n' "${HOME}/.zshrc"
  printf '%s\n' "${HOME}/.profile"
}

resolve_launcher_path() {
  command -v "${BIN_NAME}" 2>/dev/null || true
}

supports_interactive_prompt() {
  [[ -t 0 && -t 1 ]]
}

confirm_dependency_uninstall() {
  local display_name="$1"
  local target="$2"
  local answer

  if ! supports_interactive_prompt; then
    printf '[dhtgbot] preserved %s at %s (non-interactive mode)\n' "${display_name}" "${target}"
    return 1
  fi

  printf '[dhtgbot] uninstall %s at %s? [y/N] ' "${display_name}" "${target}"
  read -r answer
  case "${answer}" in
    [yY]|[yY][eE][sS])
      return 0
      ;;
    *)
      printf '[dhtgbot] preserved %s\n' "${display_name}"
      return 1
      ;;
  esac
}

resolve_app_home_from_launcher() {
  local launcher_path="$1"
  local line

  [[ -n "${launcher_path}" && -f "${launcher_path}" ]] || return 1

  while IFS= read -r line || [[ -n "${line}" ]]; do
    line="${line%$'\r'}"
    if [[ "${line}" =~ ^APP_HOME=\'([^\']+)\'$ ]]; then
      printf '%s\n' "${BASH_REMATCH[1]}"
      return 0
    fi
  done < "${launcher_path}"

  return 1
}

add_unique_candidate() {
  local value="$1"
  local normalized
  local existing

  normalized="$(normalize_dir "${value}" 2>/dev/null || true)"
  [[ -n "${normalized}" ]] || return 0

  for existing in "${CANDIDATE_DIRS[@]:-}"; do
    if [[ "${existing}" == "${normalized}" ]]; then
      return 0
    fi
  done

  CANDIDATE_DIRS+=("${normalized}")
}

resolve_existing_home() {
  local launcher_path
  local launcher_home
  local candidate

  CANDIDATE_DIRS=()
  add_unique_candidate "${APP_HOME}"
  if [[ -n "${SCRIPT_DIR}" && "$(basename -- "${SCRIPT_DIR}")" == "scripts" ]]; then
    add_unique_candidate "$(cd -- "${SCRIPT_DIR}/.." 2>/dev/null && pwd || true)"
  fi
  add_unique_candidate "${PWD}"
  add_unique_candidate "$(default_app_home)"

  launcher_path="$(resolve_launcher_path)"
  launcher_home="$(resolve_app_home_from_launcher "${launcher_path}" || true)"
  add_unique_candidate "${launcher_home}"

  for candidate in "${CANDIDATE_DIRS[@]:-}"; do
    if is_runtime_root "${candidate}"; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  done

  return 1
}

resolve_existing_install_dir() {
  local launcher_path

  if [[ -n "${INSTALL_DIR}" ]]; then
    printf '%s\n' "${INSTALL_DIR}"
    return 0
  fi

  launcher_path="$(resolve_launcher_path)"
  if [[ -n "${launcher_path}" && -e "${launcher_path}" ]]; then
    dirname -- "${launcher_path}"
    return 0
  fi

  if [[ -n "${APP_HOME}" && -f "${APP_HOME}/${BIN_NAME}" ]]; then
    printf '%s\n' "${APP_HOME}"
    return 0
  fi

  return 1
}

remove_path_entry_from_file() {
  local file="$1"
  local entry="$2"
  local temp_file
  local status=0

  [[ -f "${file}" ]] || return 0

  temp_file="$(mktemp "${TMPDIR:-/tmp}/dhtgbot-uninstall.XXXXXX")"

  awk -v target="export PATH=\"${entry}:\$PATH\"" '
    BEGIN {
      pending_comment = ""
      changed = 0
    }
    {
      if ($0 == "# dhtgbot installer") {
        pending_comment = $0
        next
      }

      if ($0 == target) {
        pending_comment = ""
        changed = 1
        next
      }

      if (pending_comment != "") {
        print pending_comment
        pending_comment = ""
      }

      print
    }
    END {
      if (pending_comment != "") {
        print pending_comment
      }
      exit(changed ? 10 : 0)
    }
  ' "${file}" > "${temp_file}" || status=$?

  case "${status:-0}" in
    10)
      mv "${temp_file}" "${file}"
      printf '[dhtgbot] removed PATH entry from %s\n' "${file}"
      return 0
      ;;
    0)
      rm -f "${temp_file}"
      return 0
      ;;
    *)
      rm -f "${temp_file}"
      return 1
      ;;
  esac
}

update_current_shell_path() {
  local filtered=()
  local entry
  local entries

  [[ -n "${INSTALL_DIR}" ]] || return 0

  IFS=':' read -r -a entries <<< "${PATH}"
  for entry in "${entries[@]}"; do
    if [[ "${entry%/}" == "${INSTALL_DIR%/}" ]]; then
      continue
    fi
    filtered+=("${entry}")
  done

  PATH="$(IFS=:; printf '%s' "${filtered[*]}")"
  export PATH
}

remove_file_if_exists() {
  local path="$1"
  if [[ -f "${path}" ]]; then
    rm -f "${path}"
    printf '[dhtgbot] removed %s\n' "${path}"
    REMOVED_ANY=1
  fi
}

remove_dir_if_exists() {
  local path="$1"
  if [[ -d "${path}" ]]; then
    rm -rf -- "${path}"
    printf '[dhtgbot] removed %s\n' "${path}"
    REMOVED_ANY=1
  fi
}

remove_empty_dir() {
  local path="$1"
  [[ -d "${path}" ]] || return 0
  if find "${path}" -mindepth 1 -print -quit 2>/dev/null | grep -q .; then
    return 0
  fi
  rmdir "${path}" 2>/dev/null || return 0
  printf '[dhtgbot] removed empty directory %s\n' "${path}"
}

resolve_dependency_binary_path() {
  local env_install_dir="$1"
  local binary_name="$2"
  local command_name="$3"
  local candidate=""

  if [[ -n "${env_install_dir}" && -f "${env_install_dir}/${binary_name}" ]]; then
    printf '%s\n' "${env_install_dir}/${binary_name}"
    return 0
  fi

  candidate="$(command -v "${command_name}" 2>/dev/null || true)"
  if [[ -n "${candidate}" && -f "${candidate}" ]]; then
    printf '%s\n' "${candidate}"
    return 0
  fi

  if [[ -f "${HOME}/.local/bin/${binary_name}" ]]; then
    printf '%s\n' "${HOME}/.local/bin/${binary_name}"
    return 0
  fi

  return 1
}

uninstall_dependency_binary() {
  local display_name="$1"
  local binary_path="$2"
  local binary_dir

  if ! confirm_dependency_uninstall "${display_name}" "${binary_path}"; then
    return 0
  fi

  rm -f "${binary_path}"
  printf '[dhtgbot] removed %s\n' "${binary_path}"
  REMOVED_ANY=1

  binary_dir="$(dirname -- "${binary_path}")"
  remove_empty_dir "${binary_dir}"
}

maybe_uninstall_amagi() {
  local binary_path
  binary_path="$(resolve_dependency_binary_path "${AMAGI_INSTALL_DIR:-}" "amagi" "amagi" || true)"
  if [[ -n "${binary_path}" ]]; then
    uninstall_dependency_binary "amagi" "${binary_path}"
  fi
}

maybe_uninstall_tdlr() {
  local binary_path
  binary_path="$(resolve_dependency_binary_path "${TDLR_INSTALL_DIR:-}" "tdlr" "tdlr" || true)"
  if [[ -n "${binary_path}" ]]; then
    uninstall_dependency_binary "tdlr" "${binary_path}"
  fi
}

maybe_uninstall_aria2() {
  local brew_path=""
  local target=""
  local binary_path=""

  if [[ "$(uname -s)" == "Darwin" ]]; then
    brew_path="$(command -v brew 2>/dev/null || true)"
    if [[ -n "${brew_path}" ]] && "${brew_path}" list --versions aria2 >/dev/null 2>&1; then
      target="$("${brew_path}" --prefix aria2)/bin/aria2c"
      if confirm_dependency_uninstall "aria2" "${target}"; then
        "${brew_path}" uninstall aria2
        printf '[dhtgbot] uninstalled Homebrew formula aria2\n'
        REMOVED_ANY=1
      fi
      return 0
    fi
  fi

  binary_path="$(resolve_dependency_binary_path "${ARIA2_INSTALL_DIR:-}" "aria2c" "aria2c" || true)"
  if [[ -n "${binary_path}" ]]; then
    uninstall_dependency_binary "aria2" "${binary_path}"
  fi
}

REMOVED_ANY=0

APP_HOME="$(resolve_existing_home || true)"
if [[ -z "${APP_HOME}" ]]; then
  printf '[dhtgbot] no existing runtime installation was detected automatically.\n'
  printf '[dhtgbot] workspace checkouts are not removed automatically; delete that directory manually if needed.\n'
  exit 0
fi

INSTALL_DIR="$(resolve_existing_install_dir || true)"
if [[ -n "${INSTALL_DIR}" ]]; then
  INSTALL_DIR="$(normalize_dir "${INSTALL_DIR}" 2>/dev/null || printf '%s' "${INSTALL_DIR}")"
fi

if [[ -n "${INSTALL_DIR}" ]]; then
  remove_file_if_exists "${INSTALL_DIR}/${BIN_NAME}"
fi
remove_dir_if_exists "${APP_HOME}/bin"
remove_dir_if_exists "${APP_HOME}/scripts"
remove_file_if_exists "${APP_HOME}/config.example.yaml"

if [[ "${REMOVE_DATA}" -eq 1 ]]; then
  remove_dir_if_exists "${APP_HOME}"
else
  printf '[dhtgbot] preserved runtime data in %s\n' "${APP_HOME}"
  printf '[dhtgbot] kept config.yaml, data, logs, and any other user files\n'
  remove_empty_dir "${APP_HOME}"
fi

if [[ "${KEEP_PATH}" -eq 0 && -n "${INSTALL_DIR}" ]]; then
  while IFS= read -r profile_file; do
    remove_path_entry_from_file "${profile_file}" "${INSTALL_DIR}"
  done < <(resolve_profile_files)
  update_current_shell_path
fi

if [[ -n "${INSTALL_DIR}" ]]; then
  remove_empty_dir "${INSTALL_DIR}"
fi

maybe_uninstall_amagi
maybe_uninstall_tdlr
maybe_uninstall_aria2

if [[ "${REMOVED_ANY}" -eq 1 ]]; then
  printf '[dhtgbot] uninstall complete\n'
else
  printf '[dhtgbot] nothing was removed\n'
fi
