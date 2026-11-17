# Findings — performance

Escopo: 26 guias lidos integralmente da categoria `performance`.

Premissa desta fatia: o `sniff` não mede Core Web Vitals. Onde um guia descreve
**código de medição** (PerformanceObserver, Event Timing, LoAF, visibility-state)
ou **comportamento de rede/JS** (fetchLater, scheduler.postTask, fetch priority),
o sinal não existe na página renderizada: classifiquei como `N` em vez de inventar
proxy fraco. Onde o guia deixa **vestígio declarativo no DOM/CSSOM** (atributos,
computed styles, ARIA, retângulo), classifiquei como `D`/`F`. Nenhum guia desta
fatia cai integralmente em check existente (`E=0`): os checks atuais são de
layout/AX/contraste, não de dívida de performance.

| # | guia | classe | sinal detectável / razão do N | check_id proposto | facets exigidas | severidade | keepWhen / dropWhen |
|---|---|---|---|---|---|---|---|
| 1 | `batch-analytics-events` | N | `batch-analytics-events`: "call `fetchLater()` with your data payload". API de JS e beacons de rede; nenhum atributo/estilo/AX muda no snapshot. CDP `Network` veria requisições, mas classificar "analytics" depende de intenção do autor e endpoint. | — | — | — | n/a (N) |
| 2 | `break-up-long-tasks` | N | `break-up-long-tasks`: "scheduler.yield() only splits a single running task". Duração de tarefa é tempo de execução; requer CDP `Performance`/LoAF e varia com dispositivo — contraria determinismo. | — | — | — | n/a (N) |
| 3 | `calculate-total-foreground-time` | N | `calculate-total-foreground-time`: "query the `visibility-state` performance entries". Histórico da Performance Timeline; `document.visibilityState` no snapshot só daria o estado atual, não o tempo acumulado. | — | — | — | n/a (N) |
| 4 | `conditional-async-dependencies` | N | `conditional-async-dependencies`: "top-level await allows modules to act as asynchronous functions". Estrutura do grafo de módulos e detecção de feature são código-fonte; o DOM final não revela `await import()` condicional. | — | — | — | n/a (N) |
| 5 | `defer-rendering-heavy-content` | D | `defer-rendering-heavy-content`: "You MUST pair `content-visibility: auto` with `contain-intrinsic-size`". `styles.layout` já captura ambas; `rect` + `__meta.viewport` detectam uso acima da dobra. | `content-visibility-without-intrinsic-size`, `content-visibility-above-the-fold` | `styles.layout` + `rect` + `__meta.viewport` | warn | Não disparar se `content-visibility` for `hidden`/`visible`, se `contain-intrinsic-size` trouxer `<length>`, ou se o nó for `display:none`. |
| 6 | `defer-work-until-scroll-ends` | N | `defer-work-until-scroll-ends`: "The `scrollend` event ... when a scroll has rested". Registro de listener e o `setTimeout` do debounce são comportamento de JS; `DOMDebugger.getEventListeners` mostraria o listener, não seu corpo. Container com `overflow:auto` é sinal comum demais. | — | — | — | n/a (N) |
| 7 | `deliver-optimized-decorative-images` | F | `deliver-optimized-decorative-images`: "provide a standard image declaration *before* the image-set() rule". Ordem de declarações no CSSOM não aparece no computed style (que resolve para um valor só). | `image-set-without-legacy-fallback` | `styles.visual.background-image` + `declared` (CSSOM) + `pseudo` | warn | Não disparar se não houver `image-set(` na regra, se o fallback `url()` existir, ou se for `mask-image` sem fallback visual necessário. |
| 8 | `deprioritize-background-fetches` | N | `deprioritize-background-fetches`: "Add the `priority: 'low'` option". `priority` é opção de `fetch()`, invisível no DOM; CDP `Network` expõe a prioridade, mas "background" é juízo de intenção do autor. | — | — | — | n/a (N) |
| 9 | `detect-initial-visibility-state` | N | `detect-initial-visibility-state`: "Use `performance.getEntriesByType('visibility-state')`". Precisa do histórico (primeira entrada `hidden`); `__env` daria só o estado atual e perderia a race condition que o guia explicita. | — | — | — | n/a (N) |
| 10 | `efficient-background-processing` | D | `efficient-background-processing`: "By listening to this event, you can pause expensive operations". A metade CSS é observável: `content-visibility:auto` sem `contain-intrinsic-size`; o listener `contentvisibilityautostatechange` e o pause de WebGL/WebSocket não são. | `content-visibility-without-intrinsic-size` | `styles.layout` | warn | Mesmo do check compartilhado; não exigir listener (fora de escopo do snapshot). |
| 11 | `faster-spa-view-transitions` | D | `faster-spa-view-transitions`: "Use the `inert` attribute to ensure interface elements ... not included in the keyboard tab order". `content-visibility:hidden` em `styles.layout`, `aria.aria_hidden` e `--attrs inert` no mesmo nó. | `cv-hidden-view-not-inert` | `styles.layout` + `aria.aria_hidden` + `attrs` + descendentes `aria.focusable` | warn | Não disparar se o nó for `display:none`, se todos os descendentes focalizáveis forem `disabled`, ou se o estado atual for a view ativa (`content-visibility:visible`). |
| 12 | `flicker-free-client-side-ab-testing` | F | `flicker-free-client-side-ab-testing`: "add `blocking=\"render\"`". Atributo vive em `<script>`/`<link>` do `<head>`; `__document` é que precisa existir. O hack legado `opacity:0` com classe `ab-loading` é observável em `styles.visual` + `attrs`. | `render-blocking-script-misuse` | `__document.scripts/links` + (secundário) `attrs` + `styles.visual.opacity` | warn | Não disparar em script de analytics sem `blocking`, nem quando `__document` ausente (reportar `unknown`, não `warn`); não flagrar o fallback de opacidade em navegador sem suporte a `blocking`. |
| 13 | `full-session-analytics` | N | `full-session-analytics`: "creating an `<img>` pixel in an `unload` event listener". `fetchLater()` e listeners de `unload` são código; elegibilidade para bfcache não é observável num único snapshot renderizado. | — | — | — | n/a (N) |
| 14 | `identify-heavy-scripts` | N | `identify-heavy-scripts`: "emits a `long-animation-frame` entry ... `sourceURL` and `duration`". É API de medição em campo; mesmo CDP `Performance` não atribui `sourceURL` por script com determinismo. | — | — | — | n/a (N) |
| 15 | `identify-inp-causes` | N | `identify-inp-causes`: "measuring the causes of slow INP in the field is essential". Subpartes (input/processing/presentation) e `forcedStyleDuration` são séries temporais de Event Timing + LoAF, não estado renderizado. | — | — | — | n/a (N) |
| 16 | `improve-next-page-load-performance` | F | `improve-next-page-load-performance`: "DO NOT speculate URLs that likely trigger state changes". As regras são JSON inline em `<script type="speculationrules">`; é preciso `__document.scripts[].text` para avaliar `eagerness`/exclusões. | `speculation-rules-misuse` | `__document.scripts[].text` | warn | Não disparar se não houver script `speculationrules`, em lista curta de URLs, nem em regras `conservative` com exclusões explícitas; SPA é fora de escopo do guia. |
| 17 | `interaction-paint-timing` | N | `interaction-paint-timing`: "interaction causality across asynchronous tasks". `interaction-contentful-paint`/`soft-navigation` são entradas com `interactionId` e `presentationTime`; `__actions` mostra **o quê** mudou após o clique, não **quando** houve paint. | — | — | — | n/a (N) |
| 18 | `interactions-in-complex-layouts` | D | `interactions-in-complex-layouts`: "Apply `content-visibility: auto` to large, self-contained containers". Mesmo par `content-visibility`/`contain-intrinsic-size` capturado em `styles.layout`; o benefício em elementos visíveis vem do `contain`, também capturado. | `content-visibility-without-intrinsic-size` | `styles.layout` | warn | Não disparar se `contain-intrinsic-size` existir ou se o autor usar o fallback `contain: layout style paint` explícito. |
| 19 | `lazy-load-video-and-audio` | D | `lazy-load-video-and-audio`: "Always include the `controls` attribute on `<audio loading=\"lazy\">` elements". `tag` + `--attrs loading,width,height,controls,poster,preload,autoplay,muted` + `rect`/viewport cobrem os MANDATORY do guia. | `lazy-media-without-dimensions-or-controls`, `lazy-media-above-the-fold` | `tag` + `attrs` + `rect` + `__meta.viewport` | warn | Não disparar em mídia `eager`, em `<video>` com `width`/`height` via CSS explícito, nem quando houver fallback `data-src`/`data-poster` (IntersectionObserver). |
| 20 | `optimize-image-priority` | D | `optimize-image-priority`: "**DO NOT** combine `fetchpriority="high"` with `loading="lazy"`". Conflito de atributos é determinístico; `low`+`lazy` abaixo da dobra e `fetchpriority="auto"` também. | `fetchpriority-high-with-lazy`, `lazy-media-above-the-fold` | `tag` + `attrs` + `rect` + `__meta.viewport` | warn | Não disparar em `img` com `fetchpriority=high` e `eager` (caso correto do LCP), nem em elemento fora de viewport com `lazy` isolado. |
| 21 | `optimize-preload-priority` | F | `optimize-preload-priority`: "ensure that font preloads always have the `crossorigin` attribute". `<link rel=preload>` é recurso de `<head>`; `__document.links` com `rel/as/type/fetchpriority/crossorigin` resolve. | `preload-priority-misuse` | `__document.links` | warn | Não disparar se `__document` ausente, em preload `as=image` com `fetchpriority=high` (correto), nem quando o browser alvo ignora `fetchpriority` (progressive enhancement). |
| 22 | `optimize-script-priority` | F | `optimize-script-priority`: "DO NOT use the deprecated `importance` attribute". `importance` e `fetchpriority` em `<script>` exigem `__document.scripts[]`. | `script-priority-misuse` | `__document.scripts` | warn | Não disparar se `__document` ausente, se `fetchpriority` aparecer em 1–2 scripts críticos `async`, nem em browser sem suporte (atributo ignorado). |
| 23 | `out-of-order-html-streaming` | D | `out-of-order-html-streaming`: "Only apply the live region markup to the container for the text that indicates progress". `aria-live`/`aria-busy` persistem no DOM final; `aria.role` já captura `alert`. O mecanismo de streaming (processing instructions, `streamHTML`) não é observável. | `streaming-live-region-scope` | `attrs` (`aria-live`,`aria-busy`) + `aria.role` + árvore (`node_count`) | warn | Não disparar se `aria-live` estiver num nó folha de texto, se a região estiver `aria-hidden`/`display:none`, ou se o polyfill reescreveu marcadores como comentários. |
| 24 | `performance` (guia guarda-chuva) | D | `performance`: "DO pair content-visibility with contain-intrinsic-size". Vários itens caem em facets atuais: `content-visibility`, dimensões de `img`, `lazy` acima da dobra, `srcset` sem `sizes`, conflito `fetchpriority`+`lazy`. Itens de `<head>`/HTTP/JS ficam em `__document`/N. | `content-visibility-without-intrinsic-size`, `image-without-dimensions`, `lazy-media-above-the-fold`, `srcset-without-sizes`, `fetchpriority-high-with-lazy` | `styles.layout` + `tag` + `attrs` + `rect` + `__meta.viewport` | warn | União dos keepWhen dos checks listados; não exigir `__document` para os checks D; `will-change`/`@import`/Service Worker ficam como F/N separados. |
| 25 | `resolution-optimized-pseudo-elements` | F | `resolution-optimized-pseudo-elements`: "provide a standard image declaration *before* the image-set() rule". `pseudo[].styles.accessibility.content` e `pseudo[].styles.visual.background-image` veem o valor resolvido; a ordem das declarações exige `declared`. | `image-set-without-legacy-fallback` | `pseudo` + `declared` (CSSOM) | warn | Não disparar se `content`/`background-image` do pseudo não contiver `image-set(`, se houver fallback `url()` anterior, ou se o pseudo for `content: ""` sem imagem. |
| 26 | `schedule-tasks-by-priority` | N | `schedule-tasks-by-priority`: "lets you schedule, reprioritize, and cancel work across priority levels". `scheduler.postTask()`/`TaskController.setPriority()` não têm reflexo em DOM, CSS, AX ou atributos; só instrumentação de runtime. | — | — | — | n/a (N) |

## Detalhes dos checks propostos (só D e F)

### `content-visibility-without-intrinsic-size`
- **Guias**: `defer-rendering-heavy-content`, `efficient-background-processing`, `interactions-in-complex-layouts`, `performance`.
- **Gatilho**: nó com `styles.layout["content-visibility"]` exatamente `auto` **e** ausência de `styles.layout["contain-intrinsic-size"]` (ou valor `normal`/`none` sem `<length>`). As chaves `contain-intrinsic-width/height` também são capturadas e podem satisfazer o requisito.
- **Evidência**: `"content-visibility:auto sem contain-intrinsic-size em {selector}; placeholder 0px causa salto de scrollbar/CLS"`.
- **Falso positivo / keepWhen**: não disparar para `content-visibility: hidden|visible`; não disparar quando o fallback `@supports not (content-visibility:auto)` com `contain: layout style paint` estiver presente (isso é o fallback legítimo, detectável só com `declared` — nesse caso reportar `unknown`); não disparar em nó com `is_user_noticeable.display_visible == false` por `display:none`.
- **Fallback/Baseline**: `content-visibility` é *Newly available* desde 2025-09-15; quando não suportado a propriedade é ignorada e o ganho de performance desaparece. O check não pode exigir a propriedade — apenas cobra o par quando ela já está aplicada.
- **Fixture mínima**: `<section class="items">` com CSS `content-visibility:auto` e nenhuma declaração de `contain-intrinsic-size`; segunda fixture com `contain-intrinsic-size:auto 150px` deve passar.

### `content-visibility-above-the-fold`
- **Guias**: `defer-rendering-heavy-content`, `performance`.
- **Gatilho**: `styles.layout["content-visibility"] == "auto"` **e** `rect.y < __meta.viewport.height` (interseção com a viewport inicial); `rect.width > 0 && rect.height > 0`.
- **Evidência**: `"content-visibility:auto em {selector} dentro da viewport inicial (y={rect.y}, vh={viewport.height}) — adiciona overhead na renderização crítica"`.
- **Falso positivo / keepWhen**: não disparar em viewport emulada ausente; não disparar se o nó estiver fora da viewport por scroll (o `rect` é do estado capturado); não disparar em `content-visibility:hidden`.
- **Fallback/Baseline**: mesmo de `content-visibility`; o guia recomenda `contain-intrinsic-size` e diz explicitamente não aplicar acima da dobra — o check apenas reforça a recomendação, sem exigir suporte.
- **Fixture mínima**: elemento `content-visibility:auto` com `rect.y=20` e viewport `900`; variante com `rect.y=1200` deve passar.

### `cv-hidden-view-not-inert`
- **Guias**: `faster-spa-view-transitions` (sobreposição parcial com o check existente `aria-hidden-focusable`, que já cobre `aria-hidden` + focalizável — **não duplicar**: este check cobre o caso em que `aria-hidden` está ausente e só `content-visibility:hidden` foi usado).
- **Gatilho**: `styles.layout["content-visibility"] == "hidden"` **e** `aria.aria_hidden` ausente **e** `attrs["inert"]` ausente **e** existe descendente (na árvore capturada) com `aria.focusable == true`.
- **Evidência**: `"view hidden via content-visibility sem inert: {selector} mantém {n} descendentes focalizáveis na ordem de tabulação"`.
- **Falso positivo / keepWhen**: não disparar se a view for a ativa (`visible`), se todos os focalizáveis estiverem `disabled`, se `is_user_noticeable.display_visible == false` por `display:none`, ou se `attrs["inert"]` estiver presente.
- **Fallback/Baseline**: `content-visibility` *Newly available* (2025-09-15); em browsers sem suporte o autor deve usar `@supports not (...)` com `display:none` — nesse caso o nó não tem `content-visibility:hidden` computado e o check silencia naturalmente. O guia também pede eviction strategy (LRU) para apps dinâmicas, o que é comportamento de JS e fica fora de escopo.
- **Fixture mínima**: duas `.spa-view`; uma com `content-visibility:hidden` + `<button>` interno sem `inert`/`aria-hidden` (deve disparar); outra com `inert` (deve passar).

### `lazy-media-without-dimensions-or-controls`
- **Guias**: `lazy-load-video-and-audio`, `performance` (itens de vídeo).
- **Gatilho**: (a) `tag == "VIDEO"` e `attrs["loading"] == "lazy"` e (`attrs["width"]`/`attrs["height"]` ausentes ou não-inteiros); **ou** (b) `tag == "AUDIO"` e `attrs["loading"] == "lazy"` e `attrs["controls"]` ausente. Detectável também: `tag == "VIDEO"` com `autoplay` e sem `muted` (risco WCAG 1.4.2 — manter `warn` porque a snapshot não mede duração >3s).
- **Evidência**: `"{tag} {selector} com loading=lazy sem dimensões inteiras — caixa cai para 300x150 e pode nunca intersectar a viewport"` / `"<audio loading=lazy> {selector} sem controls: nunca carrega nem reproduz"`.
- **Falso positivo / keepWhen**: não disparar em mídia `eager`/sem `loading`; não disparar se `styles.box_model["width"]` e `["height"]` forem valores explícitos não-`auto` (sizing via CSS pode bastar); não disparar quando houver fallback `data-src`/`data-poster` típico do IntersectionObserver; não transformar o caso `autoplay`+sem `muted` em `fail` sem medir duração.
- **Fallback/Baseline**: `loading="lazy"` em mídia é *Limited availability* (Chrome/Edge 150); o guia recomenda progressive enhancement (`preload=none`/`metadata`) ou fallback `IntersectionObserver`. O check não pode punir a ausência do atributo — só cobre os atributos companion quando o autor já optou por `lazy`.
- **Fixture mínima**: `<video loading="lazy" poster="x.jpg">` sem `width`/`height` (dispara); `<audio loading="lazy" src="a.mp3">` sem `controls` (dispara); versões com atributos completos passam.

### `lazy-media-above-the-fold`
- **Guias**: `lazy-load-video-and-audio`, `optimize-image-priority`, `performance`.
- **Gatilho**: `tag` em {`IMG`, `VIDEO`, `AUDIO`, `IFRAME`}, `attrs["loading"] == "lazy"`, e `rect` intersectando a viewport inicial (`rect.y < vh && rect.y + rect.height > 0`).
- **Evidência**: `"loading=lazy em {tag} {selector} dentro da viewport ({rect.y}..{rect.y+rect.height}) — atrasa LCP/poster"`.
- **Falso positivo / keepWhen**: não disparar sem `__meta.viewport`; não disparar se o elemento estiver totalmente abaixo de `vh`; não disparar em imagem pequena/decorativa onde o atraso é inofensivo — usar limite de área (ex.: `rect.width*rect.height` acima de um limiar configurável) para manter determinismo.
- **Fallback/Baseline**: `loading=lazy` para imagens é *Widely available*; para mídia é *Limited*. Em browsers sem suporte o atributo é ignorado (e o guia nota que browsers só deferem com JS habilitado) — o check apenas sinaliza o conflito de intenção, nunca exige o atributo.
- **Fixture mínima**: `<img loading="lazy" width="800" height="400">` com `rect.y=100`, viewport `768` (dispara); mesmo elemento com `rect.y=900` passa.

### `fetchpriority-high-with-lazy`
- **Guias**: `optimize-image-priority`, `performance`.
- **Gatilho**: `tag == "IMG"` (ou `SOURCE`) com `attrs["fetchpriority"] == "high"` **e** `attrs["loading"] == "lazy"`. Variante determinística: `attrs["fetchpriority"] == "auto"` (o guia recomenda omitir) — severidade `warn`, baixa prioridade.
- **Evidência**: `"fetchpriority=high combinado com loading=lazy em {selector}: sinais contraditórios ao preload scanner"`.
- **Falso positivo / keepWhen**: não disparar em `fetchpriority=high` com `loading` ausente/`eager` (configuração correta do LCP); não disparar se `__attrs` não tiver sido capturado (reportar `unknown`); não exigir `fetchpriority=high` — "faltar high" exigiria saber qual elemento é o LCP real (dado de campo → fora de escopo).
- **Fallback/Baseline**: Fetch priority é *Newly available* desde 2024-10-29 e é progressive enhancement (atributo ignorado em browsers antigos). O check não pode inferir "deveria ter high" a partir da snapshot.
- **Fixture mínima**: `<img fetchpriority="high" loading="lazy">` (dispara); `<img fetchpriority="high">` (passa).

### `image-without-dimensions`
- **Guias**: `performance` (também ecoado por `optimize-image-priority`, `lazy-load-video-and-audio`).
- **Gatilho**: `tag == "IMG"` com `attrs["width"]` ou `attrs["height"]` ausente **e** `styles.box_model` não fornecendo `width`/`height` explícitos (`auto`).
- **Evidência**: `"<img> {selector} sem width/height — aspect ratio desconhecido até o download (risco de CLS)"`.
- **Falso positivo / keepWhen**: não disparar se `styles.box_model` tiver `width` e `height` resolutionados para valores não-`auto`; não disparar em imagens intrinsecamente dimensionadas via CSS `aspect-ratio` (capturado em `box_model`); não disparar em `<svg>` inline.
- **Fallback/Baseline**: dimensões HTML são estabilidade de layout, sem problema de suporte; o check não depende de feature alguma. `loading=lazy` é progressive enhancement.
- **Fixture mínima**: `<img src="x.jpg">` sem dimensões e sem CSS (dispara); `<img width="400" height="300">` (passa).

### `srcset-without-sizes`
- **Guias**: `performance`.
- **Gatilho**: `tag == "IMG"` com `attrs["srcset"]` presente **e** `attrs["sizes"]` ausente.
- **Evidência**: `"srcset sem sizes em {selector}: browser assume 100vw e pode baixar o maior recurso"`.
- **Falso positivo / keepWhen**: não disparar em `<source>` dentro de `<picture>` — lá o `sizes` deve estar no `<source>`, e o check deve ser aplicado ao `<source>` com `srcset`; não disparar se `attrs` não tiver `sizes` habilitado (reportar `unknown`); não disparar em `srcset` com descritores de densidade `1x/2x` (onde `sizes` não se aplica) — decidir pela presença de `w` no token, de forma determinística.
- **Fallback/Baseline**: imagens responsivas são *Widely available*; não há fallback necessário, apenas desperdício de banda.
- **Fixture mínima**: `<img srcset="a-400w.jpg 400w, a-800w.jpg 800w">` sem `sizes` (dispara); com `sizes="50vw"` passa; `<img srcset="a.jpg 1x, a-2x.jpg 2x">` passa.

### `streaming-live-region-scope`
- **Guias**: `out-of-order-html-streaming`.
- **Gatilho**: nó com `attrs["aria-live"]` presente **e** `node_count()` (subárvore capturada) acima de um limiar pequeno (ex.: > 5 descendentes) — ou seja, a live region envolve conteúdo "patchado", não só o texto de progresso. Variante determinística: `attrs["aria-busy"] == "true"` em ancestral e descendente com `aria-live` (o guia proíve esse aninhamento). `aria.role == "alert"` em região de progresso é sinal secundário e **não** deve ser usado sozinho sem texto confiável (localização).
- **Evidência**: `"live region em {selector} cobre {n} nós; guia exige aria-live apenas no texto de progresso"`.
- **Falso positivo / keepWhen**: não disparar se a live region for folha de texto; não disparar em nó `aria_hidden`/`display:none`; não disparar quando o polyfill (`template-for-polyfill`) reescreveu os processing instructions como comentários — o check só olha ARIA residual, nunca presume streaming.
- **Fallback/Baseline**: OOO streaming é *Limited availability* (Chrome 150/157); browsers sem suporte veem `<?marker>` como comentários. Como o mecanismo não é observável, o check se limita ao markup de anúncio que persiste no DOM final, preservando o fallback do guia.
- **Fixture mínima**: `<section aria-live="polite"><div class="skeleton">…5+ filhos…</div></section>` (dispara) vs. `<p aria-live="polite">Loading…</p>` (passa).

### `image-set-without-legacy-fallback`
- **Guias**: `deliver-optimized-decorative-images`, `resolution-optimized-pseudo-elements`.
- **Gatilho**: (a) `styles.visual["background-image"]` contém `image-set(` **ou** (b) `pseudo[].styles.accessibility["content"]` / `pseudo[].styles.visual["background-image"]` contém `image-set(`; **e** a regra CSSOM correspondente em `declared` não possui uma declaração `url(...)` anterior na mesma regra para a mesma propriedade.
- **Extensão / custo**: facet `declared` (CSSOM: regras casadas + origem author/UA/inline, com as declarações em ordem) — **uma linha global** para o CSSOM serializado; alternativa mais cara: por nó, anexar as declarações casadas (custo por nó, evitada). `pseudo` já existe e custa por nó apenas quando habilitado.
- **Evidência**: `"image-set() em {selector}::{pseudo} sem declaração url() anterior — browsers sem suporte perdem a imagem"`.
- **Falso positivo / keepWhen**: não disparar se não houver `image-set(`; não disparar se existir `url()` anterior na mesma regra; não disparar em `mask-image` puro sem contrapartida visual; não disparar se `declared` estiver ausente (reportar `unknown`, nunca `warn`); não exigir fallback quando o baseline do projeto já exclui browsers pré-2023 — nesse caso o próprio `declared` permite detectar a diretiva de baseline, mas isso fica fora do check.
- **Fallback/Baseline**: `image-set()` é *Widely available* desde 2023-09-18; o guia marca como MANDATORY declarar `url()` antes por cascata. O check existe precisamente para proteger esse fallback — nunca deve sugerir removê-lo.
- **Fixture mínima**: `.gallery { background-image: image-set(url("a.avif") type("image/avif") 1x); }` sem `url()` prévio (dispara); com `background-image: url("a.jpg")` anterior passa. Segunda fixture com `.icon::before { content: image-set(...) }`.

### `render-blocking-script-misuse`
- **Guias**: `flicker-free-client-side-ab-testing` (e, parcialmente, o item "não bloquear com JS de terceiros" de `performance`).
- **Gatilho**: em `__document.scripts[]`: (a) `blocking` contém `render` **e** o script não está no `<head>`; (b) `blocking=render` **e** ausência de `async` **e** de `type=module` — nesse caso é parser-blocking, não apenas render-blocking; (c) presença de hack legado: `styles.visual["opacity"] == "0"` no elemento `html` com `attrs["class"]` contendo `ab-loading` (sinal do fallback anti-flicker). Não disparar só porque um script é de analytics (intenção do autor).
- **Extensão / custo**: facet `__document` (arrays `scripts`/`links` com `tag`, `src`, `type`, `async`, `defer`, `blocking`, `inHead`, `text` curto) — **uma linha global**; o sub-check (c) usa apenas facets atuais (`attrs` + `styles.visual`), custo zero extra.
- **Evidência**: `"script {src} com blocking=render fora do head / sem async — bloqueia parser e atrasa first paint"`.
- **Falso positivo / keepWhen**: não disparar em `__document` ausente (`unknown`); não disparar em `<link rel=stylesheet>` do head (já bloqueia por padrão); não disparar no fallback `opacity:0` quando `__document.scripts` indicar que `blocking` não é suportado (não há como saber o browser pela snapshot — nesse caso limitar a `warn` informativo e nunca `fail`); não exigir `blocking=render` em todo site de experimentos (servidor-side é alternativa legítima).
- **Fallback/Baseline**: `blocking=render` é *Limited availability* (Chrome/Edge 105, Safari 18.2; sem Firefox); o guia prescreve feature-detect e snippet anti-flicker com timeout de 4s. O check não pode exigir o atributo — apenas alerta sobre uso incorreto/persistência do hack.
- **Fixture mínima**: `<head><script src="sdk.js" blocking="render"></script></head>` sem `async` (dispara "b"); `<body><script blocking="render">` (dispara "a"); versão com `async blocking="render"` no head passa.

### `speculation-rules-misuse`
- **Guias**: `improve-next-page-load-performance`.
- **Gatilho**: `__document.scripts[]` com `type == "speculationrules"` e `text` JSON analisável: (a) `eagerness == "immediate"` aplicado a mais de um punhado de URLs/selector amplo; (b) regra `document` (`where.href_matches`/`selector_matches`) sem cláusulas `not` excluindo padrões de estado (`logout`, `add-to-cart`) quando `href_matches` é amplo; (c) mais de 2 especulações não-`eager` (limite do browser citado no guia).
- **Extensão / custo**: `__document.scripts[].text` — mesma linha global do `__document`; o JSON é curto, custo desprezível. Parse determinístico (sem heurística de intenção além dos padrões literais do próprio guia).
- **Evidência**: `"speculationrules {tag} com eagerness=immediate em {n} URLs / sem exclusão de /logout — risco de side effects"`.
- **Falso positivo / keepWhen**: não disparar se `__document`/`text` ausente ou JSON inválido (`unknown`); não disparar em lista curta de URLs conhecidas (`urls` com ≤2 entradas); não disparar em regras `prefetch conservative`; não disparar em SPA (o guia diz explicitamente que speculation rules não se aplicam — sinal de SPA viria de `__document`/framework, fora de escopo, então limitar a `warn`).
- **Fallback/Baseline**: Speculation rules são *Limited availability* (Chrome/Edge 109); progressive enhancement — browsers sem suporte ignoram. O check nunca deve exigir a presença da regra, apenas corrigir configuração.
- **Fixture mínima**: `<script type="speculationrules">{"prerender":[{"where":{"href_matches":"/*"},"eagerness":"immediate"}]}</script>` (dispara); mesma regra com `eagerness=moderate` e `not.href_matches=/logout` passa.

### `preload-priority-misuse`
- **Guias**: `optimize-preload-priority` (e item "fontes" de `performance`).
- **Gatilho**: em `__document.links[]` com `rel` contendo `preload`: (a) `as` ausente; (b) `as == "font"` sem `crossorigin`; (c) `as == "font"` com `fetchpriority == "high"` (guia: fontes já são high por padrão); (d) contagem de preloads `as=image` > 2 ou fontes > 3 na mesma página.
- **Extensão / custo**: `__document.links` — uma linha global (mesma facet dos outros checks de head).
- **Evidência**: `"preload {href} com as=font e fetchpriority=high — redundante; total de {n} preloads excede o orçamento"`.
- **Falso positivo / keepWhen**: não disparar se `__document` ausente; não disparar em `as=image` com `fetchpriority=high` em LCP (padrão correto); não disparar se `as`/`crossorigin` forem injetados por JS depois do parse — nesse caso `__document` deve refletir o DOM final, não o HTML estático; limitar contagem a preloads efetivamente presentes.
- **Fallback/Baseline**: Fetch priority é *Newly available* (2024-10-29) e progressive enhancement; browsers sem suporte usam prioridade padrão por tipo. O check corrige o uso, não exige suporte.
- **Fixture mínima**: `<link rel=preload as=font href=f.woff2 fetchpriority=high>` sem `crossorigin` (dispara a+b+c); `<link rel=preload as=image fetchpriority=high crossorigin>` com 1 imagem (passa).

### `script-priority-misuse`
- **Guias**: `optimize-script-priority` (e item "third-party scripts" de `performance`).
- **Gatilho**: em `__document.scripts[]`: (a) presença de `attrs.importance` (deprecado); (b) `fetchpriority == "low"` em script sem `async`/`defer`/`type=module` (continua parser-blocking); (c) `fetchpriority == "high"` em mais de 2 scripts (zero-sum); (d) script de terceira origem sem `defer` no `<head>`.
- **Extensão / custo**: `__document.scripts` — uma linha global.
- **Evidência**: `"script {src} com importance (deprecado) / fetchpriority=low sem async — continua bloqueando o parser"`.
- **Falso positivo / keepWhen**: não disparar se `__document` ausente; não disparar em 1–2 scripts `async` com `fetchpriority=high` (caso recomendado para INP); não disparar "terceira origem" quando o domínio for o próprio origin (self-hosting legítimo); não exigir `fetchpriority` — a ausência é o default correto.
- **Fallback/Baseline**: Fetch priority é *Newly available* (2024-10-29), ignored em browsers sem suporte — o guia diz que nenhum feature-detect é necessário. O check só sinaliza uso indevido/obsoleto.
- **Fixture mínima**: `<script src="t.js" importance="low">` (dispara); `<script src="a.js" async fetchpriority="high">` único (passa).

### `excessive-will-change`
- **Guias**: `performance` (item "DON'T overuse `will-change` globally").
- **Gatilho**: contagem de nós com `styles.visual["will-change"] != "auto"` na árvore capturada acima de um limiar configurável; priorizar valores `transform` aplicados a múltiplos elementos simultaneamente.
- **Extensão / custo**: `will-change` **não está no catálogo** de propriedades de `StyleCategory::Visual`; duas opções: adicioná-lo ao catálogo (**custo por nó** já existente, sem nova passada) ou capturar com `--categories all` (custo maior por nó). Recomendo a primeira; a facet `declared` não é necessária porque o computed value basta.
- **Evidência**: `"{n} nós com will-change:transform — VRAM excessiva; aplicar apenas em hover/interação"`.
- **Falso positivo / keepWhen**: não disparar se a propriedade não foi capturada (`unknown`); não disparar em 1–2 nós em animação ativa; não disparar em `will-change: opacity` isolado; o guia recomenda aplicar em `:hover` — um nó com `will-change` e `:hover` ativo no momento da captura é legítimo.
- **Fallback/Baseline**: `will-change` é amplamente suportado; o risco é de consumo de VRAM, não de compatibilidade. O check nunca deve exigir `will-change`.
- **Fixture mínima**: 20 `<div>` com `will-change:transform` (dispara); 1 botão com `will-change:transform` somente em `:hover` (passa).

## Resumo da fatia

- Contagens: D=8 F=6 N=12 E=0 (total 26). Nenhum `fail`: a fatia é de oportunidades, sem critério normativo explorável na snapshot; o único tema WCAG (autoplay com áudio, `lazy-load-video-and-audio`) não tem duração mensurável e por isso ficou como `warn`.
- Top 5 oportunidades priorizadas (impacto × custo):
  1. `content-visibility-without-intrinsic-size` — cobre 4 guias com `styles.layout` já existente, custo zero, previne CLS/salto de scrollbar (maior impacto por guia).
  2. `lazy-media-without-dimensions-or-controls` — `tag`+`attrs`, detecta bug funcional (mídia que nunca carrega) e risco WCAG latente; custo zero.
  3. `fetchpriority-high-with-lazy` + `lazy-media-above-the-fold` + `image-without-dimensions` + `srcset-without-sizes` — quarteto LCP/CLS sobre IMG, `attrs`+`rect`+viewport já disponíveis; cobre `optimize-image-priority` e boa parte do guarda-chuva `performance`.
  4. Facet `__document` (uma linha global) — destrava 4 guias F de uma vez (`flicker-free-client-side-ab-testing`, `improve-next-page-load-performance`, `optimize-preload-priority`, `optimize-script-priority`) e o subconjunto de head-resources de `performance`; melhor custo/benefício entre as extensões.
  5. Facet `declared` (uma linha global de CSSOM) — destrava `deliver-optimized-decorative-images` e `resolution-optimized-pseudo-elements`, protegendo o fallback `url()` que o próprio MWG marca como MANDATORY.
- Guias que exigem novas facets, agrupados por facet:
  - `__document` (scripts/links do head, com atributos e texto inline): `flicker-free-client-side-ab-testing`, `improve-next-page-load-performance`, `optimize-preload-priority`, `optimize-script-priority`; parcialmente `performance`.
  - `declared` (CSSOM com ordem das declarações): `deliver-optimized-decorative-images`, `resolution-optimized-pseudo-elements`; parcialmente `performance` (`@import`).
  - Catálogo de propriedades (incluir `will-change` em `Visual`): `performance`.
  - `__env` sozinho não resolve nada nesta fatia: `detect-initial-visibility-state` continua `N` porque o guia exige histórico, não estado atual.
  - CDP `Performance`/`Network` ou dados de campo (fora do escopo do sniff, todos `N`): `batch-analytics-events`, `break-up-long-tasks`, `calculate-total-foreground-time`, `conditional-async-dependencies`, `defer-work-until-scroll-ends`, `deprioritize-background-fetches`, `detect-initial-visibility-state`, `full-session-analytics`, `identify-heavy-scripts`, `identify-inp-causes`, `interaction-paint-timing`, `schedule-tasks-by-priority`.
- Confirmação de cobertura: todos os 26 ids da fatia (`batch-analytics-events`, `break-up-long-tasks`, `calculate-total-foreground-time`, `conditional-async-dependencies`, `defer-rendering-heavy-content`, `defer-work-until-scroll-ends`, `deliver-optimized-decorative-images`, `deprioritize-background-fetches`, `detect-initial-visibility-state`, `efficient-background-processing`, `faster-spa-view-transitions`, `flicker-free-client-side-ab-testing`, `full-session-analytics`, `identify-heavy-scripts`, `identify-inp-causes`, `improve-next-page-load-performance`, `interaction-paint-timing`, `interactions-in-complex-layouts`, `lazy-load-video-and-audio`, `optimize-image-priority`, `optimize-preload-priority`, `optimize-script-priority`, `out-of-order-html-streaming`, `performance`, `resolution-optimized-pseudo-elements`, `schedule-tasks-by-priority`) aparecem exatamente uma vez na tabela; D(8)+F(6)+N(12)+E(0)=26.
