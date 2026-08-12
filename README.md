# SniffCSS

Capture o **computed style real** de elementos de uma página e use isso para
desenvolvimento assistido por IA. Fala direto com o navegador via **Chrome
DevTools Protocol (CDP)**, sem frameworks de automação — rápido, determinístico
e com saída otimizada para LLMs.

## Instalação

```bash
curl --proto '=https' --tlsv1.2 -sSf \
  https://raw.githubusercontent.com/st-all-one/sniff-css/main/install.sh | sh
```

Baixa o binário certo para seu sistema (Linux glibc/musl, macOS Apple Silicon,
Windows), verifica o checksum e instala em `~/.local/bin`.

- Versão específica: `... install.sh | VERSION=v0.1.0 sh`
- Compilar do fonte: `cargo build --release` (ver `scripts/install.sh`)
- Container self-contained (Chromium incluso): [`docs/docker.md`](docs/docker.md)

Requisito: Chrome/Chromium no sistema (ou `SNIFF_CHROME_PATH` / `--chrome`).

## Quickstart

```bash
# Computed styles de um botão — default otimizado para IA
sniffCSS -u http://localhost:3000 -s ".btn-primary"

# Subárvore, só box-model + tipografia
sniffCSS -u http://localhost:3000 -s ".card" \
  --depth 1 --categories box-model,typography

# Diff determinístico entre duas versões
sniffCSS-diff base.jsonl head.jsonl --tolerance 0.5

# Checks offline: contraste AA, alvos ≥24px, foco, uniformidade
sniffCSS-check --input snap.jsonl --uniform --rules
```

## MCP (agentes de IA)

Exponha `sniffCSS-mcp` como servidor MCP (stdio) e deixe o agente capturar,
difar e auditar sem jogar snapshot gigante no contexto. Fluxo recomendado:
**captura determinística → diff → checks → IA interpreta o delta**.

- Guia de integração: [`docs/ai-usage.md`](docs/ai-usage.md)
- MCP no container Docker: [`docs/docker.md#mcp-via-docker`](docs/docker.md#mcp-via-docker)

## Documentação

| Doc | Conteúdo |
|---|---|
| [`docs/usage.md`](docs/usage.md) | Referência completa da CLI. |
| [`docs/docker.md`](docs/docker.md) | Docker: quickstart, compose, MCP via container. |
| [`docs/ai-usage.md`](docs/ai-usage.md) | Guia para IA: pipeline captura→diff→checks. |
| [`docs/diff-checks.md`](docs/diff-checks.md) | `sniffCSS-diff` e `sniffCSS-check`. |
| [`docs/accessibility.md`](docs/accessibility.md) | Auditoria de acessibilidade. |

## Licença

[CC0 1.0 Universal](LICENSE) (domínio público) — uso livre, sem atribuição.
