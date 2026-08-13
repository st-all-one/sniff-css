# SniffCSS

**SniffCSS** captura o **computed style real** de elementos de uma página via
**Chrome DevTools Protocol (CDP)**, com saída estruturada e determinística para
desenvolvimento assistido por IA sem frameworks de automação — rápido, determinístico
e com saída otimizada para LLMs

## Destaques

- **Interações reais antes da captura** — `--click`, `--hover`, `--type`,
  `--upload` e `--action` disparam **eventos confiáveis** (não `el.click()`
  sintético) para revelar elementos que só existem após uma ação: modais,
  dropdowns, menus de hover, type-ahead e uploads reais (cropper de CMS roda).
- **Acesso a áreas restritas** — `--header "Name: Value"` aplica o header a
  **todo** request (`Network.setExtraHTTPHeaders`), autenticando middleware
  stateless de CMS **sem token em URL**; no MCP, configure uma vez via
  `SNIFF_DEFAULT_HEADERS`.
- **Determinístico de ponta a ponta** — capture → `sniffCSS-diff` →
  `sniffCSS-check` → a IA interpreta só o delta (0 tokens antes do LLM).

## Instalação

```bash
curl --proto '=https' \
     --tlsv1.2 \
     --show-error \
     --fail \
  https://raw.githubusercontent.com/st-all-one/sniff-css/main/install.sh \
  | sh
```

> Requisito: Chrome/Chromium no sistema (ou `SNIFF_CHROME_PATH` / `--chrome`).

## Quickstart

```bash
# Computed styles de um botão — default otimizado para IA
sniffCSS -u https://example.net -s ".btn-primary"

# Subárvore, só box-model + tipografia
sniffCSS -u https://example.net -s ".card" \
  --depth 4 --categories box-model,typography

# Revelar elementos que só existem após uma ação (modal, dropdown, menu)
sniffCSS -u https://example.net -s ".modal" --click "#open-modal"
sniffCSS -u https://example.net -s ".search-results" --type "#q:shoes"

# Ações ordenadas (fluxo misto: clicar → digitar → selecionar)
sniffCSS -u https://example.net -s ".result" \
  --action "click:#open-modal:5000" \
  --action "type:#q:shoes" \
  --action "click:.result-item"

# Acesso a área restrita: header aplicado a todo request (sem token em URL)
sniffCSS -u https://example.net/cms -s "main" \
  --header "X-CMS-AI-Token: <token>"

# Diff determinístico entre duas versões
sniffCSS-diff base.jsonl head.jsonl --tolerance 0.5

# Checks offline: contraste AA, alvos ≥24px, foco, uniformidade
sniffCSS-check --input snap.jsonl --uniform --rules
```

Para o snapshot **completo** (entrada de diff/check/jq), use `--no-summary`.
Auth por header, login persistente (`--storage-state`), uploads reais e o mapa
de efeito `__actions`: [`docs/usage.md`](docs/usage.md).

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
| [`docs/ai-usage.md`](docs/ai-usage.md) | Guia para IA: pipeline captura→diff→checks. |
| [`docs/diff-checks.md`](docs/diff-checks.md) | `sniffCSS-diff` e `sniffCSS-check`. |
| [`docs/accessibility.md`](docs/accessibility.md) | Auditoria de acessibilidade. |
| [`docs/docker.md`](docs/docker.md) | Docker: quickstart, compose, MCP via container. |
| [`docs/golden-run.md`](docs/golden-run.md) | Contrato de determinismo (padrão ouro). |
| [`docs/eval-prompt.md`](docs/eval-prompt.md) | Prompt de avaliação semântica por IA. |
| [`docs/architecture.md`](docs/architecture.md) | Arquitetura interna. |
| [`SKILL.md`](SKILL.md) | Guia de uso ativo para agentes de IA. |
| [`llms.txt`](llms.txt) | Índice para modelos de linguagem. |

## Licença

[CC0 1.0 Universal](LICENSE) (domínio público) — uso livre, sem atribuição.
