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

- Versão específica: `... install.sh | VERSION=v0.3.0 sh`
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

# Revelar elementos que só existem após uma ação (modal, dropdown, menu)
sniffCSS -u http://localhost:3000 -s ".modal" --click "#open-modal"
sniffCSS -u http://localhost:3000 -s ".search-results" --type "#q:shoes"

# Upload real em <input type=file> (até oculto) — dispara `change` de verdade,
# então handlers como o cropper de um CMS rodam; cadeia click→upload funciona
sniffCSS -u http://localhost:3000 -s ".cropper" \
  --action "click:#abrir-modal:5000:800" \
  --action "upload:#arquivo:/tmp/foto.jpg"

# Acesso a área restrita: header aplicado a todo request (sem token em URL)
sniffCSS -u http://localhost:10011/cms -s "main" \
  --header "X-CMS-AI-Token: <token>"
# ...ou configure uma vez e nunca repita:
# export SNIFF_DEFAULT_HEADERS='{"X-CMS-AI-Token":"<token>"}'

# Login persiste entre capturas (cookies + localStorage sobrevivem a restarts)
sniffCSS -u "$URL/login" -s ".dashboard" \
  --type "#email:user@x.com" --type "#password:secret" \
  --click "button[type=submit]" --save-storage-state /tmp/state.json
sniffCSS -u "$URL/cms/dashboard" -s "main" --storage-state /tmp/state.json

# Map what happened at the UI level: what appeared, where (on/off-screen,
# px from the action point), and whether the interaction did anything
sniffCSS -u http://localhost:3000 -s ".modal" --click "#open-modal" \
  | jq 'select(has("__actions")) | .__actions[0] | {effect, summary}'
# -> {"effect":"revealed","summary":"... 1 element(s) appeared · biggest: DIV on-screen — 12px from click"}

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

Configuração de equipe (uma vez no ambiente do servidor — o agente não repete
auth por chamada):
`SNIFF_DEFAULT_HEADERS='{"X-CMS-AI-Token":"<token>"}'` (headers aplicados a todo
request; `headers` por chamada sobrescreve por chave), `SNIFF_STORAGE_STATE`
(estado de sessão restaurado antes de toda navegação) e `SNIFF_BASE_URL`
(prefixo para URLs relativas, ex. `cms/dashboard`).

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
