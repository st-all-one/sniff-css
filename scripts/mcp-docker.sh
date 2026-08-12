#!/usr/bin/env bash
# sniffCSS-mcp launcher for Docker (MCP stdio).
#
# Starts the sniffCSS container from Docker Hub (if not running) and runs the
# MCP server inside it via `docker exec -i`, attaching to the GUI Chromium
# (SNIFF_CONNECT=http://127.0.0.1:9222 is the image default). The init banner
# of the linuxserver image never reaches stdio, so the JSON-RPC channel stays
# clean for MCP clients.
#
# Usage (opencode.jsonc):
#   "command": ["scripts/mcp-docker.sh"]
set -euo pipefail

IMAGE="${SNIFF_DOCKER_IMAGE:-stallonels/sniffcss:latest}"
CONTAINER="${SNIFF_DOCKER_CONTAINER:-sniffcss-mcp}"
CONFIG_DIR="${SNIFF_CONFIG_DIR:-$(pwd)/sniffcss-config}"
SHM="${SNIFF_DOCKER_SHM:-1gb}"

ensure_running() {
    if docker inspect "$CONTAINER" >/dev/null 2>&1; then
        return 0
    fi
    docker run -d --name "$CONTAINER" \
        --shm-size "$SHM" \
        -v "$CONFIG_DIR:/config" \
        "$IMAGE" >/dev/null 2>&1
    # Wait for the GUI Chromium CDP endpoint (up to 90s).
    local deadline=$(( $(date +%s) + 90 ))
    until docker exec "$CONTAINER" bash -c \
        'exec 3<>/dev/tcp/127.0.0.1/9222' 2>/dev/null; do
        if (( $(date +%s) >= deadline )); then
            echo "[sniffCSS-mcp] container up but CDP not ready in 90s" >&2
            break
        fi
        sleep 2
    done
}

ensure_running
exec docker exec -i "$CONTAINER" /opt/sniffcss/bin/sniffCSS-mcp
