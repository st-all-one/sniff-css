# Guia de uso — otimizado para IA

Este guia mostra como usar `sniffCSS` + `sniffCSS-diff` do jeito certo
quando o consumidor final é um modelo de IA (agente, MCP, pipeline de regressão).

## Filosofia: a IA deve receber só o delta

```
sniffCSS  (extração determinística: a verdade exata)   ─┐
                                                                    ├─► sniffCSS-diff (determinístico) ─► delta pequeno ─► LLM
sniffCSS  (segunda execução, mesmos parâmetros)       ─┘
```

A extração e o diff são **sem IA** e custam ~0 tokens. O LLM só vê o delta
(medido: ~79% menos tokens que os snapshots completos).

## 1. Capture — o default já é otimizado

Para uso com IA, o **default** do `sniffCSS` já é o conjunto otimizado: você só
precisa de `-u` + `-s` (e, se houver, `--stable-key`). Tudo abaixo vem ligado:

```bash
sniffCSS \
  --url "http://localhost:3000/checkout" \
  --selector ".checkout-form" \
  --depth 2
#   ^-- os 5 otimizadores abaixo já estão ON por padrão
```

| Otimização | Status | Por quê |
|---|---|---|
| `compact` | **default ON** | Dedup lógico/físico + supressão de defaults + `css_variables` escopado → ~55% menos tokens. |
| `custom-props` | **default ON** | Captura as variáveis CSS (`--*`); global no `__meta`, overrides por nó. |
| `stabilize` | **default ON** | Páginas animadas/carrosséis: congela `animation`/`transition` → snapshot determinístico. |
| `contrast` | **default ON** | Facet `contrast` medido por nó (fundo efetivo resolvido in-page). |
| `ax` | **default ON** | Facet `ax` por nó (árvore de acessibilidade do Chrome). |

Controle fino **opcional** (sempre sobrepõe o default):

| Flag | Efeito |
|---|---|
| `--full` | Desliga os 5 de uma vez (full-fidelity, comportamento antigo). |
| `--no-compact` | Mantém todas as propriedades, sem dedup. |
| `--no-custom-props` | Omitir variáveis CSS. |
| `--no-stabilize` | Capturar o estado animado real (não congelado). |
| `--no-contrast` | Omitir o facet `contrast`. |
| `--no-ax` | Omitir o facet `ax`. |
| `--stable-key data-testid` | Seletores estáveis entre deploys → diff confiável. |
| `--depth N` | Subárvore controlada; `0` = só o elemento. |

Interações para revelar elementos dependentes de ação (modais, dropdowns,
menus de hover, sugestões de busca) — cada ação espera o próprio alvo, rola até
o centro e dispara um evento confiável; o pipeline de waits roda depois contra
o DOM pós-interação e o `--stabilize` é reaplicado para determinismo:

| Flag | Efeito |
|---|---|
| `--click sel[:timeout[:settle]]` | Clique real no centro de `sel` (modais, dropdowns). Repetível. |
| `--hover sel[:timeout[:settle]]` | `mouseMoved` para `sel` (menus de `:hover`). Repetível. |
| `--type sel:text` | Foca `sel` e digita `text` (type-ahead). Repetível. |
| `--upload sel:file1,file2` | Anexa arquivos locais a um `<input type=file>` (`DOM.setFileInputFiles`) — funciona em inputs ocultos e o browser dispara `change` sozinho, então uploads reais (ex. cropper do CMS) rodam. Repetível. |
| `--action spec` | Forma ordenada p/ fluxos mistos: `click:<sel>[:t[:settle]]` · `hover:<sel>[:t[:settle]]` · `type:<sel>:<text>` · `upload:<sel>:<file1,file2>`. Repetível. |
| `--effects` / `--no-effects` | **default ON com ações** — mapa `__actions` por interação (o que apareceu/sumiu/mudou e onde; `no_effect` quando nada mudou). |
| `--effects-limit N` | Cap de elementos por lista em cada entrada `__actions` (em `changed`, semânticas primeiro; default `10`). |

> Com `--action` ordenado, cadeias funcionam naturalmente (modal → mini-modal →
> input): cada passo espera o próprio alvo e gera a própria entrada em
> `__actions`, com o before = estado do passo anterior. Em passo quebrado, o
> erro nomeia o índice, o seletor e os passos anteriores.

### Acesso a áreas restritas (novo)

| Flag | Efeito |
|---|---|
| `--header "Name: Value"` | Headers HTTP extras aplicados a **todo** request da sessão (`Network.setExtraHTTPHeaders`), ex. `--header "X-CMS-AI-Token: <token>"` para auth stateless de CMS. Repetível; `SNIFF_DEFAULT_HEADERS` (JSON) é mesclado antes, e `--header` explícito vence na colisão. |
| `--storage-state PATH` | Restaura estado de sessão persistido (cookies + `localStorage`, JSON storageState do Playwright) **antes** da navegação: cookies via `Network.setCookies`, `localStorage` via script que roda antes dos scripts da página. Login prévio sobrevive a este capture. |
| `--save-storage-state PATH` | Exporta cookies + `localStorage` da origem atual ao fim do pipeline — faça login por `actions` numa captura e reutilize o arquivo em `--storage-state` nas seguintes (sobrevive a restarts do browser). |

### Controle de volumetria, prova visual e atributos DOM (novo)

| Flag | Efeito |
|---|---|
| `--summary` (ou `--output summary`/`slim`) | **Formato padrão.** Digest intermediário de 1 linha por nó: esqueleto estrutural (`tag/selector/path/depth/rect/visible/grade`) + `css` curado (display, position, cores, font-size/weight, overflow, z-index) + `contrast` (`ratio`/`aa`/`aaa`) + `aria` (`role`/`name`/`focusable`). Constantes globais em `__meta.style_defaults`. ~5-10x menor que o full; responde cor/fonte/contraste/aria sem o JSONL completo. |
| `--no-summary` | Emite o snapshot completo não-sumarizado (`--output jsonl`) em vez do digest padrão — use quando o output alimenta `sniffCSS-diff`/`sniffCSS-check`/jq. |
| `--screenshot PATH` | PNG da página no estado final (pós-stabilize, pós-interação) — prova visual complementar ao snapshot calculado. |
| `--fullpage-screenshot` | Com `--screenshot`: documento inteiro, não só a viewport. |
| `--persist` | Grava o snapshot no mesmo layout do store MCP (`sniffCSS/[domain]/[UTC]-[path]-[selector].<ext>`, no formato de `--output`), com `.gitignore` `*` na raiz — a árvore fica fora do git. A saída continua no stdout. |
| `--attrs a,b` | Captura atributos DOM verbatim por nó sob `attrs` (ex.: `--attrs name` para validar `name="parameters[items][0][title]"` de forms) — o diff compara `attrs` por chave. |
| compact (padrão) | Além de suprimir defaults, hoist de **props constantes** para `__meta.style_defaults` — props idênticas em todos os nós saem uma vez e são omitidas por nó (~50-80% menos JSONL, 80% dos bytes de estilos). `sniffCSS-diff`/`check` mesclam de volta, então mudanças de página inteira (ex.: `font-family`) continuam sendo detectadas. |

> **Conteúdo oculto por animação (WOW.js / scroll-reveal):** essas bibliotecas
> deixam `visibility:hidden` até animar; com `--stabilize` ON (default) a
> animação é cancelada e o elemento nunca fica visível, então o `element-ready`
> default (visible+has-size) nunca dispara. Capture com
> `--no-visible --wait "delay:3000"` (inclui o invisível + espera fixa) ou
> `--no-stabilize --wait "delay:3000"` (deixa a animação rodar). No MCP:
> `include_invisible:true` + `wait:["delay:3000"]`.

Flags de a11y extras (continuam opt-in, adicionam facetas):

| Flag | Facet emitido |
|---|---|
| `--ax-tree` | Linha `__ax_tree` com a subárvore AX completa dos elementos casados (implica `--ax`). |

Saída (padrão `jsonl`, árvore aninhada):

```jsonc
{"__meta":{"css_variables":{/* global :root, uma única vez */}}}
{"id":1,"tag":"DIV","selector":"div[data-testid=\"form\"]",
 "path":"div[data-testid=\"form\"]","depth":0,
 "is_user_noticeable":{"display_visible":true,"accessibility_grade":"AAA"},
 "computed_style_hash":"afbd33ba764bb8d4",
 "aria":{"role":"form","name":"","focusable":false,"has_text":false},
 "rect":{...},"metrics":{...},
 "styles":{"box_model":{...},"visual":{...}, ...},
 "children":[/* aninhado */]}
```

> O `sniffCSS-check` (ou a tool MCP `sniffCSS_check`) avalia o snapshot **sem IA**:
> `--uniform` acha o "card estranho" entre instâncias irmãs; `--rules` emite
> PASS/WARN/FAIL para contraste, alvo 24x24, indicador de foco, focusables
> ocultos e alt vazio em imagens grandes.

Campos derivados que a IA **não precisa inferir**:

- `is_user_noticeable` — divide o antigo `is_visible` em dois eixos:
  - `display_visible` — o elemento **está renderizado** (`display`≠`none`,
    `visibility`≠`hidden`, `opacity`>0, tamanho ≠ 0). **Independente do
    viewport**: conteúdo rolado para fora da tela (rodapé, conteúdo abaixo da
    dobra) continua `display_visible:true`.
  - `accessibility_grade` — `NONE` (não exposto à AT: `aria-hidden`,
    `hidden`/`inert`, `display:none`, zero-size), `AA` (exposto, mas fora da
    tela/transparente/sem nome acessível) ou `AAA` (na tela, exposto e com
    nome quando o role exige).
- `computed_style_hash` — xxHash64 dos estilos efetivos; igual entre runs
  idênticos (mesmo modo), diferente quando algo mudou.
- `id` / `parent_id` — pre-order; use `jsonl-flat` quando quiser um nó por linha.

> ⚠️ **Determinismo**: para diffar, use **o mesmo modo** nos dois lados
> (`--compact` + `--compact`, ou full + full). O hash e o conteúdo dependem do modo.

## 2. Diff determinístico (antes da IA)

```bash
sniffCSS-diff base.jsonl head.jsonl --tolerance 0.5 > delta.jsonl
```

O que sai: só os nós que mudaram.

- `CHANGED` → `changes` com `before`/`after` por propriedade (`styles`, `pseudo`,
  `aria`, `contrast`, `ax`, `rect`, `metrics`, `is_user_noticeable`).
- `ADDED`/`REMOVED` → `snapshot` completo do nó.
- `--tolerance 0.5` absorve jitter de subpixel (`16px`→`16.2px`); unidades
  diferentes (`16px` vs `16rem`) **nunca** são consideradas iguais.
- `--ignore-props transform,translate,opacity` — props voláteis/animadas não
  marcam o nó como changed (uso com `--stabilize`).
- `--no-structural` — suprime `ADDED`/`REMOVED` (reporta só `CHANGED`); certo
  para feeds com contagem de itens variável.
- `--stats-only` → só `nodes: N -> M | changed/added/removed` (varredura em escala).

```bash
# resumo para centenas de páginas
sniffCSS-diff base.jsonl head.jsonl --stats-only 2>&1
# nodes: 14 -> 14 | changed: 1 | added: 0 | removed: 0
```

## 2b. Descoberta determinística (`sniffCSS-check`)

Avalia um snapshot **sem IA** e offline:

```bash
# o "card estranho": instâncias irmãs do mesmo selector que desviam da norma
sniffCSS-check --input head.jsonl --uniform --tolerance 0.5

# regras derivadas (PASS/WARN/FAIL): contraste medido, alvo 24x24, foco, alt
sniffCSS-check --input head.jsonl --rules
```

Saída JSONL com evidência medida:

```jsonc
{"check":"contrast-aa","selector":"footer .text","tag":"P","status":"fail",
 "evidence":"ratio 2.1:1 on #212529 against #020842 (need 4.5:1 text AA)"}
{"check":"uniformity","selector":"div.card:nth-child(3)","status":"fail",
 "evidence":"deviates from the 3/3 group norm: box_model.height: 80px (norm 120px ±40.00)"}
{"__check_summary":{"uniformity_instances":3,"uniformity_outliers":1,"rules":12}}
```

O resultado vira **evidência** para o `reason` da avaliação IA (etapa 3) —
a IA cita fatos medidos, não chutes.

Exemplo de linha de delta:

```jsonc
{"status":"CHANGED",
 "selector":"button[data-testid=\"submit\"]",
 "tag":"BUTTON","depth":1,
 "changes":{"styles":{"visual":{"background-color":
   {"before":"#2563eb","after":"#16a34a"}}},
            "rect":{"before":{"x":141.8,...},"after":{"x":121.1,...}}}}
```

## 3. Avaliação semântica por IA

Só agora o LLM entra. Envie **apenas** `delta.jsonl` + o prompt de
`docs/eval-prompt.md`. A resposta deve validar contra `docs/sniffCSS-eval.schema.json`:

```jsonc
{
  "page_url": "https://exemplo.com/checkout",
  "status": "REGRESSION_DETECTED",   // IMPROVEMENT | NEUTRAL | REGRESSION_DETECTED
  "score_change": -15,               // -100..+100
  "summary": "O botão de checkout perdeu contraste e ficou inacessível.",
  "changes_evaluated": [
    {"node_selector":"button[data-testid=\"submit\"]","impact":"NEGATIVE",
     "category":"ACCESSIBILITY",
     "reason":"Contraste caiu de 4.5:1 para 2.1:1 após mudança de fundo."}
  ]
}
```

Validação mecânica antes de confiar na resposta:

```bash
jq -e 'has("status") and has("score_change") and (.changes_evaluated|length)>0' resposta.json
```

## 4. Padrões de uso por cenário

### Agente/MCP (servidor `sniffCSS-mcp`)

Preferencial para agentes: exponha `sniffCSS-mcp` como servidor MCP (stdio) em vez
de chamar o shell. O servidor mantém um Chrome headless compartilhado e oferece:

1. **`sniffCSS_page`** — captura (args: url, selector, depth, categories, compact,
   custom_props, stable_key, **attributes**, pseudo, wait, actions, viewport,
   format, stabilize, contrast, include_ax, ax_tree, effects, effects_limit,
   **include_invisible**, **exclude**, **min_width**, **min_height**,
   **screenshot** (default `false`), **screenshot_full_page**, full, persist, return,
   **headers**, **storage_state**, **save_storage_state**).
   **Defaults de equipe (env do servidor):** `SNIFF_DEFAULT_HEADERS`
   (`{"X-CMS-AI-Token":"<token>"}`, mesclado em todo request; `headers` por chamada
   sobrescreve por chave), `SNIFF_STORAGE_STATE` (estado de sessão restaurado antes
   de toda navegação; `storage_state` por chamada sobrescreve) e `SNIFF_BASE_URL`
   (prefixo para `url` relativa, ex. `cms/dashboard` → `http://localhost:10011/cms/dashboard`).
   Os defaults já são os otimizados: `compact`, `custom_props`, `stabilize`,
   `contrast` e `include_ax` vêm ligados — passe `full:true` para full-fidelity
   ou qualquer flag como `false` para desligar individualmente.
   Para elementos que só existem após uma ação, passe `actions` (array **ordenado**
   de `{type: "click"|"hover"|"type"|"upload", selector, text?, files?, timeout_ms?,
   settle_ms?}`); cada ação espera o próprio alvo, interage de verdade (evento
   confiável via `Input.dispatchMouseEvent`/`Input.insertText`, ou
   `DOM.setFileInputFiles` para `upload`) e o pipeline de waits roda depois
   contra o DOM pós-interação. Ex.: `[{"type":"click","selector":"#open-modal"}]`
   abre um modal antes de capturar o `.modal`; `{"type":"upload","selector":"#file",
   "files":["/tmp/x.png"]}` sobe um arquivo num `<input type=file>` (inclusive
   oculto) e o cropper/handler real roda. Cadeias (modal → mini-modal → input)
   listam cada passo em ordem. Com `actions` setado, o snapshot carrega a
   linha `__actions` (default ON; `effects:false` omite): **por passo**, o que
   apareceu/sumiu/mudou de estilo e **onde** — rect, on-screen, offset
   fora-da-viewport, distância do ponto da ação — além de `no_effect` quando a
   interação não mudou nada. O bloco é **orientado a tokens**: a assinatura CSS
   de ~38 props sai uma vez (`css_keys`) e os elementos aparecem como arrays
   (`css_after_values`/`css_before_values`); `changed` carrega `css_diff` só
   com as props que mudaram (com tolerância numérica) e `before`/`after`;
   reflow de raiz (`html`/`body` — scrollbar/altura/padding) é suprimido.
   Leia primeiro `effect` e `summary`, depois os elementos listados.
    **Novo (0.3):** `include_invisible:true` + `wait:["delay:..."]` captura
    conteúdo oculto por animação (WOW.js); `attributes:["name"]` traz atributos
    DOM verbatim sob `attrs` (valida reindexação de forms sem curl);
    `screenshot:true` (+ `screenshot_full_page`) persiste um PNG junto do snapshot
    e devolve `screenshot_path` no `__sniff`.
    **Novo (auth/sessão):** `headers:{"X-CMS-AI-Token":"<token>"}` autentica
    áreas restritas (aplicado a todo request via `Network.setExtraHTTPHeaders`);
    `storage_state`/`save_storage_state` restauram/exportam cookies+`localStorage`
    (um login por `actions` sobrevive a restarts).
    Por padrão **persiste** o snapshot em
    `sniffCSS/[domain]/[UTC]-[path]-[selector].jsonl` (raiz via `SNIFF_SNAPSHOT_DIR`)
    e responde com o **digest summary** (estrutura + `css` + `contrast` + `aria`);
    use `return:"reference"` para só a linha `{"__sniff": {path, url, selector, nodes}}`
    (~200 tokens) e `return:"jsonl"` para o JSONL completo inline
    (`persist:false` desativa a gravação; `return:"reference"` exige persist).
    Durante a execução envia `notifications/progress` por fase:
    `acquiring browser slot` → `navigating` → `performing interactions
    (click/hover/type)` (se `actions`) → `waiting` → `extracting` →
    `capturing accessibility tree` (se `include_ax`/`ax_tree`) → `formatting N nodes`.
2. **`sniffCSS_snapshots`** — lista os snapshots persistidos (domain/target/path/
   created_at/size), novos primeiro; filtros `domain`, `target`, `limit`. Use
   para escolher o par base/head.
3. **`sniffCSS_diff`** — diff determinístico de dois snapshots
   (args: **base_path/head_path** — o modo otimizado — ou base_jsonl/head_jsonl,
   tolerance, ignore_props, ignore_structural) → delta + `__diff_summary`.
   Quando os dois lados carregam `__actions`, eles também são comparados:
   deltas `ACTION_CHANGED`/`ACTION_ADDED`/`ACTION_REMOVED` (regressão de UI —
   ex.: `appeared[0].rect.y: 8 → 900`, `onscreen: true → false`,
   `effect: revealed → no_effect`) contados em `actions_changed`.
4. **`sniffCSS_check`** — checks determinísticos offline sobre um snapshot
   (args: **path** ou jsonl, uniform, rules, tolerance) → PASS/WARN/FAIL + outliers.
5. **`sniffCSS_categories`** — categorias aceitas.

> **Fluxo low-token (recomendado):** cada captura salva no disco. No CLI (preferido
> quando há shell), `sniffCSS ... --persist` grava o snapshot completo e emite o
> digest summary; `sniffCSS-diff <base> <head>` e `sniffCSS-check --input <head>`
> leem os arquivos. No MCP, cada `sniffCSS_page` salva e responde o summary;
> depois `sniffCSS_diff base_path/head_path` e `sniffCSS_check path` leem os
> arquivos — o snapshot completo **nunca** entra no contexto do LLM (nem no
> retorno, nem nos argumentos).

Recursos embutidos: `sniffCSS://prompts/eval` (prompt), `sniffCSS://schemas/eval`
(schema) e `sniffCSS://guides/golden` (padrão ouro de execução) — leia-os em vez
de copiar arquivos.

Config do Claude Desktop:

```json
{ "mcpServers": { "sniff": { "command": "sniffCSS-mcp" } } }
```

### Monitor de regressão (CI)

```bash
# guarda o estado atual como base de comparação
# (default otimizado; --stable-key é opcional mas recomendado)
sniffCSS --url "$URL" --selector "$SEL" --stable-key data-testid \
  > snapshots/base.jsonl

# ... no build seguinte ... (mesmos flags nos dois lados)
sniffCSS --url "$URL" --selector "$SEL" --stable-key data-testid \
  > snapshots/head.jsonl

sniffCSS-diff snapshots/base.jsonl snapshots/head.jsonl --stats-only
# falha o job se changed/added/removed > limiar
```

> ⚠️ Para full-fidelity, use `--full` nos **dois** lados — nunca misture
> default com `--full` (o hash e o conteúdo dependem do modo).

### Debug de um elemento pontual

```bash
sniffCSS -u http://localhost:3000 -s ".btn-primary" \
  --categories visual,typography \
  | jq '{color:.styles.visual.color, font:.styles.typography."font-size"}'
```

## 5. Auditoria de acessibilidade (workflow validado em produção)

Receita completa para auditar uma página, validada contra páginas reais
(portais .gov.br). O contraste, os grades de perceptibilidade e as regras
`sniffCSS-check` tornam a análise **determinística** — a IA não precisa chutar
cores nem "adivinhar" se algo está invisível.

### Passo 1 — captura estruturada

Uma captura `body` ampla + capturas focadas nas regiões-chave. Como o contraste
é resolvido in-page (o JS sobe até o canvas), **a profundidade da captura não
afeta o contraste** — use `depth` para controlar o tamanho do output, não a
precisão.

```bash
# Visão estrutural (landmarks, headings, links, imgs) + contraste medido.
# compact/contrast/ax já vêm ON por padrão; --ax-tree é o único opt-in aqui.
sniffCSS -u "$URL" -s "body" --depth 5 --ax-tree \
  > body.jsonl

# Regiões profundas demais para o body (menu, rodapé, formulários, carrossel)
sniffCSS -u "$URL" -s "nav"      --depth 4 > nav.jsonl
sniffCSS -u "$URL" -s "footer"   --depth 6 > footer.jsonl
sniffCSS -u "$URL" -s "main"     --depth 5 > main.jsonl
sniffCSS -u "$URL" -s "form, #carouselExampleCaptions" --depth 3 > forms.jsonl
```

### Passo 2 — regras determinísticas (sem IA)

```bash
sniffCSS-check --input main.jsonl   --rules    # contraste AA, target 24x24, foco, alt, hidden-focusable
sniffCSS-check --input body.jsonl   --uniform  # o "card estranho" entre irmãos
```

O `--rules` usa o **facet `contrast` medido pela engine** (com o fundo efetivo
resolvido) — `fail` é falha real de AA/AAA, `warn` é fundo-imagem (manual).

### Passo 3 — leia os facets, não inferencie

| Facet | O que responde |
|---|---|
| `aria.role` / `ax.role` | Semântica real (landmark, heading, link...) e estrutura (H1→H6). |
| `is_user_noticeable.display_visible` | Está **renderizado** (fora da dobra continua `true`). |
| `is_user_noticeable.accessibility_grade` | `NONE`=não exposto à AT · `AA`=exposto mas fora da tela/transparente/sem nome exigido · `AAA`=tudo ok. |
| `contrast.ratio` + `contrast.aa/aaa` | Contraste **medido** com o fundo efetivo. |
| `aria.name == ""` | Link/botão/img sem nome acessível (1.1.1/2.4.4). |
| `rect.width/height` | Alvo de toque (2.5.8: `< 24px`). |

### Passo 4 — checklist de julgamento IA

- [ ] Existe `<h1>`? Hierarquia não pula níveis (sem H1 ou H2→H4 é falha 1.3.1).
- [ ] Landmarks: `banner`/`navigation`/`main`/`contentinfo` presentes; skip-links.
- [ ] Links/botões/imgs **sem nome** (`aria.name` vazio) → 1.1.1 / 2.4.4 / 4.1.2.
- [ ] `contrast.aa == fail` → 1.4.3; `unknown` (fundo-imagem) → carrosséis/cards, revisar.
- [ ] Alvos `< 24px` (topbars, A+/A-, ícones) → 2.5.8.
- [ ] `accessibility_grade == NONE` em conteúdo de texto → verificar `aria-hidden`/`display:none` indevidos.
- [ ] Conteúdo "fora da dobra" não é falha de visibilidade — é `display_visible:true` + grade `AA`.

### Limitações conhecidas da ferramenta

- **Nome de link via `alt` de imagem interna**: um `<a>` que envolve
  `<img alt="...">` tem nome acessível real (o `alt`), mas o facet `aria.name`
  do link ainda sai vazio. Ao achar links "sem nome", verifique antes se há
  `img` filha com `alt` — só reporte falha se não houver.
- **Contraste sobre imagem de fundo**: qualquer fundo-imagem na cadeia vira
  `unknown` (honesto — não dá para medir sem a imagem). Avalie manualmente.
- **Carrosséis/abas**: conteúdo em painel oculto ainda está na AX tree (grade
  `AA`, `display_visible:true`) — não é "invisível", apenas fora da tela.

## 6. Boas práticas / armadilhas

1. **Mesma viewport** entre runs (default `1366x768`). Mudou o viewport? Media
   queries e valores `%`/`vh` mudam e o diff acusa falso-positivo.
2. **Mesmo modo** (`--compact` dos dois lados) — o hash e o conteúdo dependem do modo.
3. **Âncora estável** — prefira `--stable-key data-testid`; `id` gerados
   (`react-aria-123`) quebram o match entre deploys.
4. **Tolerância** — comece com `--tolerance 0.5`; suba se houver jitter de layout
   real (fontes não carregadas, animações). Não use tolerância cega: ela também
   "engole" mudanças pequenas de verdade.
5. **Espere a página estabilizar** — use `--wait` (network-idle, element-ready,
   fonts-loaded) para capturar sempre no mesmo estado; `document.fonts.ready`
   evita variação de métricas por troca de fonte.
6. **Páginas dinâmicas (carrosséis, lazy-load)** — um elemento pode existir no
   load e **sumir depois** (ex.: `.footer-widget-wrapper h3` → 4 H3s no load,
   0 após 500ms). Se o `element-ready`/`selector` padrão falhar com timeout,
   capture a **subárvore estável** (`selector=footer --depth 2`) ou use
   `--wait delay:N`. O primeiro slide oculto de um carrossel também faz o
   `element-ready` falhar mesmo com o conteúdo visível depois.
7. **Conecte no seu dev server** — `--connect http://127.0.0.1:9222` (origin HTTP é
   resolvido via `/json/version`; `ws://` direto também funciona) evita subir outro
   Chrome e captura exatamente o que você está vendo. No container Docker isso é o
   default via `SNIFF_CONNECT` (anexa ao Chromium da GUI).
8. **Elementos que só existem após interação** — um alvo com `display:none`
   (modal/dropdown/menu) falha com timeout de `element-ready` mesmo existindo no
   DOM. Use `--click "#open"` (ou `--hover`/`--type`, ou `--action` para fluxos
   ordenados; no MCP, `actions`): a ação revela o elemento e o pipeline de waits
   roda depois contra o DOM pós-interação.
9. **Não use `--no-rect/--no-metrics` no pipeline de regressão** — `rect`/`is_user_noticeable`
   são parte valiosa do sinal de CLS/visibilidade.

## Referência rápida

| Ação | Comando |
|---|---|
| Capturar (default otimizado) | `sniffCSS -u URL -s SEL [flags]` |
| Full-fidelity | `--full` |
| Desligar otimizações individuais | `--no-compact`, `--no-contrast`, `--no-ax`, `--no-stabilize`, `--no-custom-props` |
| Achatado | `--output jsonl-flat` |
| Estabilizar animações | já ON; desligue com `--no-stabilize` |
| A11y medida | já ON (`contrast` + `ax`); subárvore AX com `--ax-tree` |
| Revelar elementos por interação | `--click #open` · `--hover #menu` · `--type #q:shoes` · `--action click:#open` |
| Mapa de efeito de UI (`__actions`) | automático com ações; `--no-effects` omite · `--effects-limit N` |
| Auditoria a11y completa | `docs/accessibility.md` |
| Resumo de mudanças | `sniffCSS-diff base.jsonl head.jsonl --stats-only` |
| Ignorar props voláteis | `sniffCSS-diff ... --ignore-props transform,opacity` |
| Listas de contagem variável | `sniffCSS-diff ... --no-structural` |
| Delta completo | `sniffCSS-diff base.jsonl head.jsonl > delta.jsonl` |
| Checagens determinísticas | `sniffCSS-check --input snap.jsonl --uniform --rules` |
| Schema da resposta IA | `docs/sniffCSS-eval.schema.json` |
| Prompt de avaliação | `docs/eval-prompt.md` |
| Padrão ouro de execução | `docs/golden-run.md` |
