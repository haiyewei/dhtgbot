#!/bin/sh
set -eu

APP_HOME="${DHTGBOT_HOME:-/var/lib/dhtgbot}"
DEFAULT_TEMPLATE="/opt/dhtgbot/config.example.docker.yaml"
TEMPLATE_PATH="${DHTGBOT_CONFIG_TEMPLATE:-$DEFAULT_TEMPLATE}"
CONFIG_PATH="${APP_HOME}/config.yaml"
BIN_PATH="/opt/dhtgbot/bin/dhtgbot"
CREATED_CONFIG=0
RUN_AS_ROOT=0

bootstrap_runtime() {
  mkdir -p "${APP_HOME}" "${APP_HOME}/data" "${APP_HOME}/data/downloads" "${APP_HOME}/logs"

  CREATED_CONFIG=0
  if [ ! -f "${CONFIG_PATH}" ]; then
    cp "${TEMPLATE_PATH}" "${CONFIG_PATH}"
    CREATED_CONFIG=1
  fi

  prepare_runtime_permissions
  cd "${APP_HOME}"
}

prepare_runtime_permissions() {
  RUN_AS_ROOT=0

  if [ "$(id -u)" -ne 0 ]; then
    return
  fi

  if chown -R dhtgbot:dhtgbot "${APP_HOME}" 2>/dev/null; then
    return
  fi

  RUN_AS_ROOT=1
  printf '%s\n' "[dhtgbot] warning: failed to chown ${APP_HOME}; starting the main process as root." >&2
}

config_has_template_placeholders() {
  [ -f "${CONFIG_PATH}" ] || return 1
  grep -Eq 'REPLACE_WITH_MASTER_BOT_TOKEN' "${CONFIG_PATH}"
}

print_container_help() {
  cat <<EOF
dhtgbot container helper

Usage:
  dhtgbot [container-command]

Container commands:
  help, -h, --help      Show this help text
  init                  Create ${CONFIG_PATH} from the Docker template and print next steps
  config-path           Print the runtime config path
  show-config           Print the current runtime config.yaml (creates it first if missing)
  example-config        Print the bundled Docker config template
  dhtgbot [args]        Start the main program

Default behavior:
  Start dhtgbot with ${CONFIG_PATH}

Suggested first-run flow:
  1. docker run --rm -v <host-dir>:${APP_HOME} <image> init
  2. Edit <host-dir>/config.yaml
  3. docker run -d -v <host-dir>:${APP_HOME} -p 4567:4567 -p 8787:8787 -p 6800:6800 <image>
EOF
}

print_init_instructions() {
  cat >&2 <<EOF
[dhtgbot] runtime directory: ${APP_HOME}
[dhtgbot] config path: ${CONFIG_PATH}
[dhtgbot] template source: ${TEMPLATE_PATH}
EOF

  if [ "${CREATED_CONFIG}" -eq 1 ]; then
    printf '%s\n' "[dhtgbot] created ${CONFIG_PATH} from ${TEMPLATE_PATH}." >&2
  else
    printf '%s\n' "[dhtgbot] using existing ${CONFIG_PATH}." >&2
  fi

  cat >&2 <<EOF
[dhtgbot] next steps:
  1. Replace placeholder bot tokens and account IDs in config.yaml
  2. Adjust groups, topics, cookies, and start_command values as needed
  3. Start the container again after saving the file
EOF
}

print_placeholder_guidance() {
  cat >&2 <<EOF
[dhtgbot] configuration is not ready: ${CONFIG_PATH} still contains template placeholders.
[dhtgbot] run the init flow first, then edit config.yaml on the mounted host path before starting the service.
[dhtgbot] quick start:
  docker run --rm -v <host-dir>:${APP_HOME} <image> init
  edit <host-dir>/config.yaml
  docker run -d -v <host-dir>:${APP_HOME} -p 4567:4567 -p 8787:8787 -p 6800:6800 <image>
EOF
}

run_app() {
  bootstrap_runtime

  if [ "${CREATED_CONFIG}" -eq 1 ]; then
    print_init_instructions
    exit 64
  fi

  if config_has_template_placeholders; then
    print_placeholder_guidance
    exit 64
  fi

  if [ "$(id -u)" -eq 0 ] && [ "${RUN_AS_ROOT}" -eq 0 ]; then
    exec su dhtgbot -s /bin/sh -c 'cd "$1" && shift && exec "$@"' sh "${APP_HOME}" "${BIN_PATH}" "$@"
  fi

  exec "${BIN_PATH}" "$@"
}

if [ "$#" -eq 0 ]; then
  run_app
fi

case "$1" in
  help|-h|--help)
    print_container_help
    ;;
  init)
    bootstrap_runtime
    print_init_instructions
    ;;
  config-path)
    printf '%s\n' "${CONFIG_PATH}"
    ;;
  show-config)
    bootstrap_runtime
    cat "${CONFIG_PATH}"
    ;;
  example-config)
    cat "${TEMPLATE_PATH}"
    ;;
  dhtgbot|/opt/dhtgbot/bin/dhtgbot)
    shift
    run_app "$@"
    ;;
  -*)
    print_container_help
    ;;
  *)
    exec "$@"
    ;;
esac
