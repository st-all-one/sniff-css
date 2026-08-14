# Guia de uso — otimizado para IA

Este guia mostra como usar `sniffCSS` + `sniffCSS-diff` + `sniffCSS-check` do
jeito certo quando o consumidor final é um modelo de IA (agente, MCP, pipeline
de regressão). Para a referência completa de flags da CLI, veja
[`usage.md`](usage.md).

## Filosofia: a IA deve receber só o delta

```
captura determinística ──► diff determinístico ──► checks determinísticos ──► IA (só interpreta)
  sniffCSS       sniffCSS-diff              sniffCSS-check / sniffCSS_check      eval-prompt
```

A extração, o diff e os checks são **sem IA** e custam ~0 tokens. O LLM só vê o
delta (medido: ~79% menos tokens que os snapshots completos).

## 1. Capture — o default já é otimizado

Para uso com IA, o **default** do `sniffCSS` já é o conjunto otimizado: você só
precisa de `-u` + `-s` (e, se houver, `--stable-key`). Os 5 otimizadores abaixo
vêm ligados; controle fino opcional (`--full` desliga os 5 de uma vez, `--no-*`
individual). Flags completas: [`usage.md`](usage.md).

| Otimização | Status | Por quê |
|---|---|---|
| `compact` | default ON | Dedup lógico/físico + supressão de defaults + `css_variables` escopado → ~55% menos tokens; hoist de props constantes para `__meta.style_defaults`. |
| `custom-props` | default ON | Captura as variáveis CSS (`--*`); global no `__meta`, overrides por nó. |
| `stabilize` | default ON | Congela `animation`/`transition` → snapshot determinístico. |
| `contrast` | default ON | Facet `contrast` medido por nó (fundo efetivo resolvido in-page). |
| `ax` | default ON | Facet `ax` por nó (árvore de acessibilidade do Chrome). |

Interações para revelar elementos dependentes de ação (modais, dropdowns, menus
de hover, sugestões, uploads) — cada ação espera o próprio alvo, rola até o
centro e dispara um **evento confiável**; o pipeline de waits roda depois contra
o DOM pós-interação e o `--stabilize` é reaplicado para determinismo:

| Flag | Efeito |
|---|---|
| `--click sel[:timeout[:settle]]` / `--hover sel[:timeout[:settle]]` / `--type sel:text` / `--upload sel:file1,file2` | Atalhos por tipo, repetíveis. |
| `--action spec` | Forma **ordenada** p/ fluxos mistos: `click:<sel>[:t[:settle]]` · `hover:<sel>[:t[:settle]]` · `type:<sel>:<text>` · `upload:<sel>:<file1,file2>`. Cadeias (modal → mini-modal → input) funcionam passo a passo. |
| `--effects` / `--no-effects` | **default ON com ações** — mapa `__actions` por interação (o que apareceu/sumiu/mudou e onde; `no_effect` quando nada mudou). |
| `--effects-limit N` | Cap de elementos por lista em cada entrada `__actions` (default `10`). |

### Acesso a áreas restritas

| Flag | Efeito |
|---|---|
| `--header "Name: Value"` | Headers HTTP extras aplicados a **todo** request da sessão (`Network.setExtraHTTPHeaders`), ex. `X-CMS-AI-Token` para auth stateless de CMS. Repetível; `SNIFF_DEFAULT_HEADERS` (JSON) é mesclado antes, e `--header` explícito vence na colisão. |
| `--storage-state PATH` | Restaura estado de sessão persistido (cookies + `localStorage`, JSON storageState do Playwright) **antes** da navegação. Login prévio sobrevive a este capture. |
| `--save-storage-state PATH` | Exporta cookies + `localStorage` da origem atual ao fim do pipeline — um login por `actions` sobrevive a restarts. |

```bash
# header auth (sem token em URL / .env / proxy)
sniffCSS -u http://localhost:10011/cms -s "main" --header "X-CMS-AI-Token: <token>"
# login via ações → exporta estado → restaura nas próximas
sniffCSS -u "$URL/login" -s ".dashboard" \
  --type "#email:user@x.com" --type "#password:secret" \
  --click "button[type=submit]" --save-storage-state /tmp/state.json
sniffCSS -u "$URL/cms/dashboard" -s "main" --storage-state /tmp/state.json
```

### Volumetria, prova visual e atributos DOM

| Flag | Efeito |
|---|---|
| `--summary` (ou `--output summary`) | **Formato padrão.** Digest de 1 linha por nó (estrutura + `css` curado + `contrast` + `aria`); constantes globais em `__meta.style_defaults`. |
| `--no-summary` | Emite o snapshot completo não-sumarizado (`--output jsonl`) — use quando o output alimenta `sniffCSS-diff`/`sniffCSS-check`/jq. |
| `--screenshot PATH` (+ `--fullpage-screenshot`) | PNG do estado final (pós-stabilize, pós-interação) — prova visual. |
| `--persist` | Grava no layout do store MCP (`sniffCSS/[domain]/[UTC]-[path]-[selector].<ext>`, git-ignored). A saída continua no stdout. |
| `--attrs a,b` | Atributos DOM verbatim por nó sob `attrs` (ex.: validar `name` de forms); o diff compara `attrs` por chave. |

> **Conteúdo oculto por animação (WOW.js / scroll-reveal):** com `--stabilize`
> ON (default) a animação é cancelada e o `element-ready` nunca dispara. Capture
> com `--no-visible --wait "delay:3000"` (inclui o invisível + espera fixa) ou
> `--no-stabilize --wait "delay:3000"` (deixa a animação rodar). No MCP:
> `include_invisible:true` + `wait:["delay:3000"]`.

Campos derivados que a IA **não precisa inferir**:

- `is_user_noticeable` — `display_visible` (renderizado, **independente do
  viewport**: fora da dobra continua `true`) + `accessibility_grade`
  (`NONE`/`AA`/`AAA`).
- `computed_style_hash` — xxHash64 dos estilos efetivos; igual entre runs
  idênticos (mesmo modo), diferente quando algo mudou.
- `contrast` — ratio WCAG **medido** com o fundo efetivo resolvido in-page;
  `unknown` = fundo-imagem (revisão manual).
- `ax` — nó da árvore de acessibilidade do Chrome (`role`/`name`/`ignored`...).

> ⚠️ **Determinismo**: para diffar, use **o mesmo modo** nos dois lados
> (default+default, ou `--full`+`--full` — nunca misture). O hash e o conteúdo
> dependem do modo.

## 2. Diff determinístico (antes da IA)

```bash
sniffCSS-diff base.jsonl head.jsonl --tolerance 0.5 > delta.jsonl
```

- `CHANGED` → `changes` com `before`/`after` por propriedade (`styles`, `pseudo`,
  `aria`, `contrast`, `ax`, `rect`, `metrics`, `is_user_noticeable`).
- `ADDED`/`REMOVED` → `snapshot` completo do nó.
- `--tolerance 0.5` absorve jitter de subpixel; unidades diferentes (`16px` vs
  `16rem`) **nunca** são consideradas iguais.
- `--ignore-props transform,translate,opacity` — props voláteis não marcam o nó.
- `--no-structural` — suprime `ADDED`/`REMOVED` (feeds com contagem variável).
- `--stats-only` → `nodes: N -> M | changed/added/removed` (varredura em escala).

## 2b. Checks determinísticos (`sniffCSS-check`)

```bash
sniffCSS-check --input head.jsonl --uniform --tolerance 0.5   # o "card estranho"
sniffCSS-check --input head.jsonl --rules                     # PASS/WARN/FAIL
```

Saída JSONL com **evidência medida** (contraste AA/AAA, alvo 24×24, foco
visível, hidden-focusable, alt vazio em imagem grande) + `__check_summary`. O
resultado vira evidência para o `reason` da avaliação IA — a IA cita fatos
medidos, não chutes.

## 3. Avaliação semântica por IA

Só agora o LLM entra. Envie **apenas** `delta.jsonl` + o prompt de
[`eval-prompt.md`](eval-prompt.md). A resposta deve validar contra
[`sniffCSS-eval.schema.json`](sniffCSS-eval.schema.json):

```jsonc
{
  "page_url": "https://exemplo.com/checkout",  // ou flutter://<device> para apps Flutter
  "status": "REGRESSION_DETECTED",             // IMPROVEMENT | NEUTRAL | REGRESSION_DETECTED
  "score_change": -15,                          // -100..+100
  "summary": "O botão de checkout perdeu contraste e ficou inacessível.",
  "changes_evaluated": [
    {"node_selector":"button[data-testid=\"submit\"]","impact":"NEGATIVE",
     "category":"ACCESSIBILITY",
     "reason":"Contraste caiu de 4.5:1 para 2.1:1 após mudança de fundo.",
     "measured":{"contrast":{"ratio":2.1,"aa":"fail","aaa":"fail"}}}
  ]
}
```

`node_selector` reproduz o selector do delta (web ou Flutter, ex.
`FilledButton-[<'counter'>][0]`; para deltas de interação, `__actions[N]`).
`measured` usa apenas os nomes de campo que a ferramenta emite (contrast,
aria/ax, noticeability, geometry, uniformity, rule, action, flutter).

Validação mecânica antes de confiar na resposta:

```bash
jq -e 'has("status") and has("score_change") and (.changes_evaluated|length)>0' resposta.json
```

## 4. Padrões de uso por cenário

### Agente/MCP (servidor `sniffCSS-mcp`)

Exponha `sniffCSS-mcp` como servidor MCP (stdio) para agentes. O servidor
mantém um Chrome headless compartilhado e oferece 6 tools; os defaults já são
os otimizados (`compact`, `custom_props`, `stabilize`, `contrast`,
`include_ax`). Por padrão cada `sniffCSS_page` **persiste** o snapshot em
`sniffCSS/[domain]/[UTC]-[path]-[selector].jsonl` e responde o **digest
summary**; `return:"reference"` devolve só a linha `{"__sniff": {...}}`
(~200 tokens) e `return:"jsonl"` o JSONL completo inline (`persist:false`
desativa a gravação; `return:"reference"` exige persist). Durante a execução
envia `notifications/progress` por fase.

| Tool | Uso |
|---|---|
| `sniffCSS_page` | Captura (url, selector, depth, categories, compact, custom_props, stable_key, **attributes**, pseudo, wait, **actions**, viewport, format, stabilize, contrast, include_ax, ax_tree, **effects**, **effects_limit**, **include_invisible**, **exclude**, **min_width**, **min_height**, **screenshot**, **screenshot_full_page**, full, persist, return, **headers**, **storage_state**, **save_storage_state**). Para elementos que só existem após uma ação, passe `actions` (array **ordenado** de `{type, selector, text?, files?, timeout_ms?, settle_ms?}`); upload via `{"type":"upload","selector":"#file","files":["/tmp/x.png"]}` roda handlers reais. Com `actions`, o snapshot carrega a linha `__actions` (default ON; `effects:false` omite). |
| `sniffFlutter_page` | Captura a árvore de widgets de um app Flutter/Dart nativo (emulador/device, build **debug**) no **mesmo modelo JSONL** — (device, avd, project, target, attach, depth, selector, persist, return, screenshot, **viewport**, **actions**). `viewport` (`WxH`) aplica `adb shell wm size` e restaura; `actions` (mesmos `ActionInput` do web: `{type, selector, text?, timeout_ms?, settle_ms?}`) dirigem o app pela extensão Flutter Driver antes do freeze/extract — `type:"click"|"type"`; `hover`/`upload` falham (web-only) e exigem `enableFlutterDriverExtension()` no app. Retorna o `__sniff` handle por padrão; os mesmos `sniffCSS_diff`/`sniffCSS_check` funcionam por path. `return:"jsonl"` traz o snapshot inline. |
| `sniffCSS_snapshots` | Lista os snapshots persistidos (domain/target/path/created_at/size), novos primeiro; filtros `domain`, `target`, `limit`. Use para escolher o par base/head. |
| `sniffCSS_diff` | Diff determinístico — **base_path/head_path** (o modo otimizado) ou base_jsonl/head_jsonl, tolerance, ignore_props, ignore_structural → delta + `__diff_summary`. Quando os dois lados carregam `__actions`, eles também são comparados (`ACTION_CHANGED`/`ACTION_ADDED`/`ACTION_REMOVED` + `actions_changed`). |
| `sniffCSS_check` | Checks determinísticos offline — **path** ou jsonl, uniform, rules, tolerance → PASS/WARN/FAIL + outliers. Inclui a regra `occluded` (elemento **atrás** de outro que o cobre, por `rect` + `z-index`/ordem no DOM). |
| `sniffCSS_categories` | Categorias aceitas. |

**Defaults de equipe (env do servidor):** `SNIFF_DEFAULT_HEADERS` (headers
aplicados a **todo** request; `headers` por chamada sobrescreve por chave),
`SNIFF_STORAGE_STATE` (estado de sessão restaurado antes de toda navegação;
`storage_state` por chamada sobrescreve) e `SNIFF_BASE_URL` (prefixo para `url`
relativa, ex. `cms/dashboard` → `http://localhost:10011/cms/dashboard`).

> **Fluxo low-token (recomendado):** cada captura salva no disco. No CLI
> (preferido quando há shell), `sniffCSS ... --no-summary --persist` grava o
> snapshot completo e emite o digest summary; `sniffCSS-diff <base> <head>` e
> `sniffCSS-check --input <head>` leem os arquivos. No MCP, cada `sniffCSS_page`
> salva e responde o summary; depois `sniffCSS_diff base_path/head_path` e
> `sniffCSS_check path` leem os arquivos — o snapshot completo **nunca** entra
> no contexto do LLM (nem no retorno, nem nos argumentos).

Recursos embutidos: `sniffCSS://prompts/eval` (prompt),
`sniffCSS://schemas/eval` (schema) e `sniffCSS://guides/golden` — leia-os em vez
de copiar arquivos.

Config do Claude Desktop:

```json
{ "mcpServers": { "sniff": { "command": "sniffCSS-mcp" } } }
```

### Monitor de regressão (CI)

```bash
sniffCSS --url "$URL" --selector "$SEL" --no-summary --stable-key data-testid \
  > snapshots/base.jsonl
# ... no build seguinte ... (mesmos flags nos dois lados)
sniffCSS --url "$URL" --selector "$SEL" --no-summary --stable-key data-testid \
  > snapshots/head.jsonl
sniffCSS-diff snapshots/base.jsonl snapshots/head.jsonl --stats-only
# falha o job se changed/added/removed > limiar
```

> ⚠️ Use `--full` nos **dois** lados para full-fidelity — nunca misture default
> com `--full` (o hash e o conteúdo dependem do modo).

### Debug de um elemento pontual

```bash
sniffCSS -u http://localhost:3000 -s ".btn-primary" --categories visual,typography \
  | jq '{color:.css.color, font:.css."font-size"}'                  # summary (default)
sniffCSS -u http://localhost:3000 -s ".btn-primary" --no-summary --categories visual,typography \
  | jq '{color:.styles.visual.color, font:.styles.typography."font-size"}'  # full
```

## 5. Auditoria de acessibilidade

Workflow completo validado em produção (portais .gov.br) — capturas
estruturadas, regras determinísticas, leitura de facets e checklist de
julgamento: [`accessibility.md`](accessibility.md). O contraste é **medido**
(fundo efetivo resolvido in-page, independente da profundidade), a
perceptibilidade é **graduada** (`is_user_noticeable`) e as regras
`sniffCSS-check` são **determinísticas** — a IA não chuta cores nem adivinha se
algo está invisível.

```bash
sniffCSS -u "$URL" -s "body" --depth 5 --ax-tree > body.jsonl
sniffCSS-check --input main.jsonl   --rules    # contraste AA, target 24x24, foco, alt, hidden-focusable
sniffCSS-check --input body.jsonl   --uniform  # o "card estranho" entre irmãos
```

## 6. Boas práticas / armadilhas

1. **Mesma viewport** entre runs (default `1366x768`) — media queries e `%`/`vh`
   mudam e o diff acusa falso-positivo.
2. **Mesmo modo** (`--compact` dos dois lados) — o hash e o conteúdo dependem do modo.
3. **Âncora estável** — prefira `--stable-key data-testid`; `id` gerados
   (`react-aria-123`) quebram o match entre deploys.
4. **Tolerância** — comece com `--tolerance 0.5`; não use tolerância cega (ela
   também "engole" mudanças pequenas de verdade).
5. **Espere a página estabilizar** — use `--wait` (network-idle, element-ready,
   fonts-loaded) para capturar sempre no mesmo estado.
6. **Páginas dinâmicas (carrosséis, lazy-load)** — um elemento pode existir no
   load e **sumir depois**. Se o wait padrão falhar, capture a **subárvore
   estável** (`--selector footer --depth 2`) ou use `--wait delay:N`.
7. **Conecte no seu dev server** — `--connect http://127.0.0.1:9222` evita
   subir outro Chrome e captura exatamente o que você vê.
8. **Elementos que só existem após interação** — um alvo com `display:none`
   falha com timeout de `element-ready`. Use `--click`/`--hover`/`--type`/`--upload`
   (ou `--action`; no MCP, `actions`).
9. **Não use `--no-rect`/`--no-metrics` no pipeline de regressão** — `rect`/
   `is_user_noticeable` são parte valiosa do sinal de CLS/visibilidade.

## Referência rápida

| Ação | Comando |
|---|---|
| Capturar (default otimizado) | `sniffCSS -u URL -s SEL [flags]` |
| Snapshot completo (diff/check/jq) | `--no-summary` (ou `--output jsonl`) |
| Full-fidelity | `--full` nos dois lados |
| Revelar elementos por interação | `--click #open` · `--hover #menu` · `--type #q:shoes` · `--action click:#open` · `--upload #file:foto.png` |
| Mapa de efeito de UI (`__actions`) | automático com ações; `--no-effects` omite · `--effects-limit N` |
| Header auth / sessão | `--header "X: v"` · `--storage-state f` · `--save-storage-state f` |
| Auditoria a11y completa | [`accessibility.md`](accessibility.md) |
| Resumo de mudanças | `sniffCSS-diff base.jsonl head.jsonl --stats-only` |
| Ignorar props voláteis | `sniffCSS-diff ... --ignore-props transform,opacity` |
| Listas de contagem variável | `sniffCSS-diff ... --no-structural` |
| Delta completo | `sniffCSS-diff base.jsonl head.jsonl > delta.jsonl` |
| Checagens determinísticas | `sniffCSS-check --input snap.jsonl --uniform --rules` |
| Schema da resposta IA | [`sniffCSS-eval.schema.json`](sniffCSS-eval.schema.json) |
| Prompt de avaliação | [`eval-prompt.md`](eval-prompt.md) |
| Padrão ouro de execução | [`golden-run.md`](golden-run.md) |
