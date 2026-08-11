# sniff-computed-style

Utilitário em Rust de alta performance para capturar o **computed style real** de elementos de uma página durante o desenvolvimento — em especial no localhost — com saída estruturada, compacta e otimizada para consumo por IA.

Fala diretamente com o navegador via **Chrome DevTools Protocol (CDP) raw sobre WebSocket** (sem dependência de frameworks de automação), o que garante máxima performance e flexibilidade.

## Funcionalidades

- **CDP raw** — conexão WebSocket própria, sem chromiumoxide/puppeteer.
- **Catálogo CSS completo** — ~250 propriedades padrão da web organizadas em 8 categorias semânticas.
- **`--depth N`** — recursão controlada; sem `--recursive`, `0` = só o elemento.
- **Estratégias de espera combináveis** — `selector`, `network-idle`, `element-ready`, `fonts-loaded`, `app-flag`, `delay`.
- **Filtros de elemento** — visibilidade, tamanho mínimo, seletores de exclusão.
- **Pseudo-elementos** — `::before`, `::after`, etc.
- **Saída JSONL** (padrão) ou JSON, com normalização de cores para hex e agrupamento por categoria.
- **Reuso de browser** — uma instância Chrome serve múltiplas consultas (watch/serve/loops).
- **Conecta em browser existente** (`--connect`) — use seu dev-server com remote debugging ativo.

## Build e instalação

```bash
cargo build --release
# binários: target/release/sniff-computed-style e target/release/sniff-diff
```

Instalar no computador (fica disponível para o host em `~/.local/bin`, com PATH automático):

```bash
scripts/install.sh
# re-instala sem recompilar:
scripts/install.sh --no-build
```

Instala três binários: `sniff-computed-style`, `sniff-diff` e `sniff-mcp`.

Requisito: Chrome/Chromium disponível no sistema (ou defina `SNIFF_CHROME_PATH` / use `--chrome`).

> Guia de uso **otimizado para IA**: [`docs/ai-usage.md`](docs/ai-usage.md) —
> combinação de flags, pipeline de diff e avaliação semântica passo a passo.

## Uso rápido

```bash
# Computed styles do botão (todas as categorias)
sniff-computed-style --url http://localhost:3000 --selector ".btn-primary"

# Apenas box-model + tipografia, com 1 nível de filhos
sniff-computed-style --url http://localhost:3000 --selector ".card" \
  --depth 1 --categories box-model,typography

# Espera o app carregar (flag JS), captura pseudo-elemento e normaliza cores
sniff-computed-style --url http://localhost:3000 --selector ".modal" \
  --wait app-flag:__APP_READY__:15000 --pseudo ::before

# Conectar num browser já aberto (Chrome com --remote-debugging-port=9222)
sniff-computed-style --url http://localhost:3000 --selector "header" \
  --connect ws://127.0.0.1:9222/devtools/browser/<id>
```

## Opções principais

| Opção | Descrição | Padrão |
|---|---|---|
| `-u, --url` | URL da página | obrigatório |
| `-s, --selector` | Seletor CSS | obrigatório |
| `--depth N` | Níveis de filhos (0 = só o elemento) | `0` |
| `-c, --categories` | Lista separada por vírgula | `all` |
| `--props a,b` | Propriedades custom adicionais | — |
| `--pseudo ::before,::after` | Pseudo-elementos | — |
| `--wait spec` | Estratégias de espera (repetível) | pipeline padrão |
| `--no-visible` | Incluir elementos invisíveis | — |
| `--min-width px`, `--min-height px` | Filtro por tamanho | — |
| `--exclude sel` | Excluir seletores (repetível) | — |
| `--output jsonl\|jsonl-flat\|json` | Formato de saída | `jsonl` |
| `--compact` | Modo compacto (ver abaixo) | — |
| `--no-visibility` | Omitir campo `is_visible` por nó | — |
| `--no-style-hash` | Omitir `computed_style_hash` por nó | — |
| `--pretty` | JSON pretty | — |
| `--no-rect/--no-path/--no-metrics` | Omitir campos | — |
| `--no-normalize-colors` | Manter cores como o browser retorna | — |
| `--no-group` | Estilos achatados (sem categorias) | — |
| `--chrome PATH` | Binário do Chrome | autodetect |
| `--connect WS` | Conectar em browser existente | — |
| `--viewport WxH` | Viewport emulado (afeta `%`, `vh`, media queries) | `1366x768` |
| `--custom-props` | Capturar todas as variáveis CSS (`--*`) | — |
| `--stable-key attr` | Atributo âncora nos `selector`/`path` (ex.: `data-testid`), preferido ao `id` | — |

### Categorias disponíveis

`box-model` · `layout` · `typography` · `visual` · `transform` · `animation` · `interaction` · `accessibility` · `all`

### Estratégias de espera

Formato: `nome:arg1:arg2[:timeout_ms]`

| Estratégia | Exemplo |
|---|---|
| `selector` | `selector:.card:30000` |
| `network-idle` | `network-idle:500:30000` |
| `element-ready` | `element-ready:.card:visible,has-size,opacity=0.9:30000` |
| `fonts-loaded` | `fonts-loaded:15000` |
| `app-flag` | `app-flag:__APP_READY__:15000` |
| `delay` | `delay:2000` |

Pipeline padrão: `selector` → `network-idle` → `element-ready` (visible + has-size).

## Formato de saída (JSONL)

Uma linha JSON por elemento raiz, com filhos aninhados. Nomes legíveis e compactos:

```json
{
  "tag": "DIV",
  "selector": "div#primary",
  "path": "body > main > div.card",
  "depth": 0,
  "rect": {"x": 8.0, "y": 8.0, "width": 300.0, "height": 56.0},
  "metrics": {"z_index": "auto", "stacking_context": false},
  "styles": {
    "box_model": {"width": "300px", "height": "56px", "padding": "16px", "box-sizing": "content-box"},
    "typography": {"font-family": "Inter, sans-serif", "font-size": "16px"},
    "visual": {"background-color": "#2563eb", "opacity": "1"},
    "layout": {"display": "inline-flex", "gap": "8px"}
  },  "children": [
    {"tag": "SPAN", "selector": "div#primary > span.icon:nth-child(1)", "depth": 1, "styles": {...}}
  ]
}
```

- `pseudo`: mapa de pseudo-elementos → mesmos grupos de estilos.
- `css_variables`: grupo extra com todas as variáveis CSS (`--*`) quando `--custom-props` é usado.
- `id` / `parent_id`: todos os nós recebem identificadores (pre-order), permitindo achatamento/referência.
- `is_visible`: booleano derivado na página a partir de `display`, `visibility`, `opacity` e `rect` — a IA não precisa inferir visibilidade.
- `computed_style_hash`: checksum **xxHash64** (não criptográfico, ~40× mais rápido que SHA-1) dos estilos efetivos de cada nó (inclui pseudo-elementos). Permite **detectar mudanças entre execuções** comparando apenas o hash; determinístico entre runs no mesmo modo.
- Cores `rgb(...)`/`rgba(...)` são normalizadas para `#rrggbb`/`#rrggbbaa`.
- Use `--no-group` para um mapa plano de propriedades.

## Modo compacto (`--compact`)

Otimizado para eficiência de tokens (redução típica de ~55%):

1. **Deduplicação lógico/físico**: remove `*-block-*`/`*-inline-*`, `inset-*`,
   `grid-column-gap` etc. quando idênticos ao equivalente físico
   (`margin-top`, `top`, `column-gap`, ...).
2. **Supressão de defaults**: remove valores de ruído (`0px`, `none`, `normal`,
   `auto`, `100%`, ...) em propriedades não críticas; uma allowlist preserva
   propriedades sempre relevantes (`display`, `position`, `z-index`, cores, ...).
3. **`css_variables` escopado**: o mapa global de `:root` é emitido **uma única
   vez** numa linha `{"__meta":{"css_variables":{...}}}`, e cada nó carrega apenas
   as variáveis que **sobrescrevem** localmente.

### Formatos

- `--output jsonl`: árvore aninhada, uma linha por elemento raiz.
- `--output jsonl-flat`: um nó por linha, com `id`/`parent_id` (achatado).
- `--output json`: array único (com `__meta` + `elements` no compact+custom-props).
- Com `--stable-key data-testid`, os seletores saem como `button[data-testid="submit"]`
  em vez de dependerem de `id` gerados (`react-aria-123`), mantendo o output
  **matchável entre deploys**.

## Pipeline de diff (sniff-diff)

O workspace inclui o binário `sniff-diff`: diff **determinístico** (sem IA) entre
dois snapshots JSONL, produzindo apenas os nós que mudaram. A IA avalia só o delta.

```bash
# 1. Extração em dois momentos, com âncora estável
sniff-computed-style -u URL -s ".widget" --stable-key data-testid > base.jsonl
# ... (deploy/tempo passa) ...
sniff-computed-style -u URL -s ".widget" --stable-key data-testid > head.jsonl

# 2. Diff determinístico
sniff-diff base.jsonl head.jsonl --tolerance 0.5 > delta.jsonl

# 3. Avaliação semântica por IA (opcional): mande SÓ o delta + prompt
#    docs/eval-prompt.md; resposta valida contra docs/sniff-eval.schema.json
```

Como funciona:

- **Match de nós** por `selector` estável, com fallback posicional (estrutura).
- **Comparação por propriedade** com tolerância numérica (`--tolerance`) para
  absorver jitter de subpixel (`16px` → `16.2px`); unidades diferentes nunca
  são tratadas como iguais.
- **Saída JSONL** com `CHANGED` (deltas `before`/`after` por propriedade, incluindo
  `styles`, `pseudo`, `rect`, `metrics`, `is_visible`) e `ADDED`/`REMOVED`
  (snapshot completo do nó).
- `--stats-only` imprime apenas o resumo (`nodes: N -> M | changed/added/removed`),
  ideal para varrer centenas de páginas sem gastar tokens.
- A avaliação positiva/negativa (contraste WCAG, CLS, design system) fica na
  camada de IA; o contrato está em `docs/sniff-eval.schema.json` e o template de
  prompt em `docs/eval-prompt.md`.

> Consulte [`docs/ai-usage.md`](docs/ai-usage.md) para o passo a passo completo
> de uso orientado a IA (flags recomendadas, padrões de agente/MCP, CI e armadilhas).

## Servidor MCP (sniff-mcp)

Exponha a captura e o diff como **ferramentas MCP** para agentes de IA (Claude
Desktop, VS Code Copilot, etc.), sem shell:

```bash
sniff-mcp   # serve MCP sobre stdio (um Chrome headless compartilhado)
```

Ferramentas:

| Tool | O que faz |
|---|---|
| `sniff_page` | Captura computed styles de uma página → JSONL (compact, stable-key, wait, viewport, format) |
| `diff_snapshots` | Diff determinístico de dois JSONL inline → delta (`CHANGED`/`ADDED`/`REMOVED` + `__diff_summary`) |
| `list_categories` | Categorias de propriedades disponíveis |

Recursos: `sniff://prompts/eval` e `sniff://schemas/eval`.

**Streaming assíncrono**: durante `sniff_page`, o servidor emite
`notifications/progress` por fase (`navigating` → `waiting` → `extracting` →
`formatting N nodes`), para a IA acompanhar o pipeline sem bloquear. O browser é
reutilizado (sem cold-start) e a concorrência é limitada por semáforo; se o Chrome
morrer, ele é relançado transparentemente.

Exemplo de config para o Claude Desktop (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "sniff": {
      "command": "sniff-mcp"
    }
  }
}
```

> Para um guia completo de uso via IA (fluxo captura → diff → avaliação), veja
> [`docs/ai-usage.md`](docs/ai-usage.md).

## Integração com IA

```bash
sniff-computed-style --url http://localhost:3000 --selector ".btn-primary" \
  | jq '.styles.box_model.width'          # extrai largura
sniff-computed-style --url http://localhost:3000 --selector "body" \
  --depth 3 --no-visible                  # árvore completa
```

Qualquer ferramenta pode chamar o binário e consumir o stdout. Para agentes via MCP/LangChain, exponha o binário como ferramenta de execução.

## Estrutura do projeto

```
crates/
├── sniff-core/     # tipos, config, catálogo de ~250 propriedades CSS
├── sniff-cdp/      # cliente CDP raw (WebSocket), protocolo, gestão do processo Chrome
├── sniff-engine/   # orquestração: espera, filtro, extração (1 chamada Runtime.evaluate), saída
├── sniff-cli/      # binário clap (sniff-computed-style)
├── sniff-diff/     # diff determinístico JSONL (binário sniff-diff) + delta p/ IA
└── sniff-mcp/      # servidor MCP (stdio): sniff_page, diff_snapshots, list_categories
```

## Testes

```bash
cargo test                    # unit tests (sem Chrome)
cargo test -p sniff-engine --test integration   # e2e com Chrome real (auto-skip se ausente)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## Licença

MIT
