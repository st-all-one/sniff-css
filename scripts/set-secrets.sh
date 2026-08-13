#!/usr/bin/env bash
# Configura os secrets do GitHub necessários para o workflow de release.
#
# Requer a CLI `gh` autenticada no repositório st-all-one/sniff-css.
#
# Uso:
#   scripts/set-secrets.sh
#   # ou passando os valores inline:
#   DOCKERHUB_USERNAME=meu-usuario DOCKERHUB_TOKEN=xxxx scripts/set-secrets.sh
set -euo pipefail

REPO="st-all-one/sniff-css"
USERNAME="${DOCKERHUB_USERNAME:-}"
TOKEN="${DOCKERHUB_TOKEN:-}"

command -v gh >/dev/null 2>&1 || { echo "✗ gh CLI não encontrada. Instale: https://cli.github.com/" >&2; exit 1; }

# ── helpers ──────────────────────────────────────────────────────────────────
info() { printf "\e[34m==>\e[0m %s\n" "$*"; }
ok()   { printf "\e[32m  ✓\e[0m %s\n" "$*"; }

read_secret() {
    local prompt="$1" value="" input=""
    printf "%s: " "$prompt" >&2
    read -rs input < /dev/tty
    printf "\n" >&2
    echo "$input"
}

if [[ -z "$USERNAME" ]]; then
    USERNAME="$(read_secret "Docker Hub username")"
fi
if [[ -z "$TOKEN" ]]; then
    TOKEN="$(read_secret "Docker Hub access token (https://hub.docker.com/settings/security)")"
fi

[[ -n "$USERNAME" && -n "$TOKEN" ]] || { echo "✗ username e token são obrigatórios" >&2; exit 1; }

info "Configurando secrets em ${REPO}..."
echo "$USERNAME" | gh secret set DOCKERHUB_USERNAME --repo "$REPO"
echo "$TOKEN"    | gh secret set DOCKERHUB_TOKEN --repo "$REPO"
ok "Secrets configurados. Faça um release com:"
echo "  git tag v0.4.0 && git push origin v0.4.0"
