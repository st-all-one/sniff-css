#!/bin/bash
# Helper para build/uso do container sniffCSS (Docker).
#
#   scripts/docker.sh build           # docker build (baixa binário do Release)
#   scripts/docker.sh build-source    # docker build compilando do fonte (dev)
#   scripts/docker.sh up              # docker compose up -d (GUI em :3001)
#   scripts/docker.sh down            # docker compose down
#   scripts/docker.sh exec -- sniffCSS -u URL -s SEL   # captura dentro do container
#   scripts/docker.sh mcp             # sniffCSS-mcp (stdio) via docker exec -i
set -euo pipefail

COMPOSE_FILE="docker/docker-compose.yml"
SERVICE="sniffcss"
IMAGE="stallonels/sniffcss:latest"

case "${1:-}" in
  build)
    docker build -t "$IMAGE" -f docker/Dockerfile .
    ;;
  build-source)
    docker build -t "$IMAGE" -f docker/Dockerfile --build-arg BUILD_FROM_SOURCE=1 .
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
  mcp-docker)
    exec ./scripts/mcp-docker.sh
    ;;
  *)
    echo "uso: scripts/docker.sh {build|build-source|up|down|exec -- <cmd>|mcp|mcp-docker}"
    exit 1
    ;;
esac
