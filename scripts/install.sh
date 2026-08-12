#!/bin/bash
set -euo pipefail

# Instala os binários sniffCSS, sniffCSS-diff, sniffCSS-check e
# sniffCSS-mcp em ~/.local/bin e garante que o diretório esteja no PATH
# (Linux/macOS).

BIN_SNIFF="sniffCSS"
BIN_DIFF="sniffCSS-diff"
BIN_CHECK="sniffCSS-check"
BIN_MCP="sniffCSS-mcp"
INSTALL_DIR="${HOME}/.local/bin"

# ── helpers ──────────────────────────────────────────────────────────────────

info()  { printf "\e[34m==>\e[0m %s\n" "$*"; }
ok()    { printf "\e[32m  ✓\e[0m %s\n" "$*"; }
warn()  { printf "\e[33m  !\e[0m %s\n" "$*"; }
err()   { printf "\e[31m  ✗\e[0m %s\n" "$*"; exit 1; }

add_path_line() {
    local file="$1"
    local line='export PATH="${HOME}/.local/bin:${PATH}"'
    local marker="# --- sniffCSS path ---"

    # Skip if file doesn't exist — nothing to do
    [[ -f "$file" ]] || return 0

    # Already present — skip
    grep -qxF "$line" "$file" 2>/dev/null && return 0
    grep -qxF "$marker" "$file" 2>/dev/null && return 0

    # Ensure trailing newline before appending
    if [[ -s "$file" && "$(tail -c1 "$file" | wc -l)" -eq 0 ]]; then
        echo "" >> "$file"
    fi

    {
        echo "$marker"
        echo "$line"
    } >> "$file"
    ok "PATH adicionado a ${file/$HOME/\~}"
}

setup_path() {
    info "Verificando PATH..."

    if echo "$PATH" | tr ':' '\n' | grep -qxF "$INSTALL_DIR"; then
        ok "${INSTALL_DIR} já está no PATH"
        return 0
    fi

    add_path_line "${HOME}/.profile"
    add_path_line "${HOME}/.bashrc"
    add_path_line "${HOME}/.zshrc"
    add_path_line "${ZDOTDIR:-${HOME}}/.zshenv"
    add_path_line "${ZDOTDIR:-${HOME}}/.zshrc"

    warn "${INSTALL_DIR} foi adicionado aos rc files."
    warn "Reinicie o shell ou execute: export PATH=\"\${HOME}/.local/bin:\${PATH}\""
}

check_chrome() {
    info "Verificando Chrome/Chromium..."

    if command -v google-chrome >/dev/null 2>&1 \
        || command -v google-chrome-stable >/dev/null 2>&1 \
        || command -v chromium >/dev/null 2>&1 \
        || command -v chromium-browser >/dev/null 2>&1 \
        || [[ -n "${SNIFF_CHROME_PATH:-}" ]]; then
        ok "Navegador baseado em Chromium encontrado"
        return 0
    fi

    warn "Nenhum Chrome/Chromium encontrado no PATH."
    warn "A captura de estilos precisa dele. Instale o Chrome/Chromium ou "
    warn "defina SNIFF_CHROME_PATH=/caminho/para/chrome (ou use --chrome)."
}

# ── main ─────────────────────────────────────────────────────────────────────

cd "$(dirname "$0")/.."    # a partir de scripts/, sobe para a raiz do repo

if [[ "${1:-}" != "--no-build" ]]; then
    command -v cargo >/dev/null 2>&1 \
        || err "Rust/Cargo não encontrado. Instale com:\n  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"

    info "Compilando em release..."
    cargo build --release
else
    info "Instalando binários existentes de target/release (--no-build)..."
fi

[[ -x "target/release/${BIN_SNIFF}" ]] || err "Binário ${BIN_SNIFF} não encontrado em target/release. Rode sem --no-build."
[[ -x "target/release/${BIN_DIFF}" ]] || err "Binário ${BIN_DIFF} não encontrado em target/release. Rode sem --no-build."
[[ -x "target/release/${BIN_CHECK}" ]] || err "Binário ${BIN_CHECK} não encontrado em target/release. Rode sem --no-build."
[[ -x "target/release/${BIN_MCP}" ]] || err "Binário ${BIN_MCP} não encontrado em target/release. Rode sem --no-build."

info "Instalando em ${INSTALL_DIR}/..."
mkdir -p "$INSTALL_DIR"
cp "target/release/${BIN_SNIFF}" "${INSTALL_DIR}/${BIN_SNIFF}"
cp "target/release/${BIN_DIFF}" "${INSTALL_DIR}/${BIN_DIFF}"
cp "target/release/${BIN_CHECK}" "${INSTALL_DIR}/${BIN_CHECK}"
cp "target/release/${BIN_MCP}" "${INSTALL_DIR}/${BIN_MCP}"
ok "Binários: ${INSTALL_DIR}/${BIN_SNIFF}, ${INSTALL_DIR}/${BIN_DIFF}, ${INSTALL_DIR}/${BIN_CHECK}, ${INSTALL_DIR}/${BIN_MCP}"

setup_path
check_chrome

echo ""
info "Pronto! Execute no terminal:"
echo "  sniffCSS -u <URL> -s <selector> --stable-key data-testid   # default otimizado p/ IA"
echo "  sniffCSS -u <URL> -s <selector> --full                      # full-fidelity (sem otimizações)"
echo "  sniffCSS-diff base.jsonl head.jsonl --tolerance 0.5"
echo "  sniffCSS-check --input snap.jsonl --uniform --rules"
echo "  sniffCSS-mcp   # servidor MCP (stdio) para agentes de IA"
echo ""
info "Guia para IA: docs/ai-usage.md | Padrão ouro: docs/golden-run.md"
