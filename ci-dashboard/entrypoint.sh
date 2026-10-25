#!/bin/sh
# Starts as root only to make the state volume writable, then runs the server
# as the unprivileged node user.
set -eu
mkdir -p "${STATE_DIR}"
chown node:node "${STATE_DIR}"
exec setpriv --reuid=node --regid=node --init-groups --no-new-privs \
  bun /app/server/main.ts
