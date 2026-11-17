# Findings — ui-components + ui-atoms
> **Correção de nomenclatura (consolidação v0.6):** a flag real é `--attrs`
> (`value_delimiter=','`, repetível). Onde este relatório diz `--attr`, leia
> `--attrs`.

Escopo: 35 guias lidos integralmente de `ui-components` (22) e `ui-atoms` (13). O inventário posiciona o `sniff-css` como lado **runtime** do modern-web-guidance: quase todo guia desta fatia prescreve uma tecnologia cuja aplicação (ou ausência) deixa rasto em `styles`/`attrs`/`ax`/árvore DOM, mesmo quando a intenção do autor ou o comportamento do event handler não deixa.

Convenções usadas abaixo: `styles.<cat>.<prop>` vem dos 8 grupos já capturados; `attrs` significa `--attr NOME` (facet existente, opt-in); `--props NOME` é o caminho de propriedades arbitrárias já presente no catálogo (`properties_for(categories, custom)`) e é tratado aqui como **extensão mínima de config**, não como facet nova. `__env`, `__document`/agregados de DOM, `declared` (CSSOM) e `__actions` são as facetas propostas no contrato.

| # | guia | classe | sinal detectável / razão do N | check_id proposto | facets exigidas | severidade | keepWhen / dropWhen |
|---|---|---|---|---|---|---|---|
| 1 | `accordion` | D | `accordion`: "Build accordions from a set of adjacent `details` elements." Grupo de ≥2 gatilhos com `aria.aria_expanded` e painel irmão, sem nenhum `DETAILS`/`SUMMARY` na subárvore = reinventou o widget. Sinal secundário: `SUMMARY` com descendente `H1`–`H6` (JAWS/TalkBack descartam o heading). | `accordion-custom-instead-of-details` | `aria`, árvore (`tag`, `children`) | warn | Não disparar quando existir `DETAILS`/`SUMMARY` no grupo; não disparar o sinal de heading se o `summary` contiver só texto/icon decorativo. |
| 2 | `app-menus` | D | `app-menus`: "Put `role=\"menubar\"` with an `aria-label` on the container". `aria.role == "menubar"` com >1 descendente direto `tabindex="0"` quebra o "one tab stop" — sinal determinístico de roving tabindex mal implementado. | `menubar-multiple-tabstops` | `aria`, `attrs` (`--attr tabindex`) | warn | Não disparar em barra com exatamente um `tabindex="0"`; não disparar em `role="menubar"` desativado/`hidden`. |
| 3 | `breadcrumbs` | D | `breadcrumbs`: "Mark the current page, the last item, as a link with `aria-current=\"page\"`." Num `NAV` cujo `aria.name` contém "breadcrumb" (case-insensitive) com ≥2 links, nenhum link com `attrs["aria-current"] == "page"` (`aria-current` não está em `AriaInfo`; exige `--attr aria-current`). | `breadcrumb-current-page-missing` | `aria.name`, `attrs` (`--attr aria-current`) | warn | Não disparar em `NAV` sem nome "breadcrumb"; não disparar se o item final for texto (trilha de uma página). |
| 4 | `card` | D | `card`: "Do not nest other links, buttons, or form controls inside the link". `A` com descendente `A`/`BUTTON`/`INPUT`/`SELECT`/`TEXTAREA` na árvore = aninhamento interativo inválido. | `card-interactive-nested-in-link` | árvore (`tag`, `children`) | warn | Não disparar quando o link só envolve `ARTICLE` com conteúdo não interativo; não disparar em `<label>` de choice card. |
| 5 | `carousel` | D | `carousel`: "Do not use `::scroll-button()` or `::scroll-marker` for now." `pseudo[].name` contendo `scroll-button`/`scroll-marker`. Sinal secundário: scroller com `styles.interaction.scroll-snap-type` contendo `x` mas `overscroll-behavior-x` ausente/diferente de `contain`. | `carousel-native-scroll-controls` | `pseudo`, `styles.interaction` | warn | Não disparar em scroller sem snap; não disparar se o projeto declara explicitamente suporte experimental e há controles HTML paralelos. |
| 6 | `checkbox` | D | `checkbox`: "Use `appearance: none` when the design requires control over the..." — e o risco maior: `aria.role == "checkbox"`/`"radio"` em `DIV`/`SPAN` com `aria.aria_checked`, sem `INPUT`. Segundo sinal: `INPUT[type=checkbox]` com `styles.interaction.appearance == "none"` e `styles.visual.outline-style == "none"` (foco suprimido). | `native-checkbox-reimplemented` | `aria`, `tag`, `styles.interaction`, `styles.visual`, `attrs` (`--attr type`) | warn | Não disparar em `role="checkbox"` dentro de `role="menuitemcheckbox"` (menu); não disparar se houver `INPUT` nativo equivalente no mesmo `label`. |
| 7 | `copy-button` | D | `copy-button`: "DO NOT add `aria-hidden`, `inert` or `hidden` to the button to 'hide' it". `aria.aria_hidden` presente num botão com `command="--copy"` (via `--attr command`) ou com irmão `role="status"`. | `copy-control-hidden-from-at` | `aria`, `attrs` (`--attr command`) | warn | Não disparar quando o `aria-hidden` estiver no ícone decorativo, não no botão. |
| 8 | `dark-mode-toggle` | F | `dark-mode-toggle`: "the base declaration MUST stay `light dark`". O `styles.accessibility.color-scheme` computado na raiz é `dark` tanto quando o autor fixou o padrão quanto quando o `:has(> head > meta[content=\"dark\"])` aplicou o override — só CSSOM (`declared`) distingue. | `root-color-scheme-hardcoded` | `styles.accessibility` + `declared` (regra base casada em `:root`) | warn | Não disparar quando a declaração base for `light dark` e o valor computado vier de `:has(meta)`; não disparar em raiz com `color-scheme: light dark` explícito. |
| 9 | `dialog` | D | `dialog`: "no role=\"dialog\" or aria-modal; the native modal already provides them". `DIV`/`SPAN` com `aria.role == "dialog"` ou `"alertdialog"` e sem `DIALOG` na subárvore = overlay reimplementado. Sinal secundário: `DIALOG` nativo com `aria-modal="true"` redundante (`--attr aria-modal`). | `native-dialog-reimplemented` | `tag`, `aria`, `attrs` (`--attr aria-modal`, `--attr popover`) | warn | Não disparar no drawer que "also acts as a sidebar" (`popover="manual"` + `role="dialog"` é prescrito); não disparar em tour popover com `role="dialog"`; não disparar em `dialog[popover]` legítimo. |
| 10 | `hovercard` | D | `hovercard`: "DO NOT move focus into the target when interest is shown (no `autofocus`, no `focus()` in an `interest` listener)" — WCAG 3.2.1. `autofocus` (`--attr autofocus`) num nó com `popover="hint"` ou `role="tooltip"`. Sinal secundário: gatilho `A` com `aria.aria_expanded` e tooltip (o guia diz "never `aria-expanded`" para link). | `hovercard-autofocus-on-hint` | `attrs` (`--attr autofocus`, `--attr popover`, `--attr interestfor`), `aria` | fail | Não disparar em toggletip (`popovertarget` + `role="tooltip"`), onde `aria-expanded` é correto; não disparar se `autofocus` estiver em `<dialog>` modal. |
| 11 | `menu` | D | `menu`: "Use a `<div focusgroup=\"menu nomemory\">`, not the HTML `<menu>` element." `aria.role == "menu"` em recipiente sem `focusgroup` mas com filhos `role="menuitem"` e `tabindex` = menu reimplementado; `tag == "MENU"`; `popovertarget` no gatilho (guia exige `commandfor`); `<div>` separador em vez de `HR`. | `menu-composite-reimplemented` | `tag`, `aria`, `attrs` (`--attr focusgroup`, `--attr popovertarget`, `--attr command`) | warn | Não disparar no fallback explícito de `focusgroup` (script feature-detect); não disparar em `role="menu"` de apply-to site-nav (esses são `NAV`, ver #12). |
| 12 | `navbar` | D | `navbar`: "links stay in the `Tab` order, with no `focusgroup`, `role=\"menubar\"`, `role=\"menu\"`, `role=\"menuitem\"`, `aria-haspopup=\"menu\"`". `NAV` ou descendente com `aria.role` nesse conjunto, ou com `attrs aria-haspopup`. | `site-nav-using-menu-roles` | `tag`, `aria`, `attrs` (`--attr aria-haspopup`, `--attr focusgroup`) | warn | Não disparar em `role="menubar"` fora de `NAV` (app-menus legítimo); não disparar quando o `NAV` contém só links de site e nenhum papel de menu. |
| 13 | `navigation-drawer` | D | `navigation-drawer`: "replace top-layer promotion with `position: fixed` and a high `z-index`, replace `::backdrop` with a sibling element" — detector: elemento quase viewport (`rect` ≈ `__meta`) com `styles.layout.position == "fixed"`, `styles.layout.z-index` alto, irmão `::backdrop`-like, e nenhum `DIALOG`/`popover` na subárvore. | `drawer-reimplemented-with-fixed-overlay` | `rect`, `styles.layout`, `styles.visual`, `__meta`, `pseudo`, `attrs` (`--attr popover`) | warn | Não disparar em banner/cookie fixo que não cubra a viewport; não disparar em `dialog[open]` legítimo. |
| 14 | `persistent-app-tours` | D | `persistent-app-tours`: "Mark the container with `role=\"dialog\"` and link its heading via `aria-labelledby`." Popover `popover="manual"` com `role="dialog"` mas `aria.aria_labelledby` ausente e `aria.name` vazio. | `tour-popover-unnamed` | `attrs` (`--attr popover`), `aria` | warn | Não disparar se o popover tiver `aria-label`; não disparar em toast `popover="manual"` sem `role="dialog"` (usa `role="region"` no contêiner). |
| 15 | `persistent-toast-notifications` | F | `persistent-toast-notifications`: "Pre-populated hidden live region container" — a ausência de `aria-live`/`role="status"` no carregamento **não** aparece num snapshot de nós casados; exige agregado global do DOM/`__document`. Sinal secundário já D: toast `popover="manual"` sem botão de fechar na subárvore. | `toast-live-region-missing-at-load` | `__document` / agregado DOM (contagem de `[aria-live]`) + `attrs` (`--attr aria-live`) | warn | Não disparar quando `ariaNotify()` for usado (guia aceita); não disparar se o live region for injetado por framework antes do snapshot (o agregado captura o estado real). |
| 16 | `progress` | D | `progress`: "Use a native `<progress>` element: It provides built-in accessibility". `aria.role == "progressbar"` em `DIV`/`SPAN` com `aria-valuenow` (`--attr aria-valuenow`) e sem `PROGRESS` = barra reimplementada. | `native-progress-reimplemented` | `tag`, `aria`, `attrs` (`--attr aria-valuenow`, `--attr value`) | warn | Não disparar em `role="progressbar"` dentro de `role="menuitem"`; não disparar se houver `PROGRESS` nativo no mesmo subtree. |
| 17 | `progress-ring` | D | `progress-ring`: "uses the semantic `<progress>` element rather than ARIA". Mesmo sinal de #16 aplicado a anel: `SVG` ou `DIV` com `role="progressbar"`/`"img"` e `conic-gradient` em `styles.visual.background-image`, sem `PROGRESS`. Secundário: texto central com valor sem `aria.aria_hidden`. | `native-progress-reimplemented` | `tag`, `aria`, `styles.visual`, `styles.css_variables` | warn | Não disparar quando o `role="img"` for decorativo com `aria-hidden="true"`; não disparar se `PROGRESS` nativo existir. |
| 18 | `scrollspy` | F | `scrollspy`: "Apply the `scroll-target-group: auto` property to the container" — propriedade fora dos 8 grupos; capturável via `--props scroll-target-group`. Complemento: links `href="#..."` sem `aria-current` (`--attr href`, `--attr aria-current`). | `scrollspy-without-scroll-target-group` | `--props scroll-target-group`, `attrs` (`--attr href`, `--attr aria-current`) | warn | Não disparar quando houver fallback `IntersectionObserver` que já mantém `aria-current` sincronizado (sinal `attrs["aria-current"]` presente); não disparar em `NAV` com links externos. |
| 19 | `spinner` | D | `spinner`: "By omitting the `value` attribute, the browser treats it as an indeterminate progress bar". Mesmo `native-progress-reimplemented` para `DIV` com `role="progressbar"` + `styles.animation.animation-iteration-count == "infinite"` e `styles.transform.rotate`/`translate`. Secundário F: `PROGRESS` indeterminado com `aria-valuenow` explícito, ou giro rápido sob `__env reduced-motion: reduce`. | `native-progress-reimplemented` | `tag`, `aria`, `styles.animation`, `styles.transform`, `attrs` (`--attr value`, `--attr aria-valuenow`), `__env` (variante) | warn | Não disparar em animação de carregamento puramente decorativa com `aria-hidden="true"`; não disparar quando `PROGRESS` nativo indeterminado existir. |
| 20 | `stack-drill-down` | D | `stack-drill-down`: "apply `inert` to every view except the currently visible one". Num scroller com `styles.interaction.scroll-snap-type` contendo `x`, ≥2 filhos diretos **sem** `--attr inert` = vazamento de foco/AT para views ocultas. | `snap-stack-multiple-active-views` | `styles.interaction`, `attrs` (`--attr inert`), árvore (`children`) | warn | Não disparar em carrossel de slides (usa `role="group"`, não stack de navegação); não disparar se houver só um filho. |
| 21 | `tabs` | D | `tabs`: "we can use `focusgroup` which has a `tablist` behavior". `aria.role == "tablist"` sem `--attr focusgroup` mas com `tabindex` nos filhos = tablist reimplementado. Sinais secundários: `aria.aria_controls` em tab (guia: "Do not add `aria-controls`"); ≥2 `role="tabpanel"` sem `--attr hidden`. | `tabs-reimplemented-with-roving-tabindex` | `aria`, `tag`, `attrs` (`--attr focusgroup`, `--attr tabindex`, `--attr hidden`, `--attr aria-selected`) | warn | Não disparar no fallback de polyfill (`'focusGroup' in HTMLElement.prototype === false`); não disparar em tabpanel único. |
| 22 | `toolbar` | D | `toolbar`: "Put `focusgroup=\"toolbar wrap\"` on the `role=\"toolbar\"` container". `aria.role == "toolbar"` com >1 nó `tabindex="0"` nos controles, ou sem `focusgroup` e com `tabindex` manual. | `toolbar-roving-tabindex-broken` | `aria`, `attrs` (`--attr focusgroup`, `--attr tabindex`) | warn | Não disparar quando o fallback de polyfill estiver ativo; não disparar em toolbar com um único controle. |
| 23 | `carousel-slide-effects` | F | `carousel-slide-effects`: "Only runs the effect when the user has no reduced-motion preference". `styles.animation.animation-timeline` contendo `view(` num slide de scroller; sob `__env reduced-motion: reduce` o efeito ainda estar ativo é a violação. O `@supports`/`prefers-reduced-motion` em si é authoring-time (N), mas o estado emulado é observável. | `scroll-effect-under-reduced-motion` | `styles.animation`, `styles.interaction`, `__env` | warn | Não disparar quando o efeito for puramente decorativo e `@media` já remove a animação (emulação confirma); não disparar em `animation-timeline: scroll()` de progresso. |
| 24 | `component-specific-light-dark-theme` | D | `component-specific-light-dark-theme`: "Do not set `color-scheme` on elements without a background". `styles.accessibility.color-scheme` ∈ {`dark`,`light`,`only dark`,`only light`} num nó não-`HTML` com `styles.visual.background-color` transparente/`rgba(0,0,0,0)`/`none`. | `color-scheme-without-background` | `styles.accessibility`, `styles.visual`, `tag` | warn | Não disparar em `:root`/`HTML`; não disparar quando o elemento tiver `background-image`/`background-color` opaco; não disparar em iframe (fora do snapshot). |
| 25 | `icons` | D | `icons`: "`forced-color-adjust: none`: In forced-colors mode the browser otherwise replaces `background-color`, making a masked icon disappear." `pseudo[]` com `mask-image` ≠ `none` e `forced-color-adjust != none`. Secundário: `IMG` com `styles.visual.filter` recolorindo. | `masked-icon-missing-forced-color-adjust` | `pseudo`, `styles.visual`, `attrs` (`--attr role`, `--attr aria-label`) | warn | Não disparar em `mask-image` decorativo fora de pseudo; não disparar quando `__env forced-colors: active` mostrar que o ícone continua visível por outro mecanismo. |
| 26 | `position-aware-tooltips` | N | Tudo o que o guia acrescenta é authoring-time: `@container anchored(fallback: flip-block)`, a troca do glifo de `▼` para `▲` e a escolha do fallback `position-try`. O snapshot expõe só o **resultado final** (`styles.visual.content` no pseudo), sem campo que diga qual fallback está ativo nem se o glifo corresponde a ele; `position-area`/`position-try-fallbacks` não estão nos 8 grupos. A preocupação WCAG 2.4.11 exige estado de foco pós-interação (`__actions`), que não existe hoje. | — | — | — | — |
| 27 | `pull-to-reveal` | F | `pull-to-reveal`: "DO NOT set `scroll-initial-target` on multiple elements within the same scrollable container." Propriedade fora dos 8 grupos, capturável via `--props scroll-initial-target` em todos os nós do mesmo scroller; >1 com `nearest` é determinístico. | `multiple-scroll-initial-targets` | `--props scroll-initial-target`, `styles.box_model` (overflow), árvore | warn | Não disparar quando os alvos estiverem em scrollers distintos; não disparar se um fragmento de URL/`scroll-start` tiver precedência (o estado renderizado já reflete). |
| 28 | `responsive-table` | D | `responsive-table`: "Give the wrapper a `role=\"region\"` attribute... accessible name". Wrapper com `styles.box_model.overflow-x/y` = `auto`/`scroll`, `--attr tabindex="0"` e `aria.role`/`aria.name` ausentes. | `scroll-region-without-name` | `styles.box_model`, `attrs` (`--attr tabindex`), `aria`, `tag` | warn | Não disparar em `<main>` (já tem papel implícito); não disparar se o wrapper tiver `aria-labelledby` apontando para a `<caption>`. |
| 29 | `scroll-position-aware-elements` | F | `scroll-position-aware-elements`: "Listen for `focusout` and move focus to the scrollable region when the hidden control loses focus". A perda de foco só existe após interação; exige `__actions`/estado de `activeElement`. Sinal estático de apoio: `styles.visual.visibility == "hidden"` + `styles.visual.opacity == "0"` num controle `position: sticky`/`fixed`. | `hidden-scroll-control-focus-loss` | `__actions` (foco pós-scroll), `styles.visual`, `styles.layout` | warn | Não disparar quando o controle usar `display: none` legítimo fora da viewport; não disparar se o fallback sempre-visível estiver ativo. |
| 30 | `scroll-progress-indicator` | D | `scroll-progress-indicator`: "If the indicator contains text or images, apply `aria-hidden=\"true\"`". Elemento com `styles.animation.animation-timeline` contendo `scroll(`, `aria.aria_hidden` ausente e `aria.has_text == true` (ou descendente `IMG`/`SVG`). | `decorative-progress-exposed-to-at` | `styles.animation`, `aria`, `tag`, `pseudo` | warn | Não disparar quando o indicador for um `<progress>` semântico com rótulo; não disparar se o texto for um status intencional (`role="status"`). |
| 31 | `scrollability-affordance-hints` | D | `scrollability-affordance-hints`: "If the scrollbar is hidden, the hints are the only cue... MUST use a fallback". Scroller com `styles.interaction.scrollbar-width == "none"` e `pseudo[]` (`::before`/`::after`) com `opacity == 0` e nenhum `animation-timeline`/`container-type: scroll-state` ativo. | `hidden-scrollbar-without-affordance` | `styles.interaction`, `styles.layout`, `pseudo`, `styles.animation` | fail | Não disparar quando a barra de rolagem estiver visível (dica é suplementar); não disparar se um fallback `IntersectionObserver` tiver aplicado classe `.more-start/.more-end` com `opacity: 1`. |
| 32 | `shrinking-header-on-scroll` | D | `shrinking-header-on-scroll`: "set `scroll-padding-top` to ensure that when elements receive focus, they aren't displayed under the sticky content" (WCAG 2.4.11). Header com `styles.animation.animation-timeline` contendo `scroll(` e `styles.layout.position == "fixed"`/`"sticky"`, enquanto a raiz não tem `styles.interaction.scroll-padding-top` ≥ altura do header. | `scroll-shrink-header-missing-scroll-padding` | `styles.animation`, `styles.layout`, `styles.interaction`, `rect`, `__meta` | warn | Não disparar se o conteúdo não tiver controles focáveis abaixo do header; não disparar quando `scroll-padding-top` for animado na mesma timeline e o snapshot emulado mostrar valor suficiente. |
| 33 | `single-line-overflow` | E | `single-line-overflow`: "Use `overflow: hidden`, not `clip`". O check `ellipsis-without-clip` já cobre `text-overflow: ellipsis` sem `overflow` de recorte e sem `white-space: nowrap` — exatamente a violação central do guia. Nada mudaria na detecção; o guia só adiciona o *keepWhen* de que `overflow: hidden` é necessário para a detecção de truncamento por scroll timeline. | `ellipsis-without-clip` (existente) | `styles.typography`, `styles.box_model` | warn | Manter o keepWhen atual do check: não disparar se `overflow` ∈ {hidden, clip, auto, scroll} **ou** `white-space` ∈ {nowrap, pre}. |
| 34 | `state-aware-sticky-headers` | D | `state-aware-sticky-headers`: "Disable scroll anchoring on the parent of the `position: sticky` element". `styles.layout.position == "sticky"` + `styles.layout.container-type` contendo `scroll-state` num nó cujo pai direto **não** tem `styles.box_model.overflow-anchor == "none"` → oscilação. | `stateful-sticky-header-without-overflow-anchor-none` | `styles.layout`, `styles.box_model`, árvore (`parent_id`) | warn | Não disparar em header preso ao fundo (guia: `overflow-anchor: none` não ajuda); não disparar se o pai não tiver conteúdo em fluxo abaixo do sticky. |
| 35 | `viewport-aware-dropdowns` | D | `viewport-aware-dropdowns`: "do not add `aria-haspopup` to the trigger — its value ... is a promise that the target exposes a matching role". Gatilho com `attrs aria-haspopup` cujo alvo (`--attr popovertarget`/`id`) tem `aria.role` fora de {menu, listbox, tree, grid} ou nenhum papel. | `haspopup-without-matching-role` | `attrs` (`--attr aria-haspopup`, `--attr popovertarget`, `--attr id`), `aria`, `tag` | warn | Não disparar quando o alvo for `role="menu"` completo com navegação por setas; não disparar em `aria-haspopup="dialog"` com `popover` que implemente `role="dialog"`. |

## Detalhes dos checks propostos (só D e F)

### `accordion-custom-instead-of-details`
- **Guias**: `accordion` (+ sub-sinal `accordion-heading-inside-summary`).
- **Gatilho**: agrupar irmãos por `parent_id`; se ≥2 nós com `aria.aria_expanded != null` e um painel irmão referenciado por `aria.aria_controls`, e nenhum `DETAILS` na subárvore do grupo →warn. Sub-sinal: nó `SUMMARY` com descendente `tag ∈ {H1..H6}` →warn.
- **Evidência**: `"custom accordion: N aria-expanded triggers, 0 <details> in group"` / `"heading inside <summary> is dropped by JAWS/TalkBack"`.
- **Falso positivo / keepWhen**: o guia permite accordion customizado quando cada item precisa de heading próprio — o sinal é structure-only e não pode inferir essa intenção, então manter `warn` e não `fail`; não disparar em único disclosure (`<button aria-expanded>` + painel) nem em `details`/`summary` nativos.
- **Fallback/Baseline**: `details name` é *Newly available* desde 2024-09-03 com fallback "vários itens abertos"; a sugestão de migrar para `details` preserva esse fallback. `::details-content` (2025-09-16) é progressive enhancement.
- **Fixture mínima**: `<section><h2 id=h>FAQ</h2><h3><button aria-expanded="false" aria-controls="a1">Q1</button></h3><div id="a1">…</div>…</section>` e `<details><summary><h3>Q</h3></summary>…</details>`.

### `menubar-multiple-tabstops`
- **Guias**: `app-menus`.
- **Gatilho**: nó com `aria.role == "menubar"`; contar descendentes diretos com `attrs["tabindex"] == "0"`. Se contagem > 1 →warn. Secundário: nenhum descendente com `tabindex="0"` e menu fechado →warn (tab stop perdido).
- **Evidência**: `"role=menubar has N tabbable triggers (expected exactly 1)"`.
- **Falso positivo / keepWhen**: não disparar quando o APG aceita mudar o tab stop após Arrow Left/Right — mas isso ainda mantém exatamente um `0` por vez, então o gatilho continua válido; não disparar em `role="menubar"` com `aria-hidden`.
- **Fallback/Baseline**: focusgroup é *Limited availability* (Chrome/Edge 150); o fallback do guia instala roving tabindex manual — o check deve continuar a valer nesse fallback.
- **Fixture mínima**: `<div role="menubar" aria-label="Commands"><button tabindex="0" aria-expanded="false">File</button><button tabindex="0">Edit</button></div>`.

### `breadcrumb-current-page-missing`
- **Guias**: `breadcrumbs`.
- **Gatilho**: `tag == "NAV"` com `aria.name` casando `/breadcrumb/i`, ≥2 descendentes `A`, e nenhum com `attrs["aria-current"] == "page"` →warn. (Exige `--attr aria-current`, ausente em `AriaInfo`.)
- **Evidência**: `"breadcrumb nav has N links but no aria-current=\"page\""`.
- **Falso positivo / keepWhen**: não disparar quando a trilha termina em texto não-link (o guia recomenda manter link, mas é escolha de autor); não disparar em `NAV` rotulado de outra forma (ex.: "Primary") — a identificação é conservadora de propósito.
- **Fallback/Baseline**: `corner-shape` é *Limited availability* com fallback "segmentos retos com separador"; o check não depende da forma visual, apenas de `aria-current`, então não muda com o fallback.
- **Fixture mínima**: `<nav aria-label="Breadcrumbs"><ol><li><a href="/">Home</a></li><li><a href="/docs">Docs</a></li></ol></nav>`.

### `card-interactive-nested-in-link`
- **Guias**: `card`.
- **Gatilho**: para cada `A`, checar descendentes com `tag ∈ {A, BUTTON, INPUT, SELECT, TEXTAREA}` →warn. Opcional: se o link tiver `pseudo[]` com `position:absolute; inset:0` e outro interactivo sem `styles.layout.position == "relative"` →warn (overlay captura clique).
- **Evidência**: `"card link wraps N interactive descendants"` / `"stretched-link overlay covers sibling controls"`.
- **Falso positivo / keepWhen**: não disparar em choice card (`LABEL` com `INPUT` filho); não disparar quando o descendente for `<span role="button">` decorativo? — nesse caso disparar, pois é pior; manter apenas tags nativas no gatilho.
- **Fallback/Baseline**: `appearance: base-select` (mencionado no guia) tem fallback para `<select>` custom; o check não depende dele.
- **Fixture mínima**: `<a href="/x"><article class="card"><h3>Title</h3><button>Act</button></article></a>`.

### `carousel-native-scroll-controls`
- **Guias**: `carousel`.
- **Gatilho**: (a) `pseudo[].name` contém `scroll-button` ou `scroll-marker` →warn; (b) nó com `styles.interaction.scroll-snap-type` contendo `x` e `styles.interaction.overscroll-behavior-x` ausente ou ≠ `contain` →warn separado (`carousel-snap-without-overscroll-contain`).
- **Evidência**: `"carousel uses ::scroll-marker; HTML buttons required for now"` / `"horizontal snap track has no overscroll-behavior-x: contain"`.
- **Falso positivo / keepWhen**: não disparar (b) em scroller vertical (usar `overscroll-behavior-y`); não disparar (a) se existirem `<button>` de prev/next e marcadores HTML no mesmo `section` — nesse caso o guia ainda desaconselha, mas o risco é menor; manter `warn`.
- **Fallback/Baseline**: `overscroll-behavior` é *Limited availability* (sem Safari) e o guia manda tratar como progressive enhancement sem feature detect; `scrollend` (2025-12-12) tem fallback de listener `scroll` com debounce.
- **Fixture mínima**: `<div class="track" style="scroll-snap-type:x mandatory">…</div>` com `::scroll-marker` em CSS.

### `native-checkbox-reimplemented`
- **Guias**: `checkbox`, `progress`, `progress-ring`, `spinner` (compartilhado com `native-progress-reimplemented`).
- **Gatilho**: `tag ∈ {DIV, SPAN}` com `aria.role ∈ {checkbox, radio, progressbar, switch, slider}` e (`aria.aria_checked` ou `attrs["aria-valuenow"]`), e nenhum `INPUT`/`PROGRESS` native na subárvore →warn. Variante custom: `INPUT` com `styles.interaction.appearance == "none"` e `styles.visual.outline-style == "none"` →warn (`custom-checkbox-focus-hidden`).
- **Evidência**: `"DIV role=checkbox reimplements native input"` / `"appearance:none checkbox suppresses focus outline"`.
- **Falso positivo / keepWhen**: não disparar `role="checkbox"` em item de `role="menu"` (`menuitemcheckbox` é legítimo); não disparar quando um `INPUT` nativo estiver no mesmo `<label>`; para a variante custom, não disparar se a focus ring vier de `box-shadow` (checar `styles.visual.box-shadow`).
- **Fallback/Baseline**: `accent-color` é *Limited availability* (Safari 26.2) e é progressive enhancement; CSS masks são *Widely available* (2023-12-07) — o check incentiva a opção `appearance: none` **com** foco preservado, cujos fallbacks o guia descreve.
- **Fixture mínima**: `<div role="checkbox" aria-checked="false">Accept</div>` e `<input type="checkbox" class="custom" style="appearance:none;outline:none">`.

### `copy-control-hidden-from-at`
- **Guias**: `copy-button`.
- **Gatilho**: botão com `attrs["command"]` começando com `--copy` (ou irmão com `aria.role == "status"`) e `aria.aria_hidden` presente →warn. Variante: `styles.layout.display == "none"` no mesmo botão →warn (remove do tab order).
- **Evidência**: `"copy button is aria-hidden; AT users cannot reach it"`.
- **Falso positivo / keepWhen**: não disparar quando `aria-hidden="true"` estiver no `<span class="icon">` decorativo; não disparar em botão desabilitado permanentemente (`aria.disabled`).
- **Fallback/Baseline**: Invoker Commands são *Newly available* (2025-12-12) com fallback de listener `click`; Clipboard API é *Widely available* mas exige contexto seguro — o check não depende de nenhum dos dois.
- **Fixture mínima**: `<button type="button" aria-hidden="true" command="--copy" commandfor="snippet">Copy</button><span role="status"></span>`.

### `root-color-scheme-hardcoded`
- **Guias**: `dark-mode-toggle`.
- **Gatilho**: `styles.accessibility.color-scheme` no nó `HTML` ∈ {`light`, `dark`, `only light`, `only dark`} e `declared` mostrar que a regra base (fora de `:has(meta)`) declara esse valor →warn. Se `declared` indicar apenas a regra `:has(> head > meta[...])`, não disparar.
- **Evidência**: `"root color-scheme base is pinned to 'dark'; system preference cannot win"`.
- **Falso positivo / keepWhen**: não disparar em site que intencionalmente fixa tema (o guia admite intenção rara, mas exige disclosure em settings); manter `warn`; não disparar quando meta `content="light dark"` e computed for `light dark`.
- **Fallback/Baseline**: `color-scheme` é *Widely available* (2022-02-03); `light-dark()` é *Newly available* (2024-05-13). A migração preserva o fallback `prefers-color-scheme` descrito no guia. Sem `declared`, o check é **F** (não D) porque o valor computado é ambíguo.
- **Fixture mínima**: `<meta name="color-scheme" content="light dark"><style>:root{color-scheme:dark}</style>` vs. `:root{color-scheme:light dark}:root:has(meta[content=dark]){color-scheme:dark}`.

### `native-dialog-reimplemented`
- **Guias**: `dialog`.
- **Gatilho**: `tag ∈ {DIV, SPAN}` com `aria.role ∈ {dialog, alertdialog}` e nenhum `DIALOG` na subárvore →warn. Variante: `DIALOG` com `attrs["aria-modal"] == "true"` →warn (redundante). Variante: `DIALOG` com `styles.box_model.overflow` do `html`/`body` = `hidden` (`--attr` não necessário) →warn separado.
- **Evidência**: `"DIV role=dialog reimplements native <dialog>"` / `"native dialog has redundant aria-modal"`.
- **Falso positivo / keepWhen**: **não disparar** em drawer sidebar (`popover="manual"` + `role="dialog"` — prescrito pelo guia `navigation-drawer`) nem em tour popover (`role="dialog"` obrigatório); não disparar em `dialog[popover]` (Rule 2) com `aria-modal` ausente.
- **Fallback/Baseline**: `closedby` é *Limited availability* (sem Safari) com fallback de `cancel` handler; Invoker Commands *Newly available* com polyfill condicional. O check incentiva `<dialog>`, que é a mesma base dos fallbacks.
- **Fixture mínima**: `<div role="dialog" aria-modal="true" aria-labelledby="t"><h2 id="t">Delete?</h2></div>` e `<dialog id="d" aria-modal="true">…</dialog>`.

### `hovercard-autofocus-on-hint`
- **Guias**: `hovercard`.
- **Gatilho**: nó com `attrs["popover"] == "hint"` ou `aria.role == "tooltip"` que contenha descendente com `attrs["autofocus"]` →fail (WCAG 3.2.1 citada no guia). Variante →warn: gatilho `A` com `aria.aria_expanded != null` e `aria.aria_describedby` apontando para o hint.
- **Evidência**: `"interest popover moves focus via autofocus (WCAG 3.2.1 On Focus)"`.
- **Falso positivo / keepWhen**: não disparar a variante em toggletip (botão com `popovertarget` cujo único trabalho é o hint), onde `aria-expanded` é correto; não disparar em `<dialog>` modal com `autofocus` (regra diferente).
- **Fallback/Baseline**: Interest Invokers e `popover="hint"` são *Limited availability* (Chrome/Edge 149 e 151; sem Safari/Firefox) com polyfills `interestfor` e `@oddbird/popover-polyfill` — o check continua válido nos polyfills porque `autofocus` é atributo DOM.
- **Fixture mínima**: `<a href="/a" interestfor="p">A</a><div id="p" popover="hint"><button autofocus>Go</button></div>`.

### `menu-composite-reimplemented`
- **Guias**: `menu`, `app-menus`.
- **Gatilho**: nó com `aria.role == "menu"` sem `attrs["focusgroup"]` e com ≥2 filhos `aria.role == "menuitem"` que tenham `attrs["tabindex"]` →warn. Sinais paralelos: `tag == "MENU"`; gatilho com `attrs["popovertarget"]` em vez de `attrs["command"]`; separador `DIV` com `aria.role == "separator"` em vez de `tag == "HR"`.
- **Evidência**: `"role=menu lacks focusgroup and uses manual roving tabindex"`.
- **Falso positivo / keepWhen**: não disparar no fallback de `focusgroup` (`'focusGroup' in HTMLElement.prototype === false` — o script aplica `role="menu"`/`menuitem"`), a menos que o check possa ver o feature detect via `__env`/`declared`; nesse caso manter `warn` com nota; não disparar em `role="menu"` dentro de `NAV` (ver #12).
- **Fallback/Baseline**: focusgroup é *Limited availability* (Chrome/Edge 150) com fallback documentado; Popover é *Newly available* (2025-01-27) com polyfill obrigatório condicionado a `Object.hasOwn(HTMLElement.prototype, "popover")`.
- **Fixture mínima**: `<button aria-haspopup="menu" aria-expanded="false">Actions</button><div role="menu"><button tabindex="0">Duplicate</button><button tabindex="-1">Rename</button></div>`.

### `site-nav-using-menu-roles`
- **Guias**: `navbar`.
- **Gatilho**: nó `NAV` ou descendente com `aria.role ∈ {menubar, menu, menuitem}` ou `attrs["aria-haspopup"]` →warn. O guia também veta `focusgroup` em navegação de site.
- **Evidência**: `"site navigation uses ARIA menu roles instead of links in the Tab order"`.
- **Falso positivo / keepWhen**: não disparar em `role="menubar"` fora de `NAV` (app-menus); não disparar quando o descendente for um menu de comandos dentro de um item de navegação (o guia permite "sibling menus" com botão separado) — nesse caso o `role="menu"` não deve estar no `NAV` raiz, então restringir o gatilho ao próprio `NAV` e seus filhos diretos.
- **Fallback/Baseline**: Popover *Newly available* com polyfill; Anchor positioning *Limited availability* (sem Firefox) com fallback `position-area` gateado por `@supports`. O check não depende de nenhum.
- **Fixture mínima**: `<nav aria-label="Primary"><div role="menubar"><button role="menuitem" aria-haspopup="menu">Services</button></div></nav>`.

### `drawer-reimplemented-with-fixed-overlay`
- **Guias**: `navigation-drawer`, `persistent-app-tours` (compartilha sinal de overlay).
- **Gatilho**: nó com `rect` cobrindo ≥95% de `__meta.width × __meta.height`, `styles.layout.position == "fixed"`, `styles.layout.z-index` numérico alto e nenhum `DIALOG`/`popover` na subárvore →warn. Secundário: `styles.box_model.overflow` em `HTML`/`BODY` = `hidden` (o guia veta travar scroll assim).
- **Evidência**: `"full-viewport fixed overlay reimplements dialog/popover top layer"`.
- **Falso positivo / keepWhen**: não disparar em cookie banner/modal de consentimento que cubra a viewport mas não seja drawer; não disparar em `dialog[open]` com `::backdrop`; exigir presença de links de navegação dentro do overlay para reforçar o sinal.
- **Fallback/Baseline**: o próprio guia descreve o fallback sem popover (`position: fixed` + z-index + irmão de backdrop) — portanto o sinal é "reinventou a roda" apenas quando **não** há justificativa de baseline; manter `warn` e citar o fallback no texto.
- **Fixture mínima**: `<div class="Drawer" style="position:fixed;inset:0;z-index:9999"><nav>…</nav></div><div class="scrim"></div>`.

### `tour-popover-unnamed`
- **Guias**: `persistent-app-tours`.
- **Gatilho**: nó com `attrs["popover"] == "manual"` e `aria.role == "dialog"` com `aria.aria_labelledby` ausente e `aria.name` vazio →warn.
- **Evidência**: `"tour popover role=dialog has no accessible name"`.
- **Falso positivo / keepWhen**: não disparar quando `aria-label` estiver presente; não disparar em toast `popover="manual"` (usa `role="region"` no contêiner, não `role="dialog"`).
- **Fallback/Baseline**: Popover *Newly available* com polyfill; Anchor positioning *Limited availability* com polyfill `@oddbird/css-anchor-positioning` (sem âncoras implícitas). O check depende só de `role`/nome, sobrevive a ambos.
- **Fixture mínima**: `<dialog id="tour" popover="manual" role="dialog"><p>New feature…</p><button>Got it</button></dialog>`.

### `toast-live-region-missing-at-load`
- **Guias**: `persistent-toast-notifications`.
- **Gatilho**: agregado `__document`/DOM: nenhum nó com `attrs["aria-live"]` ou `aria.role == "status"` presente no carregamento, enquanto existir toast `popover="manual"` (ou código de notificação detectado por `role="region"` com nome "Notifications") →warn. Variante já D: toast `popover="manual"` sem descendente `BUTTON` com `attrs["popovertargetaction"] == "hide"`.
- **Evidência**: `"no pre-populated aria-live region at load; toast announcement unreliable"`.
- **Falso positivo / keepWhen**: não disparar quando o projeto use `ariaNotify()` (o guia aceita); não disparar se o live region for criado por hidratação antes do snapshot — o agregado captura o estado real, não o código-fonte.
- **Fallback/Baseline**: `ariaNotify()` é *Newly available* (2026-09-14) e o guia manda usar live region como fallback; o check incentiva exatamente esse fallback. Popover *Newly available* com polyfill.
- **Fixture mínima**: `<div popover="manual">Saved</div>` sem `<div aria-live="assertive">` no DOM; variante com `<div role="region" aria-label="Notifications"><div popover="manual">Saved</div></div>`.

### `native-progress-reimplemented`
- **Guias**: `progress`, `progress-ring`, `spinner` (vistos em #16/#17/#19).
- **Gatilho**: `tag ∈ {DIV, SPAN, SVG}` com `aria.role == "progressbar"` e (`attrs["aria-valuenow"]` ou `attrs["aria-valuemin"]`/`["aria-valuemax"]`), sem `PROGRESS` na subárvore →warn. Variante spinner: `styles.animation.animation-iteration-count == "infinite"` e `aria.role == "progressbar"` sem valor →warn.
- **Evidência**: `"DIV role=progressbar reimplements native <progress>"`.
- **Falso positivo / keepWhen**: não disparar em `role="progressbar"` dentro de `role="menuitem"`; não disparar quando existir `PROGRESS` nativo; não disparar em anel SVG decorativo com `aria-hidden="true"`.
- **Fallback/Baseline**: `accent-color` é *Limited availability* (Safari 26.2) e progressive enhancement; `@property` é *Newly available* (2024-07-09) com fallback de "salto instantâneo"; `background-clip: border-area` é *Limited availability* com fallback `mask-image`. Migrar para `<progress>` preserva todos.
- **Fixture mínima**: `<div role="progressbar" aria-valuenow="75" aria-valuemin="0" aria-valuemax="100" style="width:50%"></div>` e `<progress value="75" max="100">`.

### `scrollspy-without-scroll-target-group`
- **Guias**: `scrollspy`.
- **Gatilho**: nó com ≥2 descendentes `A` cujos `attrs["href"]` começam com `#` e `--props scroll-target-group` ausente/diferente de `auto` no recipiente →warn. Refinar: se nenhum link tiver `attrs["aria-current"]`, o fallback `IntersectionObserver` também não está sincronizado →warn reforçado.
- **Evidência**: `"fragment-link nav has no scroll-target-group and no aria-current sync"`.
- **Falso positivo / keepWhen**: não disparar quando um scroll container diferente do `body` for `role="region"` com nome (o guia permite) e o script de fallback atualizar `aria-current`; não disparar em links âncora fora de um `NAV` de índice.
- **Fallback/Baseline**: `scroll-target-group` é *Limited availability* (Chrome/Edge 140) com fallback `IntersectionObserver` documentado — o check incentiva esse fallback.
- **Fixture mínima**: `<nav><a href="#intro">Intro</a><a href="#more">More</a></nav><section id="intro" tabindex="-1">…</section>`.

### `scroll-effect-under-reduced-motion`
- **Guias**: `carousel-slide-effects`.
- **Gatilho**: sob `__env reduced-motion: reduce`, nó com `styles.animation.animation-timeline` contendo `view(` (ou `view-timeline-name` em `styles.animation`) e `styles.animation.animation-name != "none"` dentro de scroller com `styles.interaction.scroll-snap-type` →warn.
- **Evidência**: `"scroll-driven slide effect still animates with reduced-motion: reduce"`.
- **Falso positivo / keepWhen**: não disparar se `@media (prefers-reduced-motion: no-preference)` já removeu a animação (emulação confirma `animation-name: none`); não disparar em `scroll-timeline` de barra de progresso não-decorativa sem movimento (ver #30).
- **Fallback/Baseline**: Scroll-driven animations são *Newly available* (2026-10-27); o guia **proíbe** `scroll-timeline-polyfill` e recomenda progressive enhancement — o check incentiva exatamente esse caminho.
- **Fixture mínima**: `.scroller > *{animation:animate auto linear both;animation-timeline:view(inline)}` capturada com `__env reduced-motion=reduce`.

### `multiple-scroll-initial-targets`
- **Guias**: `pull-to-reveal`.
- **Gatilho**: dentro do mesmo scroller (`styles.box_model.overflow-y ∈ {auto, scroll}`), >1 nó com `--props scroll-initial-target == "nearest"` →warn.
- **Evidência**: `"N elements declare scroll-initial-target:nearest in one scroller; only the first wins"`.
- **Falso positivo / keepWhen**: não disparar quando os alvos pertencerem a scrollers distintos (comparar `parent_id`/caminho até o scroller); não disparar se um fragmento de URL ou `scroll-start` tiver precedência — nesse caso o snapshot já mostra a posição real e o sinal é inofensivo.
- **Fallback/Baseline**: `scroll-initial-target` é *Limited availability* (Chrome/Edge 133) com fallback `scrollIntoView({behavior:'instant'})`; o check continua valendo no fallback.
- **Fixture mínima**: `<div class="scroller"><div class="a" style="scroll-initial-target:nearest">…</div><div class="b" style="scroll-initial-target:nearest">…</div></div>`.

### `hidden-scroll-control-focus-loss`
- **Guias**: `scroll-position-aware-elements`.
- **Gatilho**: com `__actions` registrando foco antes/depois de scroll: um controle com `styles.visual.visibility == "hidden"` (ou opacity 0) era `activeElement` e, após o scroll, o foco caiu para `BODY` em vez de ir para o scroller →warn. Sinal estático de apoio: `styles.layout.position ∈ {sticky, fixed}` + visibility hidden sem listener de `focusout` observável.
- **Evidência**: `"focus lost to <body> when scroll-position-aware control became hidden"`.
- **Falso positivo / keepWhen**: não disparar quando o controle usar `display: none` legítimo fora do padrão scroll-state; não disparar se o fallback "sempre visível" estiver ativo.
- **Fallback/Baseline**: Container scroll-state queries são *Limited availability* (Chrome/Edge 144) com fallback "sempre visível" ou `IntersectionObserver` — o check incentiva o fallback correto.
- **Fixture mínima**: `.scroller{container-type:scroll-state}.back-to-top{position:sticky;visibility:hidden}@container scroll-state(scrollable:top){.back-to-top{visibility:visible}}` + ação de foco no botão e `scrollTo(0,0)`.

### `scroll-region-without-name`
- **Guias**: `responsive-table`.
- **Gatilho**: nó com `styles.box_model.overflow-x`/`overflow-y` ∈ {auto, scroll}, `attrs["tabindex"] == "0"`, e (`aria.role` vazio ou ≠ `region`) e `aria.name` vazio →warn. (Restringir a nós que contêm `TABLE` para evitar ruído.)
- **Evidência**: `"scrollable table wrapper is tabbable but has no role=region / accessible name"`.
- **Falso positivo / keepWhen**: não disparar em `<main>` (papel implícito); não disparar se `aria-labelledby` apontar para a `<caption>` (presente em `aria.aria_labelledby`).
- **Fallback/Baseline**: Container queries (size) são *Widely available* (2023-02-14), sem fallback necessário; o check éorthogonal ao layout empilhado.
- **Fixture mínima**: `<div tabindex="0" class="table-wrapper"><table><caption>Employees</caption>…</table></div>` sem `role`/`aria-label`.

### `decorative-progress-exposed-to-at`
- **Guias**: `scroll-progress-indicator`.
- **Gatilho**: nó com `styles.animation.animation-timeline` contendo `scroll(` e (`aria.has_text == true` ou descendente `IMG`/`SVG`) e `aria.aria_hidden` ausente e `aria.role` ≠ `status` →warn.
- **Evidência**: `"decorative scroll progress exposes text/content to assistive technology"`.
- **Falso positivo / keepWhen**: não disparar em `<progress>` com rótulo (semântico, não decorativo); não disparar se o texto for status intencional (`role="status"`).
- **Fallback/Baseline**: Scroll-driven animations *Newly available* (2026-10-27), sem `scroll-timeline-polyfill`; o fallback JS escreve `transform` — o check continua valendo (o nó decorativo permanece).
- **Fixture mínima**: `<div id="progress">Loading 42%</div>` com `animation-timeline: scroll()` e sem `aria-hidden`.

### `hidden-scrollbar-without-affordance`
- **Guias**: `scrollability-affordance-hints`.
- **Gatilho**: scroller com `styles.interaction.scrollbar-width == "none"` (ou `::-webkit-scrollbar {display:none}` detectável via `pseudo`) e `pseudo[]` com `name ∈ {::before, ::after}` tendo `styles.visual.opacity == "0"` e (`styles.animation.animation-timeline` vazio **e** `styles.layout.container-type` sem `scroll-state`) →fail (WCAG 1.4.11 citada no guia quando a barra some).
- **Evidência**: `"scrollbar hidden and no scroll-state/scroll-driven affordance remains"`.
- **Falso positivo / keepWhen**: **não disparar quando a barra de rolagem estiver visível** — o guia diz que as dicas são suplementares; não disparar se fallback `IntersectionObserver` tiver aplicado `.more-start/.more-end` (opacity 1 no snapshot).
- **Fallback/Baseline**: scroll-state queries *Limited availability* (Chrome/Edge 144) e scroll-driven animations *Newly available* (2026-10-27); o guia exige fallback `IntersectionObserver`/`background-attachment` quando a barra está escondida — é exatamente o que o check verifica.
- **Fixture mínima**: `.scroller{overflow-x:auto;scrollbar-width:none}.scroller::before{opacity:0}` sem `animation-timeline` nem `.more-start`.

### `scroll-shrink-header-missing-scroll-padding`
- **Guias**: `shrinking-header-on-scroll`, `state-aware-sticky-headers` (mesmo sinal de `scroll-padding-top`).
- **Gatilho**: header com `styles.animation.animation-timeline` contendo `scroll(` e `styles.layout.position ∈ {fixed, sticky}`; altura atual `rect.height` do header; nó `HTML` sem `styles.interaction.scroll-padding-top` ou com valor < `rect.height` →warn.
- **Evidência**: `"animated sticky header has no scroll-padding-top >= header height (WCAG 2.4.11)"`.
- **Falso positivo / keepWhen**: não disparar se o conteúdo abaixo não tiver controles focáveis (o risco é específico de foco obscurecido); não disparar quando `scroll-padding-top` for animado na mesma timeline e `__env`/snapshot emulado mostrar valor suficiente no estado recolhido.
- **Fallback/Baseline**: Scroll-driven animations *Newly available* (2026-10-27) com fallback de `scroll` listener que escreve `height`; o `scroll-padding-top` deve existir nos dois caminhos.
- **Fixture mínima**: `<header style="position:fixed;height:200px;animation-timeline:scroll(block root)">…</header>` com `html` sem `scroll-padding-top`.

### `stateful-sticky-header-without-overflow-anchor-none`
- **Guias**: `state-aware-sticky-headers`.
- **Gatilho**: nó com `styles.layout.position == "sticky"` e `styles.layout.container-type` contendo `scroll-state`; `parent_id` cujo `styles.box_model.overflow-anchor != "none"` →warn.
- **Evidência**: `"scroll-state sticky header changes box model without overflow-anchor:none on parent"`.
- **Falso positivo / keepWhen**: não disparar em header preso ao **fundo** (o guia diz que `overflow-anchor: none` não ajuda); não disparar se a regra de estado não alterar propriedades de layout (só cor/sombra) — requirements: checar `styles.visual` vs `styles.box_model` no snapshot de estado preso.
- **Fallback/Baseline**: scroll-state queries *Limited availability* (Chrome/Edge 144) com fallback de `IntersectionObserver` duplicando estilos em `.is-stuck`; o `overflow-anchor: none` deve permanecer nos dois.
- **Fixture mínima**: `<div class="section"><div class="sticky-container"><div class="sticky-header">H</div></div><div class="content">…</div></div>` com `container-type:scroll-state` e `overflow-anchor` do pai = `auto`.

### `haspopup-without-matching-role`
- **Guias**: `viewport-aware-dropdowns`.
- **Gatilho**: nó com `attrs["aria-haspopup"]` e `attrs["popovertarget"]`/`attrs["commandfor"]`; resolver o alvo por id (`attrs["id"]`) e checar `aria.role` do alvo. Se `aria-haspopup ∈ {menu, listbox, tree, grid, dialog}` e o alvo não tiver o papel correspondente (ou nenhum papel) →warn.
- **Evidência**: `"aria-haspopup=menu points to popover without role=menu"`.
- **Falso positivo / keepWhen**: não disparar quando o alvo for `role="menu"` com `focusgroup` e navegação por setas (não verificável no snapshot — manter `warn` edocumentar); não disparar em `aria-haspopup="dialog"` com `role="dialog"`; **`popover="auto"` sozinho não basta** — o guia exige o papel explícito quando `aria-haspopup` é usado.
- **Fallback/Baseline**: Anchor positioning *Limited availability* (sem Firefox) com polyfill `@oddbird/css-anchor-positioning` (sem âncoras implícitas) ou gate `@supports (position-area: block-end)`; Popover *Newly available* com polyfill condicional. O check depende só de papéis ARIA.
- **Fixture mínima**: `<button popovertarget="p" aria-haspopup="menu">Actions</button><div id="p" popover="auto"><button>Edit</button></div>` (sem `role="menu"`).

## Resumo da fatia

- Contagens: D=27 F=6 N=1 E=1 (total 35 = 22 `ui-components` + 13 `ui-atoms`). Todos os 35 ids da fatia foram cobertos — nenhuma omissão.
- Top 5 oportunidades priorizadas (impacto × custo):
  1. `native-progress-reimplemented` (progress / progress-ring / spinner) — três guias, sinal trivial (`tag` + `aria.role`), terreno mais fértil de "reinventou a roda"; custo zero.
  2. `native-dialog-reimplemented` + `drawer-reimplemented-with-fixed-overlay` (dialog / navigation-drawer / app-tours) — overlays são a classe mais comum de erro de top-layer; custo zero com `attrs` já existente.
  3. `menu-composite-reimplemented` + `site-nav-using-menu-roles` + `tabs-reimplemented-with-roving-tabindex` + `toolbar-roving-tabindex-broken` — quatro guias convergem no mesmo erro (widget composto sem `focusgroup`/com roving manual quebrado); reutilizam `aria` + `attrs`.
  4. `color-scheme-without-background` + `masked-icon-missing-forced-color-adjust` + `hidden-scrollbar-without-affordance` — três checks atômicos de alta severidade em `forced-colors`/scroll-state, todos lidos diretamente de `styles`/`pseudo`.
  5. `root-color-scheme-hardcoded` (F, `declared`) — a oportunidade de maior valor **novo** para o sniff: é o caso onde o valor computado é ambíguo e só a facet CSSOM proposta resolve o "default fixado vs. override por `:has(meta)`".
- Guias que exigem novas facets, agrupados por facet:
  - `declared` (CSSOM/regras casadas): `dark-mode-toggle` (diferenciar base `:root` de override `:has(meta)`).
  - `__document` / agregado de DOM: `persistent-toast-notifications` (live region pré-povoada ausente no load).
  - `__env` (emulação de preferências): `carousel-slide-effects` (`reduced-motion: reduce`); `spinner` (variante de giro rápido sob reduced motion); `icons` (validação final em `forced-colors: active`).
  - `__actions` (estado pós-interação): `scroll-position-aware-elements` (perda de foco ao esconder controle); `position-aware-tooltips` (tooltip que obscurece controle focado — hoje `N`).
  - `--props` (captura de propriedades fora dos 8 grupos, extensão de config já suportada por `properties_for`): `scrollspy` (`scroll-target-group`), `pull-to-reveal` (`scroll-initial-target`), `viewport-aware-dropdowns`/`position-aware-tooltips` (`position-area`, `position-try-fallbacks`), `hovercard`/`toolbar` (`interest-delay`).
  - `attrs` opt-in (facet existente, listado só para custo de config): `aria-current`, `tabindex`, `aria-haspopup`, `popover`, `interestfor`, `focusgroup`, `command`, `aria-valuenow`, `hidden`, `inert`, `aria-live`, `autofocus`, `aria-selected`, `id`.
- Nota metodológica: nenhum check proposto usa LLM, heurística temporal ou leitura de intenção do autor. Onde o sinal é ambíguo por construção (`dark-mode-toggle`, `position-aware-tooltips`), a classificação foi `F`/`N` em vez de inventar um sinal fraco. `fail` foi reservado para critérios normativos explícitos nos guias (WCAG 3.2.1 em `hovercard`; WCAG 1.4.11 em `scrollability-affordance-hints`).
