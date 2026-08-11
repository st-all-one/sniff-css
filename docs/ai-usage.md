# Guia de uso — otimizado para IA

Este guia mostra como usar `sniff-computed-style` + `sniff-diff` do jeito certo
quando o consumidor final é um modelo de IA (agente, MCP, pipeline de regressão).

## Filosofia: a IA deve receber só o delta

```
sniff-computed-style  (extração determinística: a verdade exata)   ─┐
                                                                    ├─► sniff-diff (determinístico) ─► delta pequeno ─► LLM
sniff-computed-style  (segunda execução, mesmos parâmetros)       ─┘
```

A extração e o diff são **sem IA** e custam ~0 tokens. O LLM só vê o delta
(medido: ~79% menos tokens que os snapshots completos).

## 1. Flag‑set recomendado para captura

Para uso com IA, capture sempre com o mesmo conjunto de flags nas duas runs:

```bash
sniff-computed-style \
  --url "http://localhost:3000/checkout" \
  --selector ".checkout-form" \
  --depth 2 \
  --categories all \
  --compact \
  --custom-props \
  --stable-key data-testid
```

| Flag | Por quê |
|---|---|
| `--categories all` | Só os ~250 props do catálogo, nunca as ~400 do navegador. |
| `--compact` | Dedup lógico/físico + supressão de defaults + `css_variables` escopado → ~55% menos tokens. |
| `--custom-props` | Captura as variáveis CSS (`--*`); global no `__meta`, overrides por nó. |
| `--stable-key data-testid` | Seletores estáveis entre deploys → diff confiável. |
| `--stabilize` | Páginas animadas/carrosséis: congela `animation`/`transition` → snapshot determinístico. |
| `--depth N` | Subárvore controlada; `0` = só o elemento. |

Flags de a11y (opt-in, adicionam facetas medidas):

| Flag | Facet emitido |
|---|---|
| `--contrast` | `contrast` por nó: ratio WCAG **medido** + AA/AAA (normal vs. texto grande). O fundo efetivo é resolvido **na página** — o JS compõe camadas transparentes/semi-transparentes subindo até o canvas do `html`/`body`, independente da profundidade da captura. `unknown` apenas quando há **fundo-imagem** na cadeia (revisão manual). |
| `--ax` | `ax` por nó: nó da árvore de acessibilidade do Chrome (`role`/`name`/`focusable`/`ignored`/`level`...). |
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

> O `sniff-check` (ou a tool MCP `run_checks`) avalia o snapshot **sem IA**:
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
sniff-diff base.jsonl head.jsonl --tolerance 0.5 > delta.jsonl
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
sniff-diff base.jsonl head.jsonl --stats-only 2>&1
# nodes: 14 -> 14 | changed: 1 | added: 0 | removed: 0
```

## 2b. Descoberta determinística (`sniff-check`)

Avalia um snapshot **sem IA** e offline:

```bash
# o "card estranho": instâncias irmãs do mesmo selector que desviam da norma
sniff-check --input head.jsonl --uniform --tolerance 0.5

# regras derivadas (PASS/WARN/FAIL): contraste medido, alvo 24x24, foco, alt
sniff-check --input head.jsonl --rules
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
`docs/eval-prompt.md`. A resposta deve validar contra `docs/sniff-eval.schema.json`:

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

### Agente/MCP (servidor `sniff-mcp`)

Preferencial para agentes: exponha `sniff-mcp` como servidor MCP (stdio) em vez
de chamar o shell. O servidor mantém um Chrome headless compartilhado e oferece:

1. **`sniff_page`** — captura (args: url, selector, depth, categories, compact,
   custom_props, stable_key, pseudo, wait, viewport, format, stabilize,
   contrast, include_ax, ax_tree) e retorna JSONL.
   Durante a execução envia `notifications/progress` por fase:
   `acquiring browser slot` → `navigating` → `waiting` → `extracting` →
   `capturing accessibility tree` (se `--ax`) → `formatting N nodes`.
2. **`diff_snapshots`** — diff determinístico de dois JSONL inline
   (args: base_jsonl, head_jsonl, tolerance, ignore_props, ignore_structural)
   → delta + `__diff_summary`.
3. **`run_checks`** — checks determinísticos offline sobre um JSONL inline
   (args: jsonl, uniform, rules, tolerance) → PASS/WARN/FAIL + outliers.
4. **`list_categories`** — categorias aceitas.

Recursos embutidos: `sniff://prompts/eval` (prompt), `sniff://schemas/eval`
(schema) e `sniff://guides/golden` (padrão ouro de execução) — leia-os em vez
de copiar arquivos.

Config do Claude Desktop:

```json
{ "mcpServers": { "sniff": { "command": "sniff-mcp" } } }
```

### Monitor de regressão (CI)

```bash
# guarda o estado atual como base de comparação
sniff-computed-style --url "$URL" --selector "$SEL" --stable-key data-testid \
  --compact > snapshots/base.jsonl

# ... no build seguinte ...
sniff-computed-style --url "$URL" --selector "$SEL" --stable-key data-testid \
  --compact > snapshots/head.jsonl

sniff-diff snapshots/base.jsonl snapshots/head.jsonl --stats-only
# falha o job se changed/added/removed > limiar
```

### Debug de um elemento pontual

```bash
sniff-computed-style -u http://localhost:3000 -s ".btn-primary" \
  --categories visual,typography --compact \
  | jq '{color:.styles.visual.color, font:.styles.typography."font-size"}'
```

## 5. Auditoria de acessibilidade (workflow validado em produção)

Receita completa para auditar uma página, validada contra páginas reais
(portais .gov.br). O contraste, os grades de perceptibilidade e as regras
`sniff-check` tornam a análise **determinística** — a IA não precisa chutar
cores nem "adivinhar" se algo está invisível.

### Passo 1 — captura estruturada

Uma captura `body` ampla + capturas focadas nas regiões-chave. Como o contraste
é resolvido in-page (o JS sobe até o canvas), **a profundidade da captura não
afeta o contraste** — use `depth` para controlar o tamanho do output, não a
precisão.

```bash
# Visão estrutural (landmarks, headings, links, imgs) + contraste medido
sniff-computed-style -u "$URL" -s "body" --depth 5 --compact --contrast --ax-tree \
  > body.jsonl

# Regiões profundas demais para o body (menu, rodapé, formulários, carrossel)
sniff-computed-style -u "$URL" -s "nav"      --depth 4 --compact --contrast > nav.jsonl
sniff-computed-style -u "$URL" -s "footer"   --depth 6 --compact --contrast > footer.jsonl
sniff-computed-style -u "$URL" -s "main"     --depth 5 --compact --contrast > main.jsonl
sniff-computed-style -u "$URL" -s "form, #carouselExampleCaptions" --depth 3 --compact --contrast > forms.jsonl
```

### Passo 2 — regras determinísticas (sem IA)

```bash
sniff-check --input main.jsonl   --rules    # contraste AA, target 24x24, foco, alt, hidden-focusable
sniff-check --input body.jsonl   --uniform  # o "card estranho" entre irmãos
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
7. **Conecte no seu dev server** — `--connect ws://127.0.0.1:9222/devtools/browser/<id>`
   evita subir outro Chrome e captura exatamente o que você está vendo.
8. **Não use `--no-rect/--no-metrics` no pipeline de regressão** — `rect`/`is_user_noticeable`
   são parte valiosa do sinal de CLS/visibilidade.

## Referência rápida

| Ação | Comando |
|---|---|
| Capturar | `sniff-computed-style -u URL -s SEL [flags]` |
| Achatado | `--output jsonl-flat` |
| Estabilizar animações | `--stabilize` |
| A11y medida | `--contrast --ax` ou `--ax-tree` |
| Auditoria a11y completa | `docs/accessibility.md` |
| Resumo de mudanças | `sniff-diff base.jsonl head.jsonl --stats-only` |
| Ignorar props voláteis | `sniff-diff ... --ignore-props transform,opacity` |
| Listas de contagem variável | `sniff-diff ... --no-structural` |
| Delta completo | `sniff-diff base.jsonl head.jsonl > delta.jsonl` |
| Checagens determinísticas | `sniff-check --input snap.jsonl --uniform --rules` |
| Schema da resposta IA | `docs/sniff-eval.schema.json` |
| Prompt de avaliação | `docs/eval-prompt.md` |
| Padrão ouro de execução | `docs/golden-run.md` |
