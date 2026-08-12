#!/bin/bash
# Helper para build/uso do container sniffCSS (Docker).
#
#   scripts/docker.sh build          # docker build -t sniffcss:latest
#   scripts/docker.sh up             # docker compose up -d (GUI em :3001)
#   scripts/docker.sh down           # docker compose down
#   scripts/docker.sh exec -- sniffCSS -u URL -s SEL   # captura dentro do container
#   scripts/docker.sh mcp            # sniffCSS-mcp (stdio) via docker exec -i
set -euo pipefail

COMPOSE_FILE="docker/docker-compose.yml"
SERVICE="sniffcss"

case "${1:-}" in
  build)
    docker build -t sniffcss:latest -f docker/Dockerfile .
    ;;
  up)
    docker compose -f "$COMPOSE_FILE" up -d
    ;;
  down)
    docker compose -f "$COMPOSE_FILE" down
    ;;
  exec)
    shift
    docker compose -f "$COMPOSE_FILE" exec "$SERVICE" "$@"
    ;;
  mcp)
    docker compose -f "$COMPOSE_FILE" exec -i "$SERVICE" sniffCSS-mcp
    ;;
  *)
    echo "uso: scripts/docker.sh {build|up|down|exec -- <cmd>|mcp}"
    exit 1
    ;;
esac
