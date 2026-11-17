# Findings — css + visual-design

Escopo: 40 guias lidos integralmente de `css` (18) e `visual-design` (22). Facets existentes consideradas: `styles` em 8 categorias + `custom` (via `--props NOME`, que captura **qualquer** propriedade computada por nome — ver `crates/sniff-css/src/cli/capture.rs:64` e `crates/sniff-engine/src/extractor.rs:630`), `css_variables`, `attrs` (opt-in), `rect`, `metrics`, `aria`, `ax`, `contrast`, `pseudo`, `__actions`, `__ax_tree`. Facets planejadas (F) conforme `docs/modernization/03-facets.md`: `declared`, `__document`, `__env`, `__meta.matrix`, `__dom_stats`.
Onde este relatório escreve `--attr NOME` como taquigrafia, leia a flag
implementada `--attrs NOME` (repetível); as facetas `attrs`/`styles.custom`
já existem — o que é novo é o **preset** `modern` e a agregação em `check`.

| # | guia | classe | sinal detectável / razão do N | check_id proposto | facets exigidas | severidade | keepWhen / dropWhen |
|---|---|---|---|---|---|---|---|
| 1 | `animate-to-intrinsic-sizes` | F | `interpolate-size` via `--props` + `transition-property`/`block-size` computados; provar ausência do guard exige segunda captura sob `prefers-reduced-motion: reduce`. `animate-to-intrinsic-sizes`: "Transitions to intrinsic keywords are disabled by default for compatibility" | `intrinsic-size-transition-no-reduced-motion` | `styles.custom(interpolate-size)` + `styles.animation` + `styles.box_model` + `__env` + `__meta.matrix` | warn | Não disparar quando sob `reduce` o `transition-duration` cai a `0s`; nem quando não há transição de dimensão. |
| 2 | `atrule-support-conditionals` | N | Guia é inteiramente sobre técnica de autoria (`@supports at-rule()`, `@property`, `CSS.supports()`). `atrule-support-conditionals`: "Use `@supports at-rule(@<at-rule name>) {…}` to conditionally apply". O snapshot vê só o valor final; `declared` (como especificado) coleta apenas `CSSStyleRule`, não prelúdios de at-rules nem script. | — | — | — | — |
| 3 | `calculate-with-intrinsic-sizes` | F | `calc-size()` aparece no valor computado de `inline-size`/`block-size`; o MANDATORY do guia é o fallback. `calculate-with-intrinsic-sizes`: "Always declare a standard sizing keyword or length immediately before" | `calc-size-missing-fallback` | `styles.box_model` + `declared` | warn | Não disparar quando uma declaração de keyword/length precede `calc-size()` na mesma regra; nem quando o browser ignora `calc-size()` (valor computado vazio). |
| 4 | `child-state-based-styling` | F | `:has(...:checked/:invalid)` só é verificável no `selectorText` casado; mudança de cor é efeito colateral. `child-state-based-styling`: "You must use the `:has()` selector on the container element" | `has-critical-state-without-fallback` | `declared` + `styles.visual` + `__actions` | warn | Não disparar quando a propriedade alterada é só decorativa (cor), ou quando existe `@supports not selector(:has(*))` com classe equivalente. |
| 5 | `content-based-styling` | F | Mesmo mecanismo de #4, agora com presença de conteúdo. `content-based-styling`: "You must use the `:has()` selector on the container element" | `has-critical-state-without-fallback` | `declared` + `styles.layout` + `__actions` | warn | Não disparar para efeito puramente decorativo; só quando layout/visibilidade muda e não há fallback `.has-image`. |
| 6 | `css` | F | Guia guarda-chuva; fatia detectável é a regra anti-inline. `css`: "DO NOT specify nontrivial styling values inline" | `inline-nontrivial-style` | `declared` (origin `inline`) | warn | Não disparar para valores triviais (`transparent`, `0`, `none`) nem para estilo inline dinâmico de estado; só literais de cor/comprimento em nó estático. |
| 7 | `css-layout` | D | `order`/`flex-direction:*-reverse` em contêiner flex/grid com descendentes focáveis. `css-layout`: "They change visual order only; the DOM order still drives sequential focus" | `visual-order-breaks-tab-order` | `styles.layout` + `aria.focusable`/`ax.focusable` + `rect` + `path` | warn | Não disparar quando nenhum filho reordenado é focável; só quando `order != 0` ou direção reversa altera a sequência vs `path`. |
| 8 | `design-token-reactivity` | F | Efeito (padding diferente) é computável, mas o mecanismo `@container style()` exige prelúdio de at-rule. `design-token-reactivity`: "style elements based on the computed custom property value of an ancestor" | `style-query-without-selector-fallback` | `styles.box_model` + `declared.at_rules` | warn | Não disparar quando existe fallback `:where([data-density=…])` ou quando a feature não é core (não inferível: restringir a propiedades de layout/visibilidade). |
| 9 | `dynamic-sibling-styling` | F | Valores computados resolvem `sibling-index()` para cor/matriz, sem rastro da função; detecção de feature detection exige CSSOM. `dynamic-sibling-styling`: "Use feature detection with `CSS.supports()` to ensure the script only runs" | `sibling-functions-without-feature-detect` | `declared` + `styles.visual` + `styles.transform` | warn | Não disparar quando houver `@supports (top: calc(sibling-index() * 1px))` e as variáveis `--sibling-index`/`--sibling-count` no fallback. |
| 10 | `fluid-scaling` | F | `font-size` computado é um px só; os limites do `clamp()` (regra WCAG 1.4.4) vivem na fonte. `fluid-scaling`: "the maximum size must not be more than 2.5 times the minimum size" | `fluid-type-clamp-exceeds-2_5x` | `declared` + `styles.custom(container-type)` + `styles.typography` (+ opc. `__meta.matrix`) | warn | Não disparar quando `clamp()` ausente, quando o valor é fallback `vw` sob `@supports not`, ou quando max ≤ 2,5× min. |
| 11 | `individual-transform-properties` | D | `__actions` compara before/after: aparecimento de `stacking_context` ao hover. `individual-transform-properties`: "apply an identity transformation to the base element" | `transform-stacking-context-shift-on-hover` | `metrics.stacking_context` + `__actions` + `styles.transform` | warn | Não disparar quando a base já declara `translate:0px;rotate:0deg;scale:1` e o contexto não muda; só quando base sem transform vira não-identidade e cria contexto. |
| 12 | `overflow-clipping-control` | D | `overflow-clip-margin` computado ≠ initial com `overflow-x/y` em `hidden\|auto\|scroll` → propriedade ignorada. `overflow-clipping-control`: "overflow: clip is required on non-replaced elements" | `overflow-clip-margin-ignored` | `styles.box_model` + `tag` | warn | Não disparar em elementos substituídos (`img`/`video`/`canvas`, que default para `clip`) nem quando `overflow: clip` está ativo. |
| 13 | `reduce-style-repetition` | N | `@function` é abstração de código; o resultado computado não guarda que uma função foi usada. `reduce-style-repetition`: "The `@function` at-rule allows you to encapsulate this logic". Mesmo estendendo `declared` a at-rules, "repetição reduzida" é propriedade do codebase, não da página renderizada. | — | — | — | — |
| 14 | `responsive-design` | D | `img`/`video` sem `aspect-ratio` e sem atributos `width`/`height`. `responsive-design`: "Use `aspect-ratio` for media elements" | `media-aspect-ratio-missing` | `tag` + `styles.box_model.aspect-ratio` + `attrs(width,height)` | warn | Não disparar quando há dimensões intrínsecas via atributos ou CSS; só em mídia sem reserva de espaço. (E parcial: `horizontal-overflow` cobre o item `100vw`.) |
| 15 | `selector-atrule-combinations` | F | Cruzar `__document.meta['color-scheme']`, `__env` e cores computadas sob matriz dark. `selector-atrule-combinations`: "Store the result of the combined filter on a custom property" | `color-scheme-mismatch` | `__document` + `__env` + `__meta.matrix` + `styles.accessibility.color-scheme` + `contrast` | warn | Não disparar quando `@media (prefers-color-scheme)` cobre o estado ou quando o autor declara esquema único intencionalmente detectável via `declared`; evidência deve citar `blocked` quando folha cross-origin. |
| 16 | `size-aware-styling` | D | `container-type: size` via `--props` + `block-size: auto` em box_model. `size-aware-styling`: "without one, descendants collapse because size containment" | `size-containment-without-definite-block-size` | `styles.custom(container-type)` + `styles.box_model` + `rect` | warn | Não disparar para `container-type: inline-size`; só quando `size` e `block-size` indefinido com descendentes colapsados (`rect.height≈0`). |
| 17 | `style-parent-with-has` | F | `:has(:user-invalid)` no seletor casado + `aria.aria_invalid` ausente/false no campo. `style-parent-with-has`: "Native `:user-invalid` does not automatically sync with ARIA attributes" | `has-user-invalid-without-aria-invalid` | `declared` + `aria` + `styles.box_model` | warn | Não disparar quando o fallback JS já marcou `aria-invalid` ou quando o nó não tem estilo de erro no pai; `:user-invalid` é Baseline widely (2023-11-02). |
| 18 | `usage-aware-component-variations` | F | Variante semântica via `@container style()`; fallback `:where()` só aparece na fonte. `usage-aware-component-variations`: "Use style queries (`@container style()`) to trigger contextual variations" | `style-query-without-selector-fallback` | `declared.at_rules` + `styles.visual` + `styles.layout` | warn | Não disparar quando a variação é só cor e não muda o que o componente renderiza; nem quando `:where(.featured)` existe como fallback. |
| 19 | `adapt-scrollbar-to-contrast-preferences` | F | `scrollbar-color` é computável, mas o override para `prefers-contrast: more` exige emulação. `adapt-scrollbar-to-contrast-preferences`: "Use an `@media (prefers-contrast: more)` block to define dark, distinct colors" | `scrollbar-contrast-ignores-prefers-contrast` | `styles.interaction.scrollbar-color` + `__env` + `__meta.matrix` | warn | Não disparar quando `scrollbar-color: auto`, quando thumb/track não resolvível (overlay transparente — declarar em `evidence`), ou quando o contraste já é alto sem override. |
| 20 | `apply-webgl-shaders` | N | O efeito vive em pixels GPU/textura, não no DOM. `apply-webgl-shaders`: "apply WebGL shaders to HTML content". O snapshot vê `<canvas>` e seu `rect`, mas não o shader GLSL nem seu resultado; `attrs(content="drawable")` provaria só uso da API, não a correção do efeito. | — | — | — | — |
| 21 | `browser-ui-theming` | D | `scrollbar-width` computado em `thin\|none`. `browser-ui-theming`: "use the `scrollbar-width` with any value other than `auto`" | `scrollbar-width-thin-or-none` | `styles.interaction.scrollbar-width` + `tag` + `metrics` | warn | Não disparar quando `auto` ou quando há mecanismo alternativo de rolagem detectável; só em região rolável. (E parcial: `text-not-selectable` e `contrast-aa` cobrem outras regras do guia.) |
| 22 | `color` | F | Modo forçado é preferência do usuário: precisa de emulação + verificar o que sobra quando cores autorais são substituídas. `color`: "In Forced Colors Mode (High Contrast on Windows), the browser overrides author colors" | `forced-colors-unhandled` | `__env.forced-colors` + `__meta.matrix` + `styles.accessibility.forced-color-adjust` + `styles.visual` | warn | Não disparar quando `forced-color-adjust: none` com par texto/fundo válido ou quando existe `outline`/border com system-color; `evidence` deve registrar `emulated`. (E parcial: `contrast-aa`.) |
| 23 | `complex-shapes` | F | `mask-image` é computável; o guard `@supports not (mask-image…)` exige fonte. `complex-shapes`: "Use vendor prefix for wider support in older browsers" | `mask-no-graceful-fallback` | `styles.visual.mask-image` + `declared` | warn | Não disparar quando máscara é decorativa e o conteúdo permanece legível sem ela; masks são Baseline widely (2023-12-07), então exigir fallbacks só quando o mascaramento altera visibilidade. |
| 24 | `contrast-color` | E | `contrast-color()` resolve para preto/branco e o resultado é exatamente o que o check existente mede. `contrast-color`: "ensuring text remains readable against a variable background color". Nada novo: `contrast-aa`/`contrast-aaa` já cobrem a garantia runtime, com `contrast.ratio`. | `contrast-aa` (existente) | `contrast` + `styles.visual` | (existente) | — |
| 25 | `custom-text-decorations` | D | Link com `text-decoration-line: none` + `border-bottom`/`background-image` de gradinho. `custom-text-decorations`: "instead of faking them with borders, background gradients, or pseudo-elements" | `decoration-faked-with-border-or-gradient` | `styles.typography` + `styles.visual` + `styles.box_model` + `tag`/`attrs(href)` | warn | Não disparar quando o border é componente visual legítimo (não-sublinhado) ou quando `text-decoration-line != none`; `focus-indicator` existente continua cobrindo focus cortado por `clip-path`. |
| 26 | `customize-scrollbar-color-and-thickness` | D | Contraste thumb/track direto do valor computado de `scrollbar-color`. `customize-scrollbar-color-and-thickness`: "Choose a thumb color with a contrast ratio of at least 3:1" | `scrollbar-thumb-track-contrast` | `styles.interaction.scrollbar-color` | fail (WCAG 1.4.11) | Não disparar quando `auto`, quando um dos lados é transparente (overlay) — registrar limitação em `evidence` — ou quando o fundo do contêiner dá contraste suficiente ao thumb. |
| 27 | `dark-mode` | F | `meta[name=color-scheme]` + `color-scheme` computado + preferência do SO sob matriz. `dark-mode`: "place a `<meta>` tag in your `<head>` to ensure the browser knows" | `dark-mode-ignored` | `__document` + `__env` + `__meta.matrix` + `styles.accessibility.color-scheme` | warn | Não disparar quando `@media (prefers-color-scheme)` define tokens (fallback documented) ou quando o site é light-only por decisão explícita em `declared`; `color-scheme` é Baseline widely (2022-02-03). |
| 28 | `equal-width-text-lines` | D | `text-fit` via `--props` + `inline-size: fit-content\|max-content` no contêiner → sem efeito. `equal-width-text-lines`: "never changes intrinsic size, so it does nothing" | `text-fit-on-intrinsic-width-container` | `styles.custom(text-fit)` + `styles.box_model` + `declared` | warn | Não disparar quando o fallback `--text-fit-scale`/`@supports not (text-fit: grow)` está presente; só quando `text-fit` computado ≠ none e o contêiner tem largura intrínseca. |
| 29 | `export-html-media-from-canvas` | N | Export é chamada JS (`toDataURL`/`toBlob`/`captureStream`) sobre pixels do canvas. `export-html-media-from-canvas`: "capture and export rich HTML content". Nem o evento de download nem o arquivo exportado têm reflexo no DOM/CSSOM; `attrs` veria só `content="drawable"`. | — | — | — | — |
| 30 | `expose-canvas-content-to-browser-features` | D | Filho `drawable` focável com `rect` 0×0 → exposto na AX tree sem geometria. `expose-canvas-content-to-browser-features`: "a `drawable` element whose geometry isn't updated is still exposed to the accessibility tree" | `drawable-without-geometry` | `attrs(content,drawable)` + `rect` + `__ax_tree` + `aria.focusable` | warn | Não disparar quando o fallback põe o HTML sobre o canvas (rect > 0) ou quando o `drawable` é decorativo; só para elemento interativo dentro de `<canvas content="drawable">`. |
| 31 | `improve-text-layout-and-legibility` | D | `text-wrap-mode: balance` + número de linhas estimado > 6 (`rect.height / line-height`). `improve-text-layout-and-legibility`: "Browsers impose a limit on the number of lines they will attempt to balance" | `text-wrap-balance-on-long-block` | `styles.typography.text-wrap(-mode)` + `styles.typography.line-height` + `rect` | warn | Não disparar quando `text-wrap: pretty`, quando linhas ≤ 6, ou quando o browser ignora `balance` (valor computado vazio); nunca aplicar em `*` (isso é `declared`, coberto em #6). |
| 32 | `interactive-content-in-3d-scenes` | F | Projeção 3D feita por `updateElementGeometry`/`InteractionManager` não aparece em `getComputedStyle`; 0×0 detecta só o caso ausente. `interactive-content-in-3d-scenes`: "syncs the element geometry with the canvas" | `drawable-3d-geometry-unverifiable` | nova facet `canvas.geometry` (por nó, via ponte JS do canvas) + `attrs` + `__ax_tree` | warn | Não disparar quando a facet não foi capturada (facet ausente → check não emite, nunca "passa"); só quando há `drawable` focável sem registro de geometria/hit-test. |
| 33 | `precise-text-alignment` | D | `text-box` via `--props` aplicado a bloco longo. `precise-text-alignment`: "DO NOT use it on long-form body text" | `text-box-on-long-form-text` | `styles.custom(text-box)` + `tag` + `rect` + `line-height` | warn | Não disparar em heading/button/badge; só em `p`/`blockquote` ou bloco com > 3 linhas; `text-box` é progressive enhancement sem fallback especial. |
| 34 | `shaped-cutouts` | D | `mask-image: url(#…)` (SVG mask) diretamente em elemento com texto. `shaped-cutouts`: "Not recommended (can crop text)" | `mask-on-text-containing-element` | `styles.visual.mask-image` + `aria.has_text` + `path` | warn | Não disparar quando o texto está em irmão não-mascarado (padrão `.card-accent`); só quando o próprio nó com texto recebe máscara SVG; máscara com gradiente de notch é aceitável. |
| 35 | `soft-edge-content-fade` | F | Gradiente de fade é computável; fallback com overlay e `pointer-events` exige fonte. `soft-edge-content-fade`: "may interfere with text selection unless `pointer-events: none` is used" | `mask-no-graceful-fallback` | `styles.visual.mask-image` + `declared` + `styles.interaction.pointer-events` | warn | Não disparar quando fade é puramente decorativo e sem conteúdo interativo; masks são Baseline widely — só exigir fallback quando o fade esconde conteúdo selecionável/focável. |
| 36 | `typography` | D | `text-align: justify` com `hyphens: none\|manual` em bloco com `lang`. `typography`: "Avoid `text-align: justify` as it can make text harder to read" | `justify-without-hyphens` | `styles.typography.text-align` + `styles.typography.hyphens` + `aria.lang` + `rect` | warn | Não disparar em linha única, quando `hyphens: auto` já ativo, ou quando `lang` ausente (então `hyphens:auto` não funcionaria — registrar em `evidence`). (E parcial: `line-height-below-font-size`, `horizontal-overflow`.) |
| 37 | `visual-effects` | D | `mix-blend-mode != normal` com `isolation != isolate`. `visual-effects`: "limit scope with `isolation: isolate`" | `blend-mode-without-isolation` | `styles.visual.mix-blend-mode` + `styles.visual.isolation` + `path` | warn | Não disparar quando `isolation: isolate` está no escopo correto ou quando `mix-blend-mode: normal`; `background-blend-mode` é escopo local e deve ser ignorado. |
| 38 | `visually-stable-font-fallbacks` | F | `font-size-adjust` é computável, mas saber que a fonte é web e que o fallback `@font-face` foi preservado exige fonte. `visually-stable-font-fallbacks`: "font fails to load or is in the process of loading" | `font-size-adjust-missing-fallback` | `declared` (@font-face) + `styles.typography.font-family` + `font-size-adjust` + `css_variables` | warn | Não disparar quando `font-size-adjust != none`, quando `font-display: optional` (sem swap) ou quando `size-adjust`/`ascent-override` estão no `@font-face` (fallback preservado). |
| 39 | `visually-stable-mixed-fonts` | F | Mesma facet: `font-size-adjust` + fallback `@supports not`. `visually-stable-mixed-fonts`: "Use a specific numeric aspect-ratio override value" | `font-size-adjust-missing-fallback` | `declared` + `styles.typography.font-family` + `font-size-adjust` | warn | Não disparar quando `from-font` ou valor numérico presente, nem quando `@supports not (font-size-adjust: from-font)` ajusta `line-height`/`font-size`. |
| 40 | `visually-texture-content` | F | `mask-image` com textura é computável; fallback `@supports not` exige fonte. `visually-texture-content`: "make the content itself appear textured by making parts of it semi-transparent" | `mask-no-graceful-fallback` | `styles.visual.mask-image` + `declared` | warn | Não disparar quando a textura é meramente cosmética e o conteúdo permanece legível sem ela; masks Baseline widely — exigir fallback só quando a máscara afeta legibilidade de texto. |

## Detalhes dos checks propostos (só D e F)

### `intrinsic-size-transition-no-reduced-motion`
- **Guias**: `animate-to-intrinsic-sizes`.
- **Gatilho**: nó com `styles.animation.transition-property` contendo `block-size|inline-size|height|width` e `styles.box_model.<prop>` com valor `auto|min-content|max-content|fit-content` ou `calc-size(`; em segunda captura com `__env.media['prefers-reduced-motion']=reduce`, `transition-duration` permanece > `0s`.
- **Evidência**: `transition-property=block-size; block-size=auto; emulate prefers-reduced-motion=reduce; transition-duration=0.4s (esperado 0s)`.
- **Falso positivo / keepWhen**: não disparar quando o guard `@media (prefers-reduced-motion: reduce)` zera a transição; não disparar se `interpolate-size` ausente e não há transição de dimensão.
- **Fallback/Baseline**: `interpolate-size` e `calc-size()` são Limited availability (Chrome/Edge 129; sem Firefox/Safari). O guard `@media (prefers-reduced-motion: reduce)` e o keyword fallback devem ser preservados; o check não deve confundir o fallback (keyword simples) com a animação.
- **Fixture mínima**: `.p{block-size:0;overflow:hidden;transition:block-size .4s}` + `.p.open{block-size:auto}`, uma variante com e uma sem `@media (prefers-reduced-motion: reduce){transition:none}`.

### `calc-size-missing-fallback`
- **Guias**: `calculate-with-intrinsic-sizes`.
- **Gatilho**: `declared` contém declaração de `inline-size|block-size|width|height` com `calc-size(` e nenhuma declaração anterior de keyword/length para a mesma propriedade na mesma regra; `styles.box_model.<prop>` computado contém `calc-size(`.
- **Evidência**: `inline-size=calc-size(fit-content, size + 2rem); declared.inline-size precedente=ausente`.
- **Falso positivo / keepWhen**: não disparar quando o browser não suporta `calc-size()` (propriedade computada vazia → nada a alertar); não disparar quando o fallback de keyword existe.
- **Fallback/Baseline**: Limited availability (Chrome/Edge 129). `calc-size()` é progressive enhancement; o fallback de keyword é MANDATORY e o check existe para protegê-lo.
- **Fixture mínima**: `.el{inline-size:calc-size(min-content, size + 2rem)}` vs `.el{inline-size:min-content;inline-size:calc-size(min-content, size + 2rem)}`.

### `has-critical-state-without-fallback`
- **Guias**: `child-state-based-styling`, `content-based-styling`.
- **Gatilho**: `declared.selectors` com `:has(` casando um contêiner cuja propriedade computada mudou entre estado base e pós-ação em `__actions` (ex.: `display`, `visibility`, `flex-direction`); ausência de `@supports not selector(:has(*))` no `declared.at_rules`.
- **Evidência**: `selector=.theme-card:has(.toggle:checked); flex-direction: column→row; fallback @supports not selector(:has(*))=ausente`.
- **Falso positivo / keepWhen**: não disparar para mudança só de cor/borda (decorativo); `:has()` é Baseline widely desde 2023-12-19, então só alertar quando a mudança afeta layout/visibilidade e é crítica.
- **Fallback/Baseline**: fallback MANDATORY via `@supports not selector(:has(*))` + classes (`.is-active`/`.has-image`), opcionalmente com `CSS.supports()` no JS para estado interativo; o check não deve exigir fallback para efeito decorativo.
- **Fixture mínima**: `.card:has(img){flex-direction:row}` com e sem bloco `@supports not selector(:has(*)){...}`.

### `inline-nontrivial-style`
- **Guias**: `css`.
- **Gatilho**: `declared` com `origin: "inline"` cujas `declarations` contêm cor ou comprimento literal (ex.: `background-color: #f06`, `padding: .3em`).
- **Evidência**: `origin=inline; background-color=#f06; padding=.3em; tag=div`.
- **Falso positivo / keepWhen**: não disparar para `transparent`, `0`, `none`, nem para estilo inline aplicado por script de estado; restringir a literais de cor/comprimento em elemento sem atributo de estado.
- **Fallback/Baseline**: não é feature Baseline; é regra de manutenibilidade. O check é uma fatia do guia guarda-chuva; as outras fatias (`:where()`, `@layer`, propriedades lógicas) também dependem de `declared`.
- **Fixture mínima**: `<div style="background:#f06;padding:.3em">x</div>` vs `<div style="background:transparent;padding:0">x</div>`.

### `visual-order-breaks-tab-order`
- **Guias**: `css-layout`.
- **Gatilho**: contêiner com `styles.layout.display` em `flex|grid` e `flex-direction` terminando em `-reverse`, ou filho com `styles.layout.order != 0`, havendo ≥2 descendentes com `aria.focusable=true`/`ax.focusable=true` cuja ordem de `rect` difere da ordem de `path`.
- **Evidência**: `display=flex; filho button order=-1; rect.x=10 (DOM 2º) < rect.x=120 (DOM 1º); focusable=2`.
- **Falso positivo / keepWhen**: não disparar quando nenhum filho reordenado é focável; `order` em conteúdo não-interativo é legítimo. WCAG 2.4.3 (Focus Order, Level A) é o critério citado em `reason`, mas a severidade permanece `warn` porque a ordem visual não é provada apenas por `order`.
- **Fallback/Baseline**: sem fallback de feature; `order` e flex são estáveis. Não confundir com check existente `horizontal-overflow`.
- **Fixture mínima**: `<div style="display:flex"><button>A</button><button style="order:-1">B</button></div>`.

### `style-query-without-selector-fallback`
- **Guias**: `design-token-reactivity`, `usage-aware-component-variations`.
- **Gatilho**: `declared.at_rules` contém `@container style(...)` que altera propriedade de layout/visibilidade em `.card`/`.button`/`.badge`, e não há regra `:where([data-*]) …` equivalente; `styles.box_model`/`styles.visual` mostram o valor resultante.
- **Evidência**: `@container style(--density: compact){.card{padding:8px}}; fallback :where([data-density=compact])=ausente; padding computado=8px`.
- **Falso positivo / keepWhen**: não disparar quando a variação é só estética (cor) e o padrão é aceitável; o guia diz não recomendado para core features sem fallback — como "core" não é inferível, restringir a propriedades que mudam estrutura/visibilidade. Usar `:where()` no fallback para não inflar especificidade.
- **Fallback/Baseline**: Container style queries são Newly available (Baseline 2026-05-19). Preservar o fallback com seletor (`:where([data-density=…]) .card`); o check não deve exigir fallback quando ele é deliberadamente omitido para feature não-core.
- **Fixture mínima**: dois containers `--density:spacious|compact` com `.card`, uma variante com fallback `:where(...)`, outra sem.

### `sibling-functions-without-feature-detect`
- **Guias**: `dynamic-sibling-styling`.
- **Gatilho**: `declared.declarations` com `sibling-index()`/`sibling-count()` e ausência de `@supports (top: calc(sibling-index() * 1px))` e de atribuição de `--sibling-index`/`--sibling-count` via `CSS.supports()`.
- **Evidência**: `background-color=hsl(calc(360deg / sibling-count() * sibling-index()),70%,50%); @supports=sem`; 5 irmãos com cores distintas.
- **Falso positivo / keepWhen**: não disparar quando o fallback via custom properties existe e é envolvido em `@supports`; não disparar quando os valores computados são idênticos (sem uso real de índice).
- **Fallback/Baseline**: `sibling-index()`/`sibling-count()` são Newly available (Baseline 2026-08-18). Fallback MANDATORY: `--sibling-index` 1-based e `@supports` envolvendo os overrides nativos; proteger esse fallback.
- **Fixture mínima**: lista de 5 `.swatch` com `background-color` via `sibling-index()`, com e sem bloco `@supports` + script de fallback.

### `fluid-type-clamp-exceeds-2_5x`
- **Guias**: `fluid-scaling`.
- **Gatilho**: `declared` com `font-size: clamp(MIN, …, MAX)` e `parse_px(MAX) > 2,5 × parse_px(MIN)`; `styles.custom(container-type)` = `inline-size|size` no ancestral; `styles.typography.font-size` fornece o valor resolvido para `evidence`.
- **Evidência**: `clamp(1rem, 5cqi, 3rem); min=16px; max=48px; razão=3.0 (>2.5); container-type=inline-size`.
- **Falso positivo / keepWhen**: não disparar quando `MAX ≤ 2,5×MIN`, quando a declaração é fallback `vw` dentro de `@supports not (font-size: 1cqi)`, ou quando não há `clamp()` explícito. Opcionalmente confirmar com `__meta.matrix` que o texto cresce ≥200% sob zoom.
- **Fallback/Baseline**: Container queries são Widely available (2023-02-14); o fallback `vw` deve ser preservado. WCAG 1.4.4 Resize Text é o critério de fundo; severidade `warn` porque a razão 2,5× é heurística de projeto, não limiar normativo literal.
- **Fixture mínima**: `.title{font-size:clamp(1rem,5cqi,3rem)}` (flag) vs `clamp(1rem,5cqi,2.5rem)` (ok), ambos com wrapper `container-type:inline-size`.

### `transform-stacking-context-shift-on-hover`
- **Guias**: `individual-transform-properties`.
- **Gatilho**: em `__actions`, o mesmo nó passa de `metrics.stacking_context=false` (e `styles.transform` todo identidade/ausente) para `stacking_context=true` com `translate|rotate|scale|transform` não-identidade no estado pós-ação.
- **Evidência**: `hover: transform antes=none, stacking_context=false; depois=matrix(…), stacking_context=true`.
- **Falso positivo / keepWhen**: não disparar quando a base já declara `translate:0px;rotate:0deg;scale:1` — mesmo que o contexto mude, o autor intencionou a identidade; não confundir com `fixed-broken-by-transform` existente (que trata `position: fixed`).
- **Fallback/Baseline**: Individual transform properties são Widely available (2022-08-05); `@supports not (translate: 0px)` com `transform` completo é o fallback e deve ser preservado.
- **Fixture mínima**: `.el{}` + `.el:hover{scale:1.05}` (flag) vs `.el{scale:1}` + `.el:hover{scale:1.05}` (ok), com captura de hover via `__actions`.

### `overflow-clip-margin-ignored`
- **Guias**: `overflow-clipping-control`.
- **Gatilho**: `styles.box_model.overflow-clip-margin` com valor não-initial (ex.: `15px`, `content-box`) e `styles.box_model.overflow-x`/`overflow-y` em `hidden|auto|scroll`; `tag` não é elemento substituído.
- **Evidência**: `overflow-x=hidden; overflow-clip-margin=15px; tag=div → declaração ignorada pelo browser`.
- **Falso positivo / keepWhen**: não disparar em `img|video|canvas|svg|iframe` (default `overflow: clip; overflow-clip-margin: content-box`); não disparar quando `overflow: clip` está ativo; se `overflow-clip-margin` computado vazio, browser não suporta → sem check.
- **Fallback/Baseline**: `overflow: clip` é Widely available (2022-09-12), mas `overflow-clip-margin` é Limited (só Firefox 148). Preservar `@supports (overflow-clip-margin: …)` com base `overflow: hidden` — o check não deve alertar sobre o fallback em browser sem suporte.
- **Fixture mínima**: `.p{overflow:hidden;overflow-clip-margin:15px}` (flag) vs `.p{overflow:clip;overflow-clip-margin:15px}` (ok).

### `media-aspect-ratio-missing`
- **Guias**: `responsive-design`.
- **Gatilho**: `tag` em `img|video` com `styles.box_model.aspect-ratio = auto|none` e `attrs.width`/`attrs.height` ausentes.
- **Evidência**: `tag=img; aspect-ratio=auto; attrs.width=ausente; attrs.height=ausente`.
- **Falso positivo / keepWhen**: não disparar quando há `width`/`height` HTML ou CSS (reserva feita); ignorar imagens puramente decorativas com dimensão controlada por `object-fit` e wrapper dimensionado. `attrs` já é facet existente (opt-in `--attr width --attr height`).
- **Fallback/Baseline**: sem feature nova; `aspect-ratio` é Widely available. A fatia `100vw` do mesmo guia é coberta por `horizontal-overflow` existente (E) e não é duplicada aqui.
- **Fixture mínima**: `<img src="hero.jpg">` (flag) vs `<img src="hero.jpg" width="800" height="450">` ou `.media{aspect-ratio:16/9}` (ok).

### `color-scheme-mismatch`
- **Guias**: `selector-atrule-combinations`.
- **Gatilho**: `__env.media['prefers-color-scheme']='dark'` enquanto `__document.meta['color-scheme']` ausente ou `light` e `styles.accessibility.color-scheme` ≠ `dark` no `:root`, com `contrast.ratio` de texto permanecendo no valor do tema claro sob `__meta.matrix` dark.
- **Evidência**: `prefers-color-scheme=dark; meta.color-scheme=ausente; :root.color-scheme=light only; contrast.ratio=… (tema claro)`.
- **Falso positivo / keepWhen**: não disparar quando `@media (prefers-color-scheme: dark)` em `declared` já redefine tokens; quando folha externa bloqueada, `evidence` deve dizer que a origem não pôde ser verificada (`blocked_sheets`).
- **Fallback/Baseline**: `color-scheme` Widely available (2022-02-03); combinação com `@container style()` é Newly available (2026-05-19). Para critical branching o guia recomenda duplicar regras (naive approach) — preservar.
- **Fixture mínima**: página com `:root{--color-scheme:light}` e `meta` ausente, capturada com e sem emulação dark.

### `size-containment-without-definite-block-size`
- **Guias**: `size-aware-styling`.
- **Gatilho**: `styles.custom['container-type']='size'` e `styles.box_model.block-size` em `auto|none` com descendentes cujo `rect.height ≈ 0`.
- **Evidência**: `container-type=size; block-size=auto; filho .card rect.height=0`.
- **Falso positivo / keepWhen**: não disparar para `container-type: inline-size`; não disparar quando `block-size` é definido (mesmo via `min-block-size` + conteúdo). `container-type` é capturável hoje via `--props container-type`.
- **Fallback/Baseline**: Container queries (size) Widely available (2023-02-14); fallback é layout empilhado + `@media`, preservado em `@supports (container-type: inline-size)`.
- **Fixture mínima**: `.wrap{container-type:size}` sem altura, com `.card` dentro, vs `.wrap{container-type:size;block-size:300px}`.

### `has-user-invalid-without-aria-invalid`
- **Guias**: `style-parent-with-has`.
- **Gatilho**: `declared.selectors` com `:has(:user-invalid)` casando um `.field-group` cujo `border-left-color` computado mudou para a cor de erro, e campo descendente com `aria.aria_invalid` ausente ou `"false"`.
- **Evidência**: `selector=.card-section:has(:user-invalid); border-left-color=rgb(217,48,37); input#username aria-invalid=ausente`.
- **Falso positivo / keepWhen**: não disparar quando o script fallback já sincronizou `aria-invalid="true"`; não disparar quando o campo não é `input|select|textarea`; `:user-invalid` é Baseline widely (2023-11-02) e o guia recomenda `aria-invalid="false"` como estado inicial — portanto ausência total é o sinal.
- **Fallback/Baseline**: Fallback MANDATORY via `WeakMap` + `aria-invalid`/`aria-describedby` e live region; o check existe para proteger esse fallback, não para substituí-lo.
- **Fixture mínima**: `<form><div class=card-section><input required></div></form>` após blur com valor vazio, com e sem `aria-invalid="true"`.

### `scrollbar-contrast-ignores-prefers-contrast`
- **Guias**: `adapt-scrollbar-to-contrast-preferences`.
- **Gatilho**: `styles.interaction.scrollbar-color` com dois `<color>` resolvidos; sob `__meta.matrix` com `__env.media['prefers-contrast']='more'`, o par thumb/track é idêntico ao estado `no-preference`.
- **Evidência**: `prefers-contrast=no-preference: thumb=#707070 track=#f1f1f1; emulate more: thumb=#707070 track=#f1f1f1 (inalterado)`.
- **Falso positivo / keepWhen**: não disparar quando `scrollbar-color: auto`; quando thumb/track não resolvíveis (overlay transparente), `evidence` declara a limitação; não disparar se o par já atinge contraste extremo sem override.
- **Fallback/Baseline**: `scrollbar-color` Newly available (2025-12-12); fallback `::-webkit-scrollbar` em `@supports not (scrollbar-color: auto)` deve ser preservado, com `scrollbar-width: auto` resetado.
- **Fixture mínima**: `.scroller{scrollbar-color:#707070 #f1f1f1}` com e sem `@media (prefers-contrast: more){...#000/#fff}`, capturado nas duas emulações.

### `scrollbar-width-thin-or-none`
- **Guias**: `browser-ui-theming`.
- **Gatilho**: `styles.interaction.scrollbar-width` em `thin|none` em elemento rolável (`overflow` em `auto|scroll` via `styles.box_model`).
- **Evidência**: `scrollbar-width=thin; overflow-y=auto; tag=div`.
- **Falso positivo / keepWhen**: não disparar quando `auto`, nem quando existe mecanismo alternativo explícito de rolagem detectável (ex.: botões com `aria-label` de scroll); o guia permite `thin` se houver alternativa acessível.
- **Fallback/Baseline**: `scrollbar-width` Newly available (2024-12-11), mas o alerta é de usabilidade, não de suporte.Não duplicar `text-not-selectable`/`contrast-aa` (já existentes).
- **Fixture mínima**: `.list{overflow-y:auto;scrollbar-width:thin}` vs `scrollbar-width:auto`.

### `forced-colors-unhandled`
- **Guias**: `color`.
- **Gatilho**: sob `__meta.matrix` com `__env.forced-colors='active'`, `styles.accessibility.forced-color-adjust='none'` está ausente e um estado/indicador depende apenas de `box-shadow`, `background-image` ou `border-image` (valores em `styles.visual` que o modo forçado remove), sem `outline`/border com system-color no `declared`.
- **Evidência**: `forced-colors=active; box-shadow=0 0 0 2px #2563eb; outline-style=none; border-image-source=none → indicador removido`.
- **Falso positivo / keepWhen**: não disparar quando `forced-color-adjust: none` é usado com par texto/fundo que atende 4,5:1 (guia permite só quando cor é informação essencial); não disparar quando há `outline` ou border `CanvasText`/`LinkText`; `evidence` registra `emulated`.
- **Fallback/Baseline**: Forced Colors Mode é comportamento de SO, não feature CSS; `@media (forced-colors: active)` com system-color keywords é o fallback preservado. `contrast-aa` existente cobre o modo normal, não o forçado.
- **Fixture mínima**: badge cujo estado "erro" é só `box-shadow` vermelho, capturado em `forced-colors: none` e `active`.

### `mask-no-graceful-fallback`
- **Guias**: `complex-shapes`, `soft-edge-content-fade`, `visually-texture-content`.
- **Gatilho**: `styles.visual.mask-image` ≠ `none` e `declared` não contém `@supports (not (mask-image: …))` com `clip-path` (complex-shapes), overlay `::after` (soft-edge) ou `background-image` (textura) como alternativa; a máscara altera a visibilidade de texto/conteúdo interativo do nó.
- **Evidência**: `mask-image=url(mask.svg); mask-size=cover; @supports not (mask-image…)=ausente; aria.has_text=true`.
- **Falso positivo / keepWhen**: não disparar quando a máscara é decorativa e o conteúdo permanece legível/reutilizável sem ela; Masks são Baseline widely (2023-12-07) — o guia só recomenda fallback quando a perda da máscara quebra leitura ou funcionalidade; não confundir com `mask-image` vazio (browser sem suporte).
- **Fallback/Baseline**: fallback MANDATORY citado nos três guias via `@supports (not (mask-image…)) and (not (-webkit-mask-image…))`; o check existe para protegê-lo, não para exigir em caso puramente cosmético. `-webkit-mask-image` é capturável via `--props`.
- **Fixture mínima**: `.shaped{mask-image:url(#splat);-webkit-mask-image:url(#splat)}` com e sem `@supports not (mask-image: url(x)){clip-path: inset(5% round 15px)}`.

### `decoration-faked-with-border-or-gradient`
- **Guias**: `custom-text-decorations`.
- **Gatilho**: `tag='a'` com `attrs.href`, `styles.typography.text-decoration-line='none'`, e (`styles.visual.background-image` contendo `linear-gradient` **ou** `styles.box_model.border-bottom-width > 0`) com o resto das bordas em 0.
- **Evidência**: `a[href]; text-decoration-line=none; background-image=linear-gradient(...); border-bottom-width=1px`.
- **Falso positivo / keepWhen**: não disparar quando `text-decoration-line` contém `underline`; não disparar quando o border inferior é componente visual legítimo (ex.: card com borda), restringindo a links cujo único conteúdo é texto; `focus-indicator` existente continua responsável por focus cortado por `clip-path`.
- **Fallback/Baseline**:Não é feature nova; é anti-padrão de autoria. Propriedades modernas do guia (`text-decoration-inset`, `text-decoration-skip-ink: all`, `border-shape`, `background-clip: border-area`) são capturáveis via `--props`, mas a recomendação central (usar `text-decoration` nativo) não exige extensão.
- **Fixture mínima**: `<a href="#" style="text-decoration:none;border-bottom:1px solid">Link</a>` vs `<a href="#" style="text-decoration:underline">Link</a>`.

### `scrollbar-thumb-track-contrast`
- **Guias**: `customize-scrollbar-color-and-thickness` (compartilhado com `adapt-scrollbar-to-contrast-preferences` como verificação de resultado).
- **Gatilho**: `styles.interaction.scrollbar-color` com dois `<color>` opacos resolvidos; razão de contraste calculada entre thumb e track < `3.0`.
- **Evidência**: `scrollbar-color=rgb(112,112,112) rgb(241,241,241); ratio=2.44 (<3.0); WCAG 1.4.11`.
- **Falso positivo / keepWhen**: não disparar quando `auto`; quando um lado é transparente (overlay macOS), `evidence` declara que o track real é o fundo do contêiner e a razão não é verificável; se o fundo do contêiner der contraste ≥3:1 ao thumb, não disparar.
- **Fallback/Baseline**: `scrollbar-color` Newly available (2025-12-12); fallback `::-webkit-scrollbar` em `@supports not (scrollbar-color: auto)` preservado; o check não deve exigir contraste 3:1 dentro do bloco de fallback quando as cores são as mesmas. Critério normativo explícito (WCAG 1.4.11) justifica `fail`.
- **Fixture mínima**: `.scroller{scrollbar-color:#707070 #f1f1f1}` (fail) vs `.scroller{scrollbar-color:#000 #fff}` (ok).

### `dark-mode-ignored`
- **Guias**: `dark-mode`.
- **Gatilho**: `__env.media['prefers-color-scheme']='dark'`, `__document.meta['color-scheme']` ausente/`light` e `styles.accessibility.color-scheme` no `:root` sem `dark`, sob `__meta.matrix` dark; `styles.visual.background-color` do corpo permanece igual ao estado light.
- **Evidência**: `prefers-color-scheme=dark; meta.color-scheme=ausente; :root.color-scheme=light; body.background-color=rgb(255,255,255) em ambos estados`.
- **Falso positivo / keepWhen**: não disparar quando `@media (prefers-color-scheme: dark)` em `declared` redefine tokens; não disparar quando o autor declara `color-scheme: light` explicitamente (decisão detectável em `declared`); `color-scheme` é Widely available, `light-dark()` é Newly available (2024-05-13) — o check não deve exigir `light-dark()`.
- **Fallback/Baseline**: MANDATORY: `<meta name=color-scheme>` no head para evitar FOUC + `@media (prefers-color-scheme: dark)` como fallback de `light-dark()`; preservar a regra de `@supports (color: light-dark(white, black))`.
- **Fixture mínima**: página com `:root{--brand:#0056b3;color-scheme:light}` sem `meta`, capturada em light e dark.

### `text-fit-on-intrinsic-width-container`
- **Guias**: `equal-width-text-lines`.
- **Gatilho**: `styles.custom['text-fit']` ≠ `none` noHeading e `styles.box_model.inline-size` do mesmo nó (ou do lockup) em `fit-content|max-content|auto`; `declared` sem `@supports not (text-fit: grow)`.
- **Evidência**: `text-fit=grow per-line-all 300%; inline-size=fit-content → sem efeito`.
- **Falso positivo / keepWhen**: não disparar quando há fallback `--text-fit-scale` e `@supports not (text-fit: grow)`; só quando `text-fit` computado ≠ none e a largura é intrínseca.
- **Fallback/Baseline**: `text-fit` é Limited availability (Chrome/Edge 150; sem Firefox/Safari). Fallback MANDATORY para linhas forçadas: medir `--text-fit-scale` via `ResizeObserver`; linhas livres só progressive enhancement. Preservar.
- **Fixture mínima**: `.lockup{text-fit:grow per-line-all 300%;inline-size:fit-content}` (flag) vs `inline-size:100%` (ok).

### `drawable-without-geometry`
- **Guias**: `expose-canvas-content-to-browser-features`.
- **Gatilho**: nó com `attrs['drawable']` dentro de `<canvas attrs['content']='drawable'>` que possui `aria.focusable=true`/`ax.ignored=false` e `rect.width==0 || rect.height==0` (ou `rect` ausente).
- **Evidência**: `canvas content=drawable; div drawable; button focusable=true; rect=0x0; ax.ignored=false`.
- **Falso positivo / keepWhen**: não disparar quando o fallback põe o HTML sobre o canvas (rect > 0); não disparar para `drawable` decorativo sem conteúdo interativo; quando `__ax_tree` ausente, o check não emite (nunca "passa" por omissão).
- **Fallback/Baseline**: HTML-in-Canvas é Limited availability (Chrome/Edge 157; sem Firefox/Safari). Fallback = HTML sobreposto ao canvas via CSS; `2D drawElementImage()` sincroniza geometria automaticamente — o check só alerta quando a geometria não foi sincronizada.
- **Fixture mínima**: `<canvas content=drawable><div drawable><button>OK</button></div></canvas>` sem `updateElementGeometry()` (flag) vs com chamada (rect > 0).

### `text-wrap-balance-on-long-block`
- **Guias**: `improve-text-layout-and-legibility`.
- **Gatilho**: `styles.typography.text-wrap-mode='balance'` (ou `text-wrap` contendo `balance`) e `(rect.height / line-height_px) > 6`.
- **Evidência**: `text-wrap-mode=balance; line-height=24px; rect.height=264px → 11 linhas (>6); Chromium limit=6`.
- **Falso positivo / keepWhen**: não disparar quando `text-wrap-mode='pretty'`; não disparar quando linhas ≤ 6; quando o browser não suporta `balance`, o valor computado é vazio e o check não emite; nunca aplicar em `*` (isso é `declared`, ver `inline-nontrivial-style`).
- **Fallback/Baseline**: `text-wrap: balance` é Newly available (2024-05-13); `pretty` é Limited (sem Firefox). Fallback natural é `wrap` — preservar, sem JS salvo em layout crítico.
- **Fixture mínima**: `<p style="text-wrap:balance">` com 10 linhas de texto (flag) vs 3 linhas (ok).

### `drawable-3d-geometry-unverifiable`
- **Guias**: `interactive-content-in-3d-scenes`.
- **Gatilho**: com a nova facet `canvas.geometry` presente, nó `drawable` focável cujo registro de `updateElementGeometry`/`InteractionManager.add` está ausente; `attrs(content,drawable)` e `__ax_tree` confirmam a exposição.
- **Evidência**: `canvas content=drawable; drawable button; ax.focusable=true; canvas.geometry.registro=ausente → sem bounds/hit-test`.
- **Falso positivo / keepWhen**: não disparar quando a facet não foi capturada (facet ausente → sem emissão); não disparar para `drawable` não-interativo; fallback de HTML sobreposto ao canvas não exige geometria — não sinalizar nesse caso.
- **Fallback/Baseline**: HTML-in-Canvas Limited (Chrome/Edge 157). Fallback = HTML sobre o canvas; `THREE.HTMLTexture` + `InteractionManager` é a integração recomendada. `drawable-without-geometry` (#30) detecta só o caso 0×0; esta facet estende para o caso em que a projeção 3D altera o `rect` mesmo quando sincronizada.
- **Fixture mínima**: mesh Three.js com `HTMLTexture` e um `<button drawable>`, com e sem `interactions.add(mesh)`/`interactions.update()`.

### `text-box-on-long-form-text`
- **Guias**: `precise-text-alignment`.
- **Gatilho**: `styles.custom['text-box']` ≠ `none` em nó com `tag` em `p|blockquote|li` ou com `(rect.height / line-height_px) > 3`.
- **Evidência**: `text-box=trim-both cap alphabetic; tag=p; linhas=6`.
- **Falso positivo / keepWhen**: não disparar em `h1–h6`, `button`, `badge`, `tag`; só em texto corrido. `text-box` não herda — o valor vem do próprio nó.
- **Fallback/Baseline**: `text-box` é Limited availability (Chrome/Edge 133, Safari 18.2; sem Firefox) e é progressive enhancement sem código extra; o check não deve exigir fallback, apenas alertar uso em contexto errado.
- **Fixture mínima**: `<p style="text-box:trim-both cap alphabetic">` com várias linhas (flag) vs `<h1 style="text-box:…">` (ok).

### `mask-on-text-containing-element`
- **Guias**: `shaped-cutouts`.
- **Gatilho**: `styles.visual.mask-image` contendo `url(#…)` (referência a `<mask>` SVG) em nó com `aria.has_text=true` e nenhum irmão não-mascarado com o mesmo texto (padrão `.card-body` + `.card-accent` ausente).
- **Evidência**: `mask-image=url(#splat-mask); aria.has_text=true; irmão não-mascarado=ausente → risco de crop`.
- **Falso positivo / keepWhen**: não disparar quando o texto vive em irmão separado e não-mascarado; não disparar para máscara de gradiente que apenas recorta a borda (notch) sem cruzar glyphs; só para máscara SVG direta em nó com texto.
- **Fallback/Baseline**: Masks Widely available (2023-12-07); fallback = retângulo normal. Como o texto é mantido fora da máscara, o check protege a recomendação "Adjacent Element SVG Masking".
- **Fixture mínima**: `<div style="mask-image:url(#splat)">Texto longo</div>` (flag) vs `<div class=card-body>Texto</div><div class=card-accent style="mask-image:url(#stencil)"></div>` (ok).

### `justify-without-hyphens`
- **Guias**: `typography`.
- **Gatilho**: `styles.typography.text-align='justify'` em bloco com > 1 linha (`rect.height / line-height_px > 1`) e `styles.typography.hyphens` em `none|manual`.
- **Evidência**: `text-align=justify; hyphens=manual; linhas=5; aria.lang=en`.
- **Falso positivo / keepWhen**: não disparar em linha única, quando `hyphens:auto` já ativo, ou quando `lang` ausente (então `hyphens:auto` não funcionaria — `evidence` deve registrar `aria.lang=ausente`). Restringir a texto corrido, não a tabelas de dados curtas.
- **Fallback/Baseline**: sem feature nova; `hyphens:auto` depende de `lang` (WCAG 1.4.8/1.4.9 de fundo). Não duplicar `line-height-below-font-size`/`horizontal-overflow` existentes.
- **Fixture mínima**: `<p lang="en" style="text-align:justify;hyphens:manual">…várias linhas…</p>` (flag) vs `hyphens:auto` (ok).

### `blend-mode-without-isolation`
- **Guias**: `visual-effects`.
- **Gatilho**: `styles.visual.mix-blend-mode` ≠ `normal` e `styles.visual.isolation` ≠ `isolate` no próprio nó, havendo elementos irmãos/ancestrais em `path` que possam ser afetados.
- **Evidência**: `mix-blend-mode=soft-light; isolation=auto; path=body>main>.overlay`.
- **Falso positivo / keepWhen**: não disparar quando `isolation: isolate` está no escopo correto; ignorar `background-blend-mode` (escopo local, não precisa de isolamento); não disparar quando `mix-blend-mode: normal`.
- **Fallback/Baseline**: `mix-blend-mode` e `isolation` são Widely available; o guia recomenda `isolation: isolate` para limitar o escopo — o check pega a omissão. Nota: `in oklch` para neutros é anti-padrão citado, mas isso só é verificável em `declared` (fora do check primário).
- **Fixture mínima**: `.overlay{mix-blend-mode:soft-light}` sem `isolation` (flag) vs com `isolation:isolate` (ok).

### `font-size-adjust-missing-fallback`
- **Guias**: `visually-stable-font-fallbacks`, `visually-stable-mixed-fonts`.
- **Gatilho**: `declared` define `@font-face` com `font-family: X` usado em `styles.typography.font-family` de texto com `font-size-adjust='none'`; para fontes mistas, dois `font-family` distintos na subtree sem `font-size-adjust`. Ausência de `@supports not (font-size-adjust: from-font)` com `line-height`/`font-size` alternativos (#39) ou de `size-adjust`/`ascent-override` no `@font-face` (#38).
- **Evidência**: `@font-face font-family=MyWebFont; .text font-family=MyWebFont; font-size-adjust=none; @supports not=ausente`.
- **Falso positivo / keepWhen**: não disparar quando `font-size-adjust` é `from-font` ou numérico; não disparar quando `font-display: optional` (sem swap); não disparar quando `size-adjust`/`ascent-override`/`descent-override` preservam o fallback; `#38` e `#39` compartilham o check mas têm `evidence` distinta (webfont vs mixed).
- **Fallback/Baseline**: `font-size-adjust` é Newly available (Baseline 2024-07-25). Fallback MANDATORY em `#39`: `@supports not (font-size-adjust: from-font)` com `line-height`/`font-size`; em `#38`, `@font-face size-adjust`/overrides. O check protege esses fallbacks.
- **Fixture mínima**: `@font-face{font-family:X;src:url(x.woff2)}` + `.t{font-family:X}` sem `font-size-adjust` (flag); variante com `font-size-adjust:from-font` e com `@supports not` (ok).

## Resumo da fatia

- Contagens: D=15 F=20 N=4 E=1 (total 40). Todos os 40 ids da fatia foram cobertos — nenhuma linha omitida.
- Top 5 oportunidades priorizadas (impacto × custo):
  1. `scrollbar-thumb-track-contrast` (D, `fail`, WCAG 1.4.11) — custo zero (já em `styles.interaction`), critério normativo explícito e falha comum em temas.
  2. `visual-order-breaks-tab-order` (D) — custo zero (`styles.layout` + `aria.focusable`), WCAG 2.4.3, impacto alto em navegação por teclado.
  3. `overflow-clip-margin-ignored` (D) — custo zero, detecta declaração silenciosamente ignorada pelo browser (erro real de renderização).
  4. `text-wrap-balance-on-long-block` (D) — custo zero, pega anti-padrão de performance documentado no guia com limite determinístico (>6 linhas).
  5. `has-user-invalid-without-aria-invalid` (F, `declared`, 1 linha global) — alta utilidade em forms; completa a lacuna entre `:user-invalid` nativo e ARIA que o próprio guia destaca.
- Guias que exigem novas facets, agrupados por facet:
  - **`declared` (CSSOM: regras casadas, `selectorText`, `declarations`, `origin`, at-rules)**: `calculate-with-intrinsic-sizes`, `child-state-based-styling`, `content-based-styling`, `css`, `design-token-reactivity`, `dynamic-sibling-styling`, `fluid-scaling`, `selector-atrule-combinations`, `style-parent-with-has`, `usage-aware-component-variations`, `complex-shapes`, `soft-edge-content-fade`, `visually-stable-font-fallbacks`, `visually-stable-mixed-fonts`, `visually-texture-content`. (15 guias; custo: 1 linha global + passada in-page limitada por `--declared-limit`.)
  - **`declared.at_rules` (extensão do `declared` para enumerar prelúdios de `@container style`, `@supports`, `@media`, `@function`)**: `design-token-reactivity`, `usage-aware-component-variations`, `selector-atrule-combinations`, `dynamic-sibling-styling` — necessário porque o `declared` atual só coleta `CSSStyleRule`; custo: recursão adicional sobre `document.styleSheets`, ainda 1 linha global.
  - **`__env` + `__meta.matrix` (preferências do usuário / segunda captura emulada)**: `animate-to-intrinsic-sizes`, `selector-atrule-combinations`, `adapt-scrollbar-to-contrast-preferences`, `color`, `dark-mode`. (`fluid-scaling` pode usar matrix como reforço opcional.) Custo: 1 linha global `__env` + N×captura para a matriz.
  - **`__document` (meta/title/lang)**: `selector-atrule-combinations`, `dark-mode`. Custo: 1 linha global.
  - **`canvas.geometry` (nova facet por nó, via ponte JS do canvas)**: `interactive-content-in-3d-scenes`. Custo: novo campo por nó, uma leitura extra da API do canvas — a mais cara da fatia.
  - **`attrs` (já existe, mas requer `--attr`/preset — listado para explicitar o opt-in)**: `responsive-design`, `expose-canvas-content-to-browser-features`, `interactive-content-in-3d-scenes`, `custom-text-decorations`.
- Observação de Baseline: quase todas as oportunidades F protegem um `@supports`/fallback — os `keepWhen` acima foram escritos para não penalizar a declaração de fallback, em conformidade com o contrato "proteger o fallback" do modern-web-guidance.
