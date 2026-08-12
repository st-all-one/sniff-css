# Uso da CLI (`sniffCSS`)

Referência completa de captura. Para o pipeline de diff/checks e auditoria de
acessibilidade, veja [`diff-checks.md`](diff-checks.md) e
[`accessibility.md`](accessibility.md). Para uso orientado a IA,
[`ai-usage.md`](ai-usage.md).

## Uso rápido

```bash
# Computed styles do botão — o default já é otimizado para IA:
# compact + custom-props + stabilize + contrast + ax ligados.
sniffCSS --url http://localhost:3000 --selector ".btn-primary"

# Full-fidelity (todas as ~400 props do navegador, sem dedup, sem facets
# medidos) — comportamento antigo:
sniffCSS --url http://localhost:3000 --selector ".btn-primary" --full

# Apenas box-model + tipografia, com 1 nível de filhos (flags continuam
# funcionando e sobrepõem os defaults):
sniffCSS --url http://localhost:3000 --selector ".card" \
  --depth 1 --categories box-model,typography

# Espera o app carregar (flag JS), captura pseudo-elemento e normaliza cores
sniffCSS --url http://localhost:3000 --selector ".modal" \
  --wait app-flag:__APP_READY__:15000 --pseudo ::before

# Conectar num browser já aberto (Chrome com --remote-debugging-port=9222).
# Aceita ws:// direto OU um origin http:// que resolve via /json/version.
sniffCSS --url http://localhost:3000 --selector "header" \
  --connect http://127.0.0.1:9222

# A mesma coisa usando a variável de ambiente (padrão do container Docker).
SNIFF_CONNECT=http://127.0.0.1:9222 sniffCSS --url http://localhost:3000 \
  --selector "header"

# Revelar elementos que só existem após uma interação (modal, dropdown, menu):
# a ação roda antes da captura; o pipeline de waits roda depois do clique.
sniffCSS --url http://localhost:3000 --selector ".modal" \
  --click "#open-modal"
sniffCSS --url http://localhost:3000 --selector ".search-results" \
  --type "#q:shoes" --wait "network-idle:800:30000"
```

## Opções principais

> **O default é AI-otimizado**: `compact`, `custom-props`, `stabilize`,
> `contrast` e `ax` já vêm ligados. O output resultante traz o máximo de
> informação útil no mínimo de tokens. Use `--full` para o modo full-fidelity
> (tudo desligado de uma vez) ou os `--no-*` individuais para controle fino
> **opcional**.

| Opção | Descrição | Padrão |
|---|---|---|
| `-u, --url` | URL da página | obrigatório |
| `-s, --selector` | Seletor CSS | obrigatório |
| `--depth N` | Níveis de filhos (0 = só o elemento) | `0` |
| `-c, --categories` | Lista separada por vírgula | `all` |
| `--props a,b` | Propriedades custom adicionais | — |
| `--pseudo ::before,::after` | Pseudo-elementos | — |
| `--wait spec` | Estratégias de espera (repetível) | pipeline padrão |
| `--click sel[:t[:settle]]` | Clique real no centro de `sel` antes de capturar (repetível) — revela modais/dropdowns | — |
| `--hover sel[:t[:settle]]` | `mouseMoved` para `sel` antes de capturar (repetível) — revela menus de `:hover` | — |
| `--type sel:text` | Foca `sel` e digita `text` antes de capturar (repetível) — revela type-ahead | — |
| `--action spec` | Forma **ordenada** p/ fluxos mistos: `click:<sel>[:t[:settle]]` · `hover:<sel>[:t[:settle]]` · `type:<sel>:<text>` (repetível) | — |
| `--effects` | Emitir a linha `__actions` com o mapa de efeito de UI por interação (o que apareceu/sumiu/mudou e onde) | **`on` com ações** (use `--no-effects`) |
| `--effects-limit N` | Cap de elementos por lista em cada entrada `__actions` (em `changed`, mudanças semânticas vêm antes das de posição; demais listas por área) | `10` |
| `--no-visible` | Incluir elementos invisíveis | — |
| `--min-width px`, `--min-height px` | Filtro por tamanho | — |
| `--exclude sel` | Excluir seletores (repetível) | — |
| `--output jsonl\|jsonl-flat\|json` | Formato de saída | `jsonl` |
| `--compact` | Modo compacto (ver abaixo) | **`on`** (use `--no-compact`) |
| `--custom-props` | Capturar todas as variáveis CSS (`--*`) | **`on`** (use `--no-custom-props`) |
| `--stabilize` | Congelar animações/transições (snapshot determinístico) | **`on`** (use `--no-stabilize`) |
| `--contrast` | Facet `contrast` medido por nó (WCAG AA/AAA, fundo efetivo resolvido) | **`on`** (use `--no-contrast`) |
| `--ax` | Facet `ax` por nó (árvore de acessibilidade do Chrome) | **`on`** (use `--no-ax`) |
| `--ax-tree` | Emitir a subárvore AX completa como `__ax_tree` (implica `--ax`) | — (opt-in) |
| `--full` | Desliga todos os 5 otimizadores de uma vez (equivalente ao comportamento antigo) | `off` |
| `--no-visibility` | Omitir campo `is_user_noticeable` por nó | — |
| `--no-style-hash` | Omitir `computed_style_hash` por nó | — |
| `--no-aria` | Omitir o facet `aria` por nó (role/nome/focusable) | — |
| `--pretty` | JSON pretty | — |
| `--no-rect/--no-path/--no-metrics` | Omitir campos | — |
| `--no-normalize-colors` | Manter cores como o browser retorna | — |
| `--no-group` | Estilos achatados (sem categorias) | — |
| `--chrome PATH` | Binário do Chrome | autodetect |
| `--connect ENDPOINT` | Conectar em browser existente — `ws://...` direto, ou `http://host:port` / `host:port` que resolve via `/json/version` | env `SNIFF_CONNECT` |
| `--viewport WxH` | Viewport emulado (afeta `%`, `vh`, media queries) | `1366x768` |
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

### Ações de interação

Cada ação espera o próprio alvo aparecer, rola até o centro e dispara um
**evento confiável** (`Input.dispatchMouseEvent` para click/hover,
`Input.insertText` para type) — não um `el.click()` sintético, então
`:hover`/`:active`/`pointer`/`mouse`/`click` disparam de verdade. Com ações, o
pipeline de waits roda **depois** delas, contra o DOM pós-interação (ex.: o
`.modal` que o clique abriu), e o `--stabilize` é reaplicado para snapshots
determinísticos.

| Ação | Exemplo | Observação |
|---|---|---|
| `click` | `click:#open-modal:5000:200` | espera `#open-modal` (5s), clica, espera 200ms |
| `hover` | `hover:#user-menu` | move o ponteiro para o centro de `#user-menu` |
| `type` | `type:#q:shoes` | foca `#q` e insere `shoes` (anexa; não limpa o campo) |

> Os atalhos `--click`/`--hover`/`--type` aplicam nessa ordem de grupo
> (clicks → hovers → types). Para fluxos mistos **intercalados** (ex.: clicar,
> digitar, clicar num resultado), use `--action` que preserva a ordem exata.

### Mapa de efeito de UI (`__actions`)

Com ações configuradas, cada interação gera uma entrada na linha reservada
`__actions` do JSONL (default ON; `--no-effects` omite). A entrada compara um
snapshot de página inteira antes/depois da ação e responde **o que** mudou e
**onde**:

- `effect`: `revealed` / `hidden` / `changed` / `moved` / `no_effect` — o
  `no_effect` marca interação que não mudou nada (provável falha de lógica;
  suba `settle_ms`/`--wait` se o efeito for assíncrono).
- `appeared` / `removed` / `changed`: elementos com `tag`, `path`, `rect`,
  `onscreen`, `out_of_view.{above,below,left,right}` (px além de cada borda da
  viewport), `distance_from_action` e `direction` (posição relativa ao ponto da
  ação). A assinatura CSS é compacta e orientada a tokens:
  - `css_keys`: a lista de ~38 props visuais/layout emitida **uma vez por
    entrada**; `appeared`/`removed` carregam `css_after_values`/
    `css_before_values` (arrays de valores alinhados ao `css_keys`).
  - `changed` carrega `css_diff` — só as props que mudaram além da tolerância
    numérica, com `before`/`after` (ex.: `{"padding-top": {"before":"16px",
    "after":"26px"}}`).
  - Campos vazios/ausentes são omitidos; nós raiz (`html`/`body`) só reportam
    mudanças de tema/visual (reflow de scrollbar/altura/padding é suprimido);
    `changed` lista mudanças semânticas antes das de posição.
- `summary`: resumo determinístico (ex.: `"1 element(s) appeared · biggest:
  TABLE 1430px below — 2146px from click"`).

Exemplo:

```bash
sniffCSS --url "$URL" --selector ".modal" --click "#open" \
  | jq 'select(has("__actions")) | .__actions[0] | {effect, summary}'
```

O `sniffCSS-diff` compara os blocos `__actions` entre base/head quando ambos
existem, emitindo `ACTION_CHANGED`/`ACTION_ADDED`/`ACTION_REMOVED` (regressão
de UI) e somando em `actions_changed`.

### Solução de problemas

Pipeline padrão: `selector` (10s) → `network-idle` (30s) → `element-ready` (10s,
visible + has-size). Erros são claros:

- Seletor que nunca aparece → `no elements matched selector \`X\`` (falha rápida).
- Elemento presente mas sem satisfazer as condições (ex.: primeiro slide de um
  carrossel oculto) → timeout com dica (`try a delay:N wait or a longer
  element-ready timeout`).
- Elemento com `display:none` que só aparece após um clique → use `--click`/
  `--hover`/`--type` (ou `--action`); sem a ação, o `element-ready` falha com
  timeout porque o alvo existe mas nunca fica visível.
- Spec de wait malformado → mensagem com o formato esperado
  (`element-ready:<selector>:<cond1,cond2>[:<timeout_ms>]`).

> Em páginas dinâmicas (carrosséis, lazy-load), capture a subárvore estável
> (ex.: `selector=footer --depth 2`) ou use `--wait delay:N` em vez de depender
> do `element-ready` sobre elementos que aparecem e somem.

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
- `css_variables`: grupo extra com todas as variáveis CSS (`--*`) — emitido por
  padrão (use `--no-custom-props` para omitir).
- `id` / `parent_id`: todos os nós recebem identificadores (pre-order), permitindo achatamento/referência.
- `is_user_noticeable`: objeto que divide o antigo `is_visible` em dois eixos — `display_visible`
  (renderizado de fato: `display`≠`none`, `visibility`≠`hidden`, `opacity`>0, tamanho≠0;
  **independente do viewport**, então conteúdo fora da dobra continua `true`) e
  `accessibility_grade` (`NONE` = não exposto à AT; `AA` = exposto mas fora da tela/transparente/sem
  nome acessível; `AAA` = na tela, exposto e nomeado quando o role exige).
- `aria`: role (explícita ou inferida do tag), accessible name, `focusable`, `aria-*`, `has_text` — calculados na página.
- `contrast`: ratio WCAG **medido** + AA/AAA (normal vs. texto grande). O fundo efetivo é
  resolvido **na página**: o JS compõe camadas transparentes/semi-transparentes subindo até o
  canvas do `html`/`body` (independente da profundidade da captura) — fundos-imagem viram
  `unknown` para revisão manual.
- `ax`: nó da árvore de acessibilidade do Chrome (`role`/`name`/`focusable`/`ignored`/`level`...).
- `computed_style_hash`: checksum **xxHash64** (não criptográfico, ~40× mais rápido que SHA-1) dos estilos efetivos de cada nó (inclui pseudo-elementos). Permite **detectar mudanças entre execuções** comparando apenas o hash; determinístico entre runs no mesmo modo.
- Cores `rgb(...)`/`rgba(...)` são normalizadas para `#rrggbb`/`#rrggbbaa`.
- Use `--no-group` para um mapa plano de propriedades.

## Modo compacto (`--compact`)

**Ligado por padrão.** Otimizado para eficiência de tokens (redução típica de
~55%). Desligue com `--no-compact` ou `--full` para o conjunto completo de
propriedades:

1. **Deduplicação lógico/físico**: remove `*-block-*`/`*-inline-*`, `inset-*`,
   `grid-column-gap` etc. quando idênticos ao equivalente físico
   (`margin-top`, `top`, `column-gap`, ...).
2. **Supressão de defaults**: remove valores de ruído (`0px`, `none`, `normal`,
   `auto`, `100%`, ...) em propriedades não críticas; uma allowlist preserva
   propriedades sempre relevantes (`display`, `position`, `z-index`, cores, ...).
3. **`css_variables` escopado**: o mapa global de `:root` é emitido **uma única
   vez** numa linha `{"__meta":{"css_variables":{...}}}` (em ordem ordenada,
   determinística), e cada nó carrega apenas as variáveis que **sobrescrevem**
   localmente.

## Modo full-fidelity (`--full`)

Desliga todos os otimizadores de uma vez — equivalente ao comportamento
pré-AI-default. Útil quando você precisa do conjunto **completo** de
propriedades do navegador e das cores como o Chrome reporta:

```bash
sniffCSS --url http://localhost:3000 --selector ".card" --full
```

A mesma saída também pode ser obtida flag a flag:
`--no-compact --no-custom-props --no-stabilize --no-contrast --no-ax`.
O controle fino individual (`--no-*`) é **opcional** e sempre sobrepõe o default.

### Formatos

- `--output jsonl`: árvore aninhada, uma linha por elemento raiz.
- `--output jsonl-flat`: um nó por linha, com `id`/`parent_id` (achatado).
- `--output json`: array único (com `__meta` + `elements` no compact+custom-props).
- Com `--stable-key data-testid`, os seletores saem como `button[data-testid="submit"]`
  em vez de dependerem de `id` gerados (`react-aria-123`), mantendo o output
  **matchável entre deploys**.

## Docker

O container self-contained (Chromium + toolchain), quickstart, `docker-compose`
otimizado para integração em projetos, **MCP via Docker** e variáveis de
ambiente: veja [`docker.md`](docker.md).

## Distribuição e versões

- **Binários**: publicados no [GitHub Release](https://github.com/st-all-one/sniff-css/releases)
  vinculado à tag semver. O instalador oficial baixa o binário certo por
  OS/arquitetura, verifica o checksum e instala:

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf \
    https://raw.githubusercontent.com/st-all-one/sniff-css/main/install.sh | sh
  # versão específica:
  curl --proto '=https' --tlsv1.2 -sSf \
    https://raw.githubusercontent.com/st-all-one/sniff-css/main/install.sh \
    | VERSION=v0.1.0 sh
  ```

- **Arquiteturas**: Linux x86_64/aarch64 (glibc **e** musl estático — roda em
  qualquer distro, incluindo Alpine), macOS Apple Silicon (aarch64), Windows
  x86_64.
- **Imagem Docker**: `stallonels/sniffcss` (Docker Hub), multi-arch
  (linux/amd64 + linux/arm64), tag igual à do release (`latest` aponta para o
  último). Uso e integração em [`docker.md`](docker.md).
- **Releases futuros**: `git tag vX.Y.Z && git push origin vX.Y.Z` dispara o
  workflow `.github/workflows/release.yml` (build multi-arquitetura + GitHub
  Release + push Docker Hub). Configure os secrets `DOCKERHUB_USERNAME`/
  `DOCKERHUB_TOKEN` com `scripts/set-secrets.sh`.
