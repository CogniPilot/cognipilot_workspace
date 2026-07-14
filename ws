#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
client="$root/.nixspace/ws/bin/ws"

if [ ! -x "$client" ]; then
  printf '%s\n' 'ws: the cached nixspace client is missing; run ./setup' >&2
  exit 1
fi

exec "$client" --workspace-root "$root" "$@"
