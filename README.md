# SniffCSS

**SniffCSS** captura o **computed style real** de elementos de uma página via
**Chrome DevTools Protocol (CDP)**, com saída estruturada e determinística para
desenvolvimento assistido por IA sem frameworks de automação — rápido, determinístico
e com saída otimizada para LLMs

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

# Evidência visual para review humana (PNG persistido ao lado do snapshot)
sniffCSS -u https://example.net -s ".btn-primary" --screenshot snap.png
```

Para o snapshot **completo** (entrada de diff/check/jq), use `--no-summary`.
Auth por header, login persistente (`--storage-state`), uploads reais e o mapa
de efeito `__actions`: [`docs/usage.md`](docs/usage.md).

## O que esta ferramenta faz?

**SniffCSS lê o estilo real renderizado pelo navegador** — não o que você
escreveu no CSS, mas o que o Chrome *computou*: box-model, tipografia, cores,
layout, `::before`/`::after`, variáveis e estado de acessibilidade. Para isso,
conecta-se a uma instância de Chrome/Chromium via **CDP** e emite um snapshot
estruturado (JSONL) de cada elemento.

A partir daí, um pipeline determinístico substitui o julgamento visual:

- **`sniffCSS`** captura o snapshot com um resumo otimizado para LLMs;
- **`sniffCSS-diff`** compara duas capturas e devolve **só o que mudou** (não o
  snapshot inteiro);
- **`sniffCSS-check`** avalia regras offline: contraste (WCAG), alvo de toque,
  foco, alt, uniformidade;
- **`sniffCSS-mcp`** expõe tudo como ferramentas MCP para agentes de IA.

O resultado: você (ou o agente) interpreta **evidências medidas** — como `contrast`
de `4.5:1` ou `background-color: #fff` — em vez de adivinhar pelo screenshot.

Além do estilo, cada snapshot carrega o **estado de acessibilidade** do elemento:
o papel na **árvore de acessibilidade** do navegador (`ax.role`), o nome exposto
(`aria.name`), e a nota computada pelo CDP (`accessibility_grade`: `AAA`/`AA`/
`NONE`) — para saber, por exemplo, se um `DIV` clicável é "invisível" para
leitores de tela ou se um `BUTTON` não tem nome acessível.

### Casos de uso

| Cenário | Como usar | Benefício |
|---|---|---|
| **Conferir um design** | `sniffCSS -u URL -s ".card" --depth 4` | Estilo exato renderizado (px, cor, fonte), sem inspecionar no DevTools |
| **Caçar uma regressão** | `sniffCSS-diff base.jsonl head.jsonl` | Só o que mudou: um `padding` que alterou 2px ou um card que sumiu |
| **Acessibilidade (WCAG)** | `sniffCSS-check --input snap.jsonl --rules` | Contraste real medido, alvo ≥24px, foco visível, alt — evidenciado |
| **Review humana** | `sniffCSS -u URL -s SEL --screenshot out.png` | PNG do elemento/página ao lado do snapshot — prova visual para PR/release |
| **UI que só existe após ação** | `sniffCSS -u URL -s ".modal" --click "#open"` | Modal/dropdown/upload capturado mesmo sem existir no DOM inicial |
| **Área restrita (auth)** | `sniffCSS -u URL/cms -s "main" --header "X-CMS-AI-Token: <token>"` | Snapshot da página autenticada, sem token exposto na URL |
| **Revisão de PR por IA** | MCP: captura → diff → checks → agente interpreta o delta | Revisão objetiva baseada em evidência, não em screenshot |

Em resumo: onde antes você **abria o DevTools manualmente**, descrevia screenshots
para a IA ou confiava no palpite, hoje o SniffCSS **mede e documenta** — o snapshot
é a fonte de verdade e a IA só interpreta o delta.

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

## Documentação

| Doc | Conteúdo |
|---|---|
| [`docs/usage.md`](docs/usage.md) | Referência completa da CLI. |
| [`docs/flutter.md`](docs/flutter.md) | Backend Flutter/Dart: instalação no Linux, modo `auto`, flags e resultado. |
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
