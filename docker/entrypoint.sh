#!/bin/sh
set -eu

APP_HOME="${DHTGBOT_HOME:-/var/lib/dhtgbot}"
DEFAULT_TEMPLATE="/opt/dhtgbot/config.example.docker.yaml"
TEMPLATE_PATH="${DHTGBOT_CONFIG_TEMPLATE:-$DEFAULT_TEMPLATE}"

bootstrap_runtime() {
  mkdir -p "${APP_HOME}" "${APP_HOME}/data" "${APP_HOME}/data/downloads" "${APP_HOME}/logs"

  if [ ! -f "${APP_HOME}/config.yaml" ]; then
    cp "${TEMPLATE_PATH}" "${APP_HOME}/config.yaml"
    printf '%s\n' "[dhtgbot] created ${APP_HOME}/config.yaml from ${TEMPLATE_PATH}. Update tokens and accounts before production use." >&2
  fi

  cd "${APP_HOME}"
}

if [ "$#" -eq 0 ]; then
  bootstrap_runtime
  exec /opt/dhtgbot/bin/dhtgbot
fi

case "$1" in
  dhtgbot|/opt/dhtgbot/bin/dhtgbot)
    shift
    bootstrap_runtime
    exec /opt/dhtgbot/bin/dhtgbot "$@"
    ;;
  -*)
    bootstrap_runtime
    exec /opt/dhtgbot/bin/dhtgbot "$@"
    ;;
  *)
    exec "$@"
    ;;
esac
