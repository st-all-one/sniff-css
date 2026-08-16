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
| `--upload sel:file1,file2` | Anexa arquivos locais a um `<input type=file>` antes de capturar (repetível) — funciona em inputs ocultos; o browser dispara `change` sozinho, então handlers reais (cropper do CMS) rodam | — |
| `--action spec` | Forma **ordenada** p/ fluxos mistos: `click:<sel>[:t[:settle]]` · `hover:<sel>[:t[:settle]]` · `type:<sel>:<text>` · `upload:<sel>:<file1,file2>` (repetível) | — |
| `--header "Name: Value"` | Header HTTP extra aplicado a **todo** request da sessão (repetível), ex. `X-CMS-AI-Token` para auth stateless de CMS; `SNIFF_DEFAULT_HEADERS` (JSON) é mesclado antes, e `--header` explícito vence na colisão | — |
| `--storage-state PATH` | Restaura estado de sessão persistido (cookies + `localStorage`, JSON storageState do Playwright) **antes** da navegação — login prévio sobrevive a este capture | — |
| `--save-storage-state PATH` | Exporta cookies + `localStorage` da origem atual ao fim do pipeline — paire com `--storage-state` nas capturas seguintes para o login sobreviver a restarts | — |
| `--effects` | Emitir a linha `__actions` com o mapa de efeito de UI por interação (o que apareceu/sumiu/mudou e onde) | **`on` com ações** (use `--no-effects`) |
| `--effects-limit N` | Cap de elementos por lista em cada entrada `__actions` (em `changed`, mudanças semânticas vêm antes das de posição; demais listas por área) | `10` |
| `--no-visible` | Incluir elementos invisíveis | — |
| `--min-width px`, `--min-height px` | Filtro por tamanho | — |
| `--exclude sel` | Excluir seletores (repetível) | — |
| `--output jsonl\|jsonl-flat\|json\|summary` | Formato de saída | **`summary`** (digest intermediário) |
| `--summary` | Atalho para `--output summary` (o padrão): digest de 1 linha por nó (estrutura + `css` curado + `contrast` + `aria`) | `off` |
| `--no-summary` | Emitir o snapshot completo não-sumarizado (`--output jsonl`) em vez do digest padrão | `off` |
| `--screenshot PATH` | Salvar PNG da página no estado final (pós-stabilize, pós-interação) | — |
| `--fullpage-screenshot` | Com `--screenshot`: capturar o documento inteiro em vez da viewport | `off` |
| `--persist` | Espelha o store do MCP: grava o snapshot em `sniffCSS/[domain]/[UTC]-[path]-[selector].<ext>` no CWD (ou `SNIFF_SNAPSHOT_DIR`), no formato de `--output` selecionado; a pasta ganha um `.gitignore` com `*` e nunca é rastreada pelo git. A saída continua indo para o stdout. | `off` |
| `--attrs a,b` | Capturar atributos DOM verbatim por nó (`getAttribute`), repetível ou comma-separated; emitidos sob `attrs` (ex.: validar `name` de forms) | — |
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
| `--viewport WxH` | Viewport emulado: web usa `Emulation.setDeviceMetricsOverride` (afeta `%`, `vh`, media queries); Flutter aplica `adb shell wm size WxH` no device (afeta o `MediaQuery`/layout) e restaura ao final | `1366x768` (web) / device (flutter) |
| `--stable-key attr` | Atributo âncora nos `selector`/`path` (ex.: `data-testid`), preferido ao `id` | — |

### Backend Flutter/Dart (`--backend flutter`)

Captura a árvore de widgets de um app Flutter/Dart nativo em emulador/device
(análogo do sniff web, mas sobre o **Dart VM Service** em vez de CDP). O app
precisa estar em build **debug** (release não expõe o VM Service). O snapshot
sai no **mesmo modelo JSONL** e os mesmos `sniffCSS-diff`/`sniffCSS-check`
funcionam por path.

```bash
# Forma enxuta: o URL `flutter://<serial>` infere o backend e o device
# (`-s` vira `root` se omitido):
sniffCSS -u flutter://emulator-5554 --project ~/projetos/app --depth 2

# Equivalente explícito:
sniffCSS --backend flutter --device emulator-5554 --project ~/projetos/app \
  --target lib/main.dart --depth 2 -s root

# Anexar a um device/emulador já rodando um app debug (flutter attach):
sniffCSS -u flutter://emulator-5554 --attach --project ~/projetos/app --depth 2

# Lançar um AVD, rodar o app (flutter run --machine) e capturar:
sniffCSS -u flutter://pixel --avd pixel --project ~/projetos/app --depth 2

# Screenshot do device junto com a captura:
sniffCSS -u flutter://emulator-5554 --project ~/projetos/app --screenshot out.png

# Viewport do app (media queries/layout do Flutter) em runtime:
sniffCSS -u flutter://emulator-5554 --project ~/projetos/app --viewport 540x1200
```

> O `--backend` padrão é `auto`: um `--url flutter://<serial>` (ou
> `flutter://<serial>/<path>`) seleciona o backend Flutter e usa `<serial>`
> como device; qualquer outra URL usa o backend web. Flags explícitas
> (`--backend`, `--device`, `--avd`) sempre vencem a inferência.

| Opção | Descrição | Padrão |
|---|---|---|
| `--backend web\|flutter\|auto` | Backend de captura: `web` (Chromium/CDP), `flutter` (VM Service) ou `auto` (inferido de `--url`) | `auto` |
| `--device SERIAL` | Serial `adb` do emulador/device (ex.: `emulator-5554`); default: host do `flutter://<serial>` | — |
| `--avd NAME` | Lançar este AVD em vez de anexar a um device já rodando | — |
| `--project DIR` | Diretório do app (com `pubspec.yaml`); default: pai de `--target` que tem `pubspec.yaml` | — |
| `--target ENTRY` | Entry do app | `lib/main.dart` |
| `--attach` | Anexar a um app debug já rodando (`flutter attach`) em vez de `flutter run` | `off` |
| `--viewport WxH` | Tamanho lógico do app no device (`adb shell wm size`); restaurado ao final | device |
| `--click SEL[:t[:settle]]` | Toca o widget em `SEL` antes de capturar (repetível) — revela modais/dropdowns | — |
| `--type SEL:text` | Toca `SEL` (foca) e digita `text` antes de capturar (repetível) | — |
| `--action spec` | Forma **ordenada** p/ fluxos mistos (repetível): `click:<sel>[:t[:settle]]` · `type:<sel>:<text>` · `hover`/`upload` falham (web-only) | — |

Campos do nó: `tag` (classe do widget, ex. `Text`, `ElevatedButton`),
`selector`/`path` (breadcrumb de widgets, ex. `Center > Text[0]`), `rect`
(tamanho do render object + offset acumulado do `parentData`), `styles`
agrupadas (layout/typography/visual/box-model) com cores Flutter normalizadas
para `#rrggbb`, e o facet `contrast` derivado compartilhado. Antes da captura
as animações são congeladas (`ext.flutter.timeDilation`) para determinismo e
restauradas ao final (o app não fica travado). Com ações, o toque é feito pela
extensão **Flutter Driver** — o app precisa chamar `enableFlutterDriverExtension()`
em `main()` e ter `flutter_driver` em `dev_dependencies` (ver
[`docs/flutter.md`](flutter.md#51-ações-de-interação-tap--type)); sem isso a
ação falha com mensagem clara.
**Limitações:** apenas builds debug; `rect` em coordenadas do device (não
viewport CSS); widgets sem render box ficam sem `rect`.

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
`Input.insertText` para type, `DOM.setFileInputFiles` para upload) — não um
`el.click()` sintético, então `:hover`/`:active`/`pointer`/`mouse`/`click`
disparam de verdade. Com ações, o pipeline de waits roda **depois** delas,
contra o DOM pós-interação (ex.: o `.modal` que o clique abriu), e o
`--stabilize` é reaplicado para snapshots determinísticos. Cadeias (modal →
mini-modal → input) funcionam porque cada `prepare` espera o **próprio** alvo,
que pode só existir após o passo anterior.

| Ação | Exemplo | Observação |
|---|---|---|
| `click` | `click:#open-modal:5000:200` | espera `#open-modal` (5s), clica, espera 200ms |
| `hover` | `hover:#user-menu` | move o ponteiro para o centro de `#user-menu` |
| `type` | `type:#q:shoes` | foca `#q` e insere `shoes` (anexa; não limpa o campo) |
| `upload` | `upload:#file:/tmp/foto.jpg` | anexa o arquivo a `#file` (aceita input oculto `display:none`); o browser dispara `change` e o handler real roda |

> **Selectors com `:`** — em `click`/`hover` o selector pode conter dois-pontos
> (pseudo-classes CSS), e só os campos *finais* numéricos são interpretados como
> timeout/settle: `click:.btn-group:nth-child(2) .dropdown-toggle:3000` →
> selector `.btn-group:nth-child(2) .dropdown-toggle`, timeout 3000ms. Em
> `type`/`upload` o selector é o primeiro token após o nome (o texto/arquivos
> preservam `:`), então prefira um âncora sem dois-pontos (atributo/`aria-label`)
> quando o selector teria um.
> O caminho dos arquivos em `upload` é resolvido **pelo processo do browser** —
> em container, monte o arquivo lá dentro. O `prepare` do upload não exige
> visibilidade (inputs de arquivo costumam ser ocultos).

### Acesso a áreas restritas

Para um CMS com middleware stateless de IA, o token vai por header — o request
já nasce autenticado, sem token em URL, `.env` ou proxy:

```bash
sniffCSS -u http://localhost:10011/cms -s "main" \
  --header "X-CMS-AI-Token: <token>"
# configure uma vez no shell e nunca repita:
export SNIFF_DEFAULT_HEADERS='{"X-CMS-AI-Token":"<token>"}'
```

Login por formulário persiste entre capturas (cookies + `localStorage`
sobrevivem a restarts do browser):

```bash
# 1. faça login via ações e exporte o estado
sniffCSS -u "$URL/login" -s ".dashboard" \
  --type "#email:user@x.com" --type "#password:secret" \
  --click "button[type=submit]" --save-storage-state /tmp/state.json
# 2. nas capturas seguintes, restaure antes da navegação
sniffCSS -u "$URL/cms/dashboard" -s "main" --storage-state /tmp/state.json
```

O mesmo se aplica ao MCP via `headers`/`storage_state`/`save_storage_state`, com
defaults de servidor em `SNIFF_DEFAULT_HEADERS`, `SNIFF_STORAGE_STATE` e
`SNIFF_BASE_URL` (ver `docs/ai-usage.md`).

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
- `--output summary` / `--summary` (alias `slim`): **formato padrão**. Digest
  intermediário de 1 linha por nó — esqueleto estrutural
  (`tag/selector/path/depth/rect/visible`/`grade`) **mais** os facetos que
  respondem perguntas reais sem o JSONL completo: `css` (subconjunto curado:
  display, position, width/height, cores, font-size/weight, overflow, z-index),
  `contrast` (`ratio`/`aa`/`aaa`) e `aria` (`role`/`name`/`focusable`).
  Constantes globais saem numa linha `__meta` inicial (`style_defaults`, mesma
  dedup do compact) e ficam omitidas por nó. ~5-10x menor que o snapshot
  completo.
- `--output jsonl` (ou `--no-summary`): o snapshot completo não-sumarizado —
  usado como entrada de `sniffCSS-diff`/`sniffCSS-check`/jq.
- Com `--stable-key data-testid`, os seletores saem como `button[data-testid="submit"]`
  em vez de dependerem de `id` gerados (`react-aria-123`), mantendo o output
  **matchável entre deploys**.

### Dedup de constantes no compact (`__meta.style_defaults`)

No modo compact (padrão), props cujo valor é idêntico em **todos** os nós
capturados são hoisted uma única vez para a linha `__meta.style_defaults`
(`{categoria: {prop: value}}`) e omitidas dos `styles` de cada nó. Em páginas
típicas isso corta 50-80% do JSONL (80% dos bytes de estilos) **sem perder
fidelidade**: `sniffCSS-diff` e `sniffCSS-check` (e o MCP) mesclam os defaults
de volta, então uma mudança de página inteira (ex.: troca de `font-family`) é
detectada normalmente. O `computed_style_hash` de cada nó sempre cobre os
estilos efetivos completos.

```jsonl
{"__meta":{"style_defaults":{"typography":{"font-family":"Nunito Sans, ...","font-size":"16px"}}}}
{"id":1,"tag":"MAIN","selector":"main#main","styles":{"box_model":{"width":"1350px"}}}
```

> Lendo um nó isolado: toda prop ausente do `styles` mas presente em
> `__meta.style_defaults` vale o valor global. Use `--no-compact`/`--full` para
> nós totalmente autocontidos.

### `__meta.viewport`

O `__meta` também carrega o **viewport emulado** da captura
(`{"viewport":{"width":1366,"height":768}}`) em todos os formatos que emitem
`__meta`. Os checks offline usam isso para regras relativas à tela, como
`horizontal-overflow` (conteúdo que ultrapassa a viewport) e
`backdrop-over-modal` (backdrop escuro cobrindo o modal).

## Screenshot (`--screenshot`)

Complementa o snapshot calculado com o "como a página realmente está" no estado
final do pipeline (pós-stabilize, pós-interações):

```bash
sniffCSS -u "$URL" -s ".modal" --click "#open" --screenshot modal.png
sniffCSS -u "$URL" -s "main" --depth 5 --screenshot page.png --fullpage-screenshot
```

O PNG é um artefato para olho humano; o JSONL continua sendo a fonte de verdade
para diff/check.

## Atributos DOM (`--attrs`)

Computed styles são o núcleo da ferramenta, mas quando você precisa de um
atributo DOM cru (ex.: `name="parameters[items][0][title]"` para validar a
reindexação de um form), peça-o explicitamente — ele entra verbatim no mapa
`attrs` de cada nó e é comparado por chave pelo diff:

```bash
sniffCSS -u "$URL" -s "form#sucesu" --depth 3 --attrs name,value
# ... "attrs": {"name": "parameters[items][0][title]", "value": "Primeiro"} ...
```

## Persistência em disco (`--persist`)

Espelha o layout do store do MCP sem precisar de um cliente MCP: cria
`sniffCSS/[domain]/[UTC]-[path]-[selector].<ext>` no diretório de execução
(ou em `SNIFF_SNAPSHOT_DIR` quando definida), no formato de `--output`
selecionado (`.jsonl` para `jsonl`/`jsonl-flat`/`summary`, `.json` para
`json`). A raiz `sniffCSS/` ganha um `.gitignore` com `*`, então toda a árvore
gerada é **ignorada pelo git** automaticamente. O snapshot continua saindo no
stdout; o caminho salvo é reportado no stderr.

```bash
sniffCSS -u "$URL" -s "main" --depth 5 --persist
# criou sniffCSS/localhost_3000/20260812T101530Z-index-main.jsonl
# (como o summary é o formato padrão, o arquivo guarda o digest; para persistir
#  o snapshot completo — necessário como entrada de diff/check — use junto o
#  --no-summary, ex.: sniffCSS -u "$URL" -s "main" --depth 5 --no-summary --persist)
```

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
    | VERSION=v0.4.1 sh
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
