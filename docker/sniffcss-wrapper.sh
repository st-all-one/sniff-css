#!/usr/bin/env bash
# Wait-wrapper for the sniffCSS toolchain inside the Docker container.
#
# The real binaries live at /opt/sniffcss/bin/; this wrapper (installed as
# /usr/local/bin/sniffCSS, sniffCSS-diff, sniffCSS-check, sniffCSS-mcp) waits
# until the in-container GUI Chromium's CDP endpoint is reachable, then execs
# the real binary. This avoids "browser not ready yet" races right after the
# container boots.
set -euo pipefail

self="$(basename "$0")"
real="/opt/sniffcss/bin/${self}"

# Only browsers-based tools need the wait. diff/check are pure offline.
case "$self" in
  sniffCSS | sniffCSS-mcp) ;;
  *) exec "$real" "$@" ;;
esac

# CDP endpoint to wait for. Prefer SNIFF_CONNECT (already points at the GUI
# browser in the image); fall back to the default DevTools origin.
endpoint="${SNIFF_CONNECT:-http://127.0.0.1:9222}"

# Normalize to a host:port for a TCP probe.
case "$endpoint" in
  ws://*|wss://*) hostport="${endpoint#*://}"; hostport="${hostport%%/*}" ;;
  http://*|https://*) hostport="${endpoint#*://}"; hostport="${hostport%%/*}" ;;
  *) hostport="${endpoint%%/*}" ;;
esac
host="${hostport%:*}"; port="${hostport##*:}"

deadline=$(( $(date +%s) + 60 ))
until exec 3<>"/dev/tcp/${host}/${port}" 2>/dev/null; do
  if (( $(date +%s) >= deadline )); then
    echo "[sniffCSS] CDP endpoint ${endpoint} not ready after 60s" >&2
    exec "$real" "$@"
  fi
  sleep 1
done
exec 3<&- 3>&-

# Run the real binary; SNIFF_CONNECT makes it attach to the GUI browser.
exec "$real" "$@"
