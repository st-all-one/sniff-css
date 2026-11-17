# 4. Catálogo consolidado de checks de oportunidade

> **Fonte:** tabelas e detalhes dos 7 relatórios em [`findings/`](findings/).
> Aqui está o **índice consolidado**; gatilho exato, `evidence`, `keepWhen`,
> fallback/Baseline e fixture de cada check estão no relatório de origem —
> não duplicar os detalhes abaixo.
>
> Contrato de saída: [`08-contrato-v2.md`](08-contrato-v2.md).

## 4.1 Consolidação: o que mudou quando juntamos as fatias

As sete fatias produziram **140 check_ids únicos** (110 nas seis primeiras +
30 em `css`/`visual-design`) antes da consolidação. Alguns são o mesmo check
descrito duas vezes — sinal de que o gatilho converge de guias diferentes.

| Checks duplicados / convergentes | Guias envolvidos | Decisão |
|---|---|---|
| `duplicate-view-transition-name` | `group-element-transitions`, `same-document-transitions` (+ sinal irmão em cross-doc) | **1 check**, guias múltiplos no `guide` |
| `snap-container-not-keyboard-focusable` | `carousel-snap-highlights`, `scroll-snap-realtime-feedback` | **1 check** |
| `scroll-initial-target-multiple` ≡ `multiple-scroll-initial-targets` | `scroll-target-on-load`, `pull-to-reveal` | **unificar** com o id `scroll-initial-target-multiple` |
| `reduced-motion-guard-missing` | `parallax-scroll-effects`, `scroll-entry-exit-effects`, `scrollytelling` | **1 check**, 3 guias |
| `native-progress-reimplemented` | `progress`, `progress-ring`, `spinner` | **1 check**, 3 guias |
| `autofill-purpose-tokens` | `autofill-address-form`, `autofill-sign-in-form`, `autofill-sign-up-form`, `forms` | **1 check**, 4 guias |
| `native-checkbox-reimplemented` / variante `custom-checkbox-focus-hidden` | `checkbox` (+ sobreposição com `native-progress-*`) | manter separados: um foca semântica, outro foco suprimido |
| `cross-doc-transition-missing-render-block` × `cross-doc-transition-not-opted-in` | `consistent-cross-document-transitions` × `cross-document-transitions` | manter separados: at-rule vs. head blocking |
| `root-color-scheme-hardcoded` × `color-scheme-without-background` | `dark-mode-toggle` × `component-specific-light-dark-theme` | manter separados: origem da regra vs. escopo do elemento |
| `document-metadata-basics` / `form-semantics-basics` | `html`, `forms` | avaliar fusão em F1 para reduzir ruído |
| `text-not-selectable` (check **existente**) | falso positivo legítimo no handle de `drag` | **suprimir** no handle; não criar check novo |

**Lição da consolidação:** sem a unificação, o `check` emitiria a mesma
oportunidade duas vezes para o mesmo nó — exatamente o tipo de ruído que
destrói confiança. O registro de checks (`src/modern/mod.rs`) deve ser a
**única** fonte de ids, com `guias: &'static [&'static str]`.

## 4.2 Legenda

- **Facets:** `styles.*` = categorias já capturadas; `attrs` = facet existente
  (default no v0.6); `pseudo`, `ax`, `aria`, `rect`, `path`, `css_variables` =
  existentes; `__document`, `__env`, `declared`, `matrix`, `__actions` =
  extensões (ver `03-facets.md`); `+prop` = nova propriedade no catálogo.
- **Sev:** `warn` (padrão) ou `fail` (só com WCAG/WAI-ARIA citado no guia).
- **Prio:** `P0` = F0/F1, custo zero; `P1` = F2, alto valor; `P2` = depende de
  facet nova ou matriz; `P3` = especialista/baixa frequência.

## 4.3 Família A — estrutura, semântica e metadata (F1)

| check_id | Guias | Facets | Sev | Prio |
|---|---|---|---|---|
| `document-metadata-basics` | `html`, `respect-os-text-scale` | `__document` | warn (fail se `lang` ausente — WCAG 3.1.1) | P1 |
| `form-semantics-basics` | `forms`, `html` | `attrs`, `aria`, `tag` | warn (fail em `tabindex>0`/`aria-hidden` em focusável) | P0 |
| `breadcrumb-current-page-missing` | `breadcrumbs` | `aria`, `attrs` | warn | P0 |
| `generated-content-alt` | `accessible-generated-content` | `pseudo`, `aria` | warn | P0 |
| `live-region-announcer` | `accessible-error-announcement` | `attrs`, `aria` | warn | P0 |
| `autofocus-without-describedby` | `passkey-authentication` | `attrs`, `aria` | warn | P0 |
| `card-interactive-nested-in-link` | `card` | árvore (`tag`) | warn | P0 |
| `site-nav-using-menu-roles` | `navbar` | `tag`, `aria`, `attrs` | warn | P0 |
| `scroll-region-without-name` | `responsive-table` | `styles.box_model`, `aria`, `attrs` | warn | P0 |
| `web-install-manifest-required` | `install-web-app` | `__document` (link rel=manifest) | warn | P1 |
| `webmcp-form-tool-metadata` | `agentic-forms` | `attrs`, `aria`, `tag` | warn | P1 |

## 4.4 Família B — plataforma nativa em vez de reimplementação (F2)

| check_id | Guias | Facets | Sev | Prio |
|---|---|---|---|---|
| `native-dialog-reimplemented` | `dialog` | `tag`, `aria`, `attrs` | warn | P0 |
| `native-checkbox-reimplemented` | `checkbox` | `aria`, `tag`, `styles.interaction/visual`, `attrs` | warn | P0 |
| `native-progress-reimplemented` | `progress`, `progress-ring`, `spinner` | `tag`, `aria`, `styles.animation/transform`, `attrs` | warn | P0 |
| `native-range-control` | `slider`, `switch` | `aria`, `tag`, `attrs` | warn | P0 |
| `switch-semantics` | `switch` | `aria`, `attrs` | warn | P0 |
| `accordion-custom-instead-of-details` | `accordion` | `aria`, árvore | warn | P0 |
| `menu-composite-reimplemented` | `menu`, `app-menus` | `aria`, `attrs`, `tag` | warn | P0 |
| `tabs-reimplemented-with-roving-tabindex` | `tabs` | `aria`, `attrs` | warn | P0 |
| `toolbar-roving-tabindex-broken` | `toolbar` | `aria`, `attrs` | warn | P0 |
| `drawer-reimplemented-with-fixed-overlay` | `navigation-drawer`, `persistent-app-tours` | `rect`, `styles.layout`, `__meta`, `pseudo` | warn | P0 |
| `os-text-scale-optin` | `respect-os-text-scale` | `__document` (meta text-scale) | warn | P1 |
| `root-color-scheme-hardcoded` | `dark-mode-toggle` | `styles.accessibility`, `declared` | warn | P2 |
| `color-scheme-without-background` | `component-specific-light-dark-theme` | `styles.accessibility/visual`, `tag` | warn | P0 |
| `masked-icon-missing-forced-color-adjust` | `icons` | `pseudo`, `styles.visual`, `__env` | warn | P2 |
| `coarse-pointer-target-size` | `form-factor-adaptation` | `__env`, `rect` | warn (WCAG 2.5.5 é AAA) | P2 |

## 4.5 Família C — formulários e autofill (F2)

| check_id | Guias | Facets | Sev | Prio |
|---|---|---|---|---|
| `autofill-purpose-tokens` | `autofill-address-form`, `autofill-sign-in-form`, `autofill-sign-up-form`, `forms` | `attrs`, `tag` | warn (fail em `type=email` sem autocomplete — 1.3.5) | P0 |
| `credential-autocomplete-tokens` | `autofill-sign-in-form`, `autofill-sign-up-form` | `attrs` | warn | P0 |
| `payment-field-single-input` | `autofill-payment-form` | `attrs`, `tag` | warn | P0 |
| `hint-above-input` | `autofill-payment-form`, `required-field-feedback`, `sms-otp-form`, `validate-input-after-interaction` | `aria`, `attrs` | warn | P0 |
| `user-invalid-aria-sync` | `required-field-feedback`, `select-menu-interaction`, `validate-input-after-interaction`, `accessible-error-announcement` | `styles.accessibility` (`:user-invalid` em pseudo?), `attrs`, `aria` | warn | P0/P2 |
| `otp-single-input` | `sms-otp-form` | `attrs`, `tag` | warn | P0 |
| `select-base-semantics` | `branded-select-styling`, `rich-media-picker` | `styles.interaction`, `attrs`, `aria` | warn | P0 |
| `select-placeholder-option` | `select-menu-interaction` | `aria`, `attrs` | warn | P0 |
| `select-option-noncolor-state` | `rich-media-picker` | `pseudo`, `styles.visual` | warn | P1 |
| `select-picker-animation-guard` | `animated-select-picker` | `styles.animation`, `declared` | warn | P2 |
| `select-picker-open-layout` | `custom-select-picker-layouts` | `rect`, `styles.layout`, `declared` | warn | P2 |
| `autofill-noncolor-indicator` | `autofill-highlight-inputs` | `pseudo`, `styles.visual`, `declared` | warn | P2 |
| `accent-color-contrast` | `brand-consistent-forms` | `styles.visual`, `effective_background` | warn | P2 |
| `field-sizing-constraints` | `form-fields-automatically-fit-contents` | `+prop field-sizing`, `attrs` | warn | P2 |
| `interaction-state-feedback` | `natural-interactions` | `styles.interaction`, `__actions` | warn | P2 |
| `persistent-bar-budget` | `content-first-noise-reduction` | `rect`, `styles.layout` | warn | P1 |

## 4.6 Família D — interações, transições e estados ARIA (F2)

Checks de custo zero (facets existentes) — a maioria veio de `ui-behaviors`:

| check_id | Guias | Facets | Sev | Prio |
|---|---|---|---|---|
| `duplicate-view-transition-name` | `group-element-transitions`, `same-document-transitions` | `styles.animation` | warn | P0 |
| `display-transition-without-allow-discrete` | `animate-element-entry-exit` | `styles.animation`, `attrs` | warn | P0 |
| `top-layer-transition-missing-overlay` | `animate-to-from-top-layer` | `styles.animation`, `attrs`, `pseudo` | warn | P0 |
| `active-tab-indicator-missing-aria-state` | `anchor-positioning-tab-underline` | `pseudo`, `attrs`, `aria` | warn | P0 |
| `custom-command-invalid-name-or-state` | `custom-button-actions` | `attrs`, `aria` | warn | P0 |
| `invoker-conflicting-attributes-or-missing-type` | `declarative-dialog-popover-control` | `attrs`, `aria`, `path` | warn | P0 |
| `drag-handle-affordance-misapplied` | `drag` | `styles.interaction`, `aria`, `attrs` | warn | P0 |
| `reorder-faked-with-css-order` | `drag-and-drop` | `styles.layout`, `rect`, `path` | warn | P0 |
| `stagger-delay-collapsed` | `dynamic-sibling-animations` | `styles.animation`, `css_variables` | warn | P0 |
| `sort-order-not-reflected-in-dom` | `dynamic-sort` | `styles.layout/animation`, `aria` | warn | P0 |
| `reveal-overlay-blocks-pointer-events` | `interactive-content-reveal` | `styles.visual/interaction`, `css_variables` | warn | P0 |
| `interestfor-dangling-target` | `interest-triggered-action-previews` | `attrs` (id + interestfor) | warn | P0 |
| `interest-tooltip-wrong-aria-state` | `interest-triggered-tooltips` | `attrs`, `aria` | warn | P0 |
| `dialog-light-dismiss-without-name` | `light-dismiss-a-dialog` | `attrs`, `aria`, `pseudo` | warn | P0 |
| `motion-clipped-focus-indicator` | `motion` | `aria`, `styles.visual` | warn | P0 |
| `linear-easing-missing-duration` | `physics-based-easing` | `styles.animation` | warn | P0 |
| `modal-dialog-scroll-chaining` | `prevent-page-scroll-under-modal-dialog` | `attrs`, `pseudo ::backdrop`, `styles.interaction` | warn | P0 |
| `mandatory-snap-unreachable-content` | `scroll-snap-state-sync` | `styles.interaction`, `rect` | warn | P0 |
| `hidden-until-found-display-none` | `search-hidden-content` | `attrs`, `styles.box_model/visual` | warn | P0 |
| `sortable-table-aria-sort-misuse` | `sortable-table` | `attrs`, `tag`, `path` | warn | P0 |
| `roving-tabindex-without-composite-role` | `spatial-navigation` | `attrs`, `aria`, `path` | warn | P0 |
| `swipe-gesture-no-button-alternative` | `swipe-to-remove` | `styles.interaction`, `tag`, `aria` | warn (WCAG 2.5.1) | P0 |
| `swipe-reveal-overscroll-trapped` | `swipe-to-reveal` | `styles.interaction`, `path` | warn | P0 |
| `hovercard-autofocus-on-hint` | `hovercard` | `attrs`, `aria` | **fail** (WCAG 3.2.1) | P0 |
| `hidden-scrollbar-without-affordance` | `scrollability-affordance-hints` | `styles.interaction`, `pseudo`, `styles.animation` | **fail** (WCAG 1.4.11) | P0 |
| `haspopup-without-matching-role` | `viewport-aware-dropdowns` | `attrs`, `aria` | warn | P0 |
| `snap-stack-multiple-active-views` | `stack-drill-down` | `styles.interaction`, `attrs` (inert) | warn | P0 |
| `decorative-progress-exposed-to-at` | `scroll-progress-indicator` | `styles.animation`, `aria` | warn | P0 |
| `scroll-shrink-header-missing-scroll-padding` | `shrinking-header-on-scroll` | `styles.animation/layout/interaction`, `rect`, `__meta` | warn (WCAG 2.4.11) | P0 |
| `stateful-sticky-header-without-overflow-anchor-none` | `state-aware-sticky-headers` | `styles.layout/box_model`, `parent_id` | warn | P0 |
| `copy-control-hidden-from-at` | `copy-button` | `aria`, `attrs` | warn | P0 |
| `tour-popover-unnamed` | `persistent-app-tours` | `attrs`, `aria` | warn | P0 |
| `scrollspy-without-scroll-target-group` | `scrollspy` | `+prop scroll-target-group`, `attrs` | warn | P2 |

## 4.7 Família E — preferências e estado emulado (F3)

| check_id | Guias | Facets | Sev | Prio |
|---|---|---|---|---|
| `reduced-motion-guard-missing` | `parallax-scroll-effects`, `scroll-entry-exit-effects`, `scrollytelling` | `styles.animation`, `declared` **ou** `__env` + matriz | warn | P2 |
| `scroll-effect-under-reduced-motion` | `carousel-slide-effects` | `styles.animation`, `__env` (reduce) | warn | P2 |
| `dialog-dismiss-unverified` | `platform-controls-dismiss-dialog` | `attrs`, `__actions` com action `key` | warn | P2 |
| `hidden-scroll-control-focus-loss` | `scroll-position-aware-elements` | `__actions` (foco), `styles.visual/layout` | warn | P2 |
| `toast-live-region-missing-at-load` | `persistent-toast-notifications` | `__document`/agregado DOM, `attrs` | warn | P2 |
| `cross-doc-transition-missing-render-block` | `consistent-cross-document-transitions` | `__document`, `declared` | warn | P2 |
| `cross-doc-transition-not-opted-in` | `cross-document-transitions` | `declared` | warn | P2 |
| `directional-transition-missing-type-rule` | `directional-navigation-transitions` | `declared` | warn | P2 |
| `scoped-vt-isolation-missing` | `scoped-component-transitions` | `+prop view-transition-scope`, `declared` | warn | P2 |
| `scroll-initial-target-multiple` | `scroll-target-on-load`, `pull-to-reveal` | `+prop scroll-initial-target`, `path` | warn | P2 |

## 4.8 Família F — performance lab-observável (F4)

| check_id | Guias | Facets | Sev | Prio |
|---|---|---|---|---|
| `content-visibility-without-intrinsic-size` | `defer-rendering-heavy-content`, `efficient-background-processing`, `interactions-in-complex-layouts`, `performance` | `styles.layout` | warn | P1 |
| `content-visibility-above-the-fold` | `defer-rendering-heavy-content` | `styles.layout`, `rect`, `__meta` | warn | P1 |
| `cv-hidden-view-not-inert` | `faster-spa-view-transitions` | `styles.layout`, `attrs` (inert) | warn | P1 |
| `image-without-dimensions` | `performance` | `attrs`, `rect` | warn | P0 |
| `srcset-without-sizes` | `performance` | `attrs` | warn | P0 |
| `lazy-media-above-the-fold` | `lazy-load-video-and-audio`, `optimize-image-priority`, `performance` | `attrs`, `rect`, `__meta` | warn | P0 |
| `lazy-media-without-dimensions-or-controls` | `lazy-load-video-and-audio` | `attrs`, `tag` | warn | P0 |
| `fetchpriority-high-with-lazy` | `optimize-image-priority`, `performance` | `attrs` | warn | P0 |
| `excessive-will-change` | `performance` | `+prop will-change` | warn | P2 |
| `image-set-without-legacy-fallback` | `deliver-optimized-decorative-images`, `resolution-optimized-pseudo-elements` | `declared` (CSSOM `url()`) | warn | P2 |
| `render-blocking-script-misuse` | `flicker-free-client-side-ab-testing` | `__document` (scripts) | warn | P1 |
| `preload-priority-misuse` | `optimize-preload-priority` | `__document` (link rel=preload) | warn | P1 |
| `script-priority-misuse` | `optimize-script-priority` | `__document` (fetchpriority em script) | warn | P1 |
| `speculation-rules-misuse` | `improve-next-page-load-performance` | `__document` | warn | P1 |
| `streaming-live-region-scope` | `out-of-order-html-streaming` | `attrs`, `aria`, `tag` | warn | P1 |

## 4.9 Família G — Web Components (F2, depende walker de shadow root)

| check_id | Guias | Facets | Sev | Prio |
|---|---|---|---|---|
| `shadow-root-open-mode` | `shadow-dom`, `web-components`, `prerendering-custom-elements` | walker `el.shadowRoot` | warn | P2 |
| `elementinternals-ax-state` | `accessible-web-components`, `custom-elements`, `form-associated-custom-elements` | walker + `aria`/`ax` internos | warn | P2 |
| `shadow-internals-semantics` | `accessible-web-components`, `shadow-dom`, `styling-web-components`, `web-components` | walker + `aria` | warn (fail se controle ficar sem nome — 4.1.2) | P2 |
| `custom-element-tag-name` | `custom-elements`, `web-components` | `tag` (hífen) | warn | P1 |
| `face-form-integration` | `form-associated-custom-elements` | walker + `attrs`, `aria` | warn | P2 |
| `aria-cross-shadow-refs` | `accessible-web-components` | walker + `attrs` | warn | P2 |
| `declarative-shadow-prerender` | `prerendering-custom-elements` | walker + `tag` | warn | P2 |
| `component-theming-boundary` | `styling-web-components` | walker + `css_variables` | warn | P2 |

**Nota:** `mode:'closed'` é **N** e não entra no catálogo — o walker não tem
acesso ao shadow root por definição.

## 4.10 Família H — segurança e manifesto (limites do observável)

| check_id | Guias | Facets | Sev | Prio |
|---|---|---|---|---|
| `external-subresource-integrity` | `secure-external-subresources` | `__document` (scripts/links cross-origin sem `integrity`) | warn | P2 |
| `csp-meta-mandatory-directives` | `security`, `trusted-types` | `__document` (meta CSP) | warn | P2 |
| `lna-insecure-context-delegation` | `local-network-access` | `__env.isSecureContext`, `__document` | warn | P3 |
| `email-verification-token-field` | `verify-email-ownership` | `attrs` (input/componente de token) | warn | P3 |
| `manifest-migration-metadata` | `migrate-web-app-origin` | `__manifest` (nova passada/fetch) | warn | P3 |

**Fora do catálogo (N):** WebCrypto, passkeys, WebAuthn, Temporal/Intl,
Built-in AI, WebMCP imperativo, WASM, privacy e `sanitize-untrusted-html`
(sanitização não deixa marca no DOM). Ver
[`05-inventario-guias.md`](05-inventario-guias.md).

## 4.11 Matriz de implementação (quem destrava o quê)

```
F0 (facetas + contrato)
  ├── attrs default ────────► P0: ~55 checks (Famílias A–D, F e I)
  ├── __document ───────────► P1: ~12 checks (A, B, F, H)
  ├── __env ────────────────► P2: ~8 checks (B, E, I)
  ├── walker shadowRoot ────► P2: 8 checks (G)
  ├── declared CSSOM ───────► P2: ~24 checks (B, C, E, F, H, I)
  └── matriz/action key ────► P2: ~4 checks (E)
```

**Ordem recomendada:** P0 → P1 → P2 → P3. Os ~55 checks P0 cobrem mais da
metade do valor com **nenhuma facet nova** — são o argumento mais forte para
o upgrade v0.6.

## 4.12 Checklist obrigatório por check (definition of done)

- [ ] id único no registro `src/modern/mod.rs`, com `guias: &[...]`;
- [ ] gatilho expresso só com campos do snapshot (sem intenção do autor);
- [ ] `evidence.text` determinístico + `metric`/`observed`/`threshold`/`source`
      quando aplicável;
- [ ] `guide` aponta para id existente no corpus;
- [ ] severidade justificada (`fail` exige WCAG/WAI-ARIA citado);
- [ ] `keepWhen` documentado no findings e **testado** como caso de negação;
- [ ] fixture positiva e negativa (pelo menos uma Chrome-gated se usar `attrs`);
- [ ] fallback/Baseline do guia preservado — o check nunca recomenda remover
      progressive enhancement;
- [ ] nenhuma duplicata com check existente ou outro check novo.

## 4.13 Família I — CSS/visual (consolidação final)

Última fatia a chegar; mantida como seção própria para não reordenar as
famílias já referenciadas. Detalhes completos em
[`findings/css-visual.md`](findings/css-visual.md).

**D — custo zero (facets existentes, incluindo `--props`):**

| check_id | Guias | Facets | Sev | Prio |
|---|---|---|---|---|
| `visual-order-breaks-tab-order` | `css-layout` | `styles.layout`, `rect`, `path` | warn | P0 |
| `transform-stacking-context-shift-on-hover` | `individual-transform-properties` | `styles.transform`, `pseudo` | warn | P0 |
| `overflow-clip-margin-ignored` | `overflow-clipping-control` | `styles.box_model`, `styles.layout` | warn | P0 |
| `media-aspect-ratio-missing` | `responsive-design` | `styles.custom`, `rect`, `__meta` | warn | P0 |
| `size-containment-without-definite-block-size` | `size-aware-styling` | `styles.layout`, `rect` | warn | P0 |
| `scrollbar-width-thin-or-none` | `browser-ui-theming` | `styles.interaction`, `tag` | warn | P0 |
| `decoration-faked-with-border-or-gradient` | `custom-text-decorations` | `styles.visual`, `pseudo` | warn | P0 |
| `scrollbar-thumb-track-contrast` | `customize-scrollbar-color-and-thickness` | `styles.visual`, `__env` | **fail** (WCAG 1.4.11) | P0 |
| `text-fit-on-intrinsic-width-container` | `equal-width-text-lines` | `styles.custom(text-fit)`, `styles.box_model` | warn | P0 |
| `drawable-without-geometry` | `expose-canvas-content-to-browser-features` | `ax`, `rect` | warn | P0 |
| `text-wrap-balance-on-long-block` | `improve-text-layout-and-legibility` | `styles.typography`, `rect` | warn | P0 |
| `text-box-on-long-form-text` | `precise-text-alignment` | `styles.custom(text-box)`, `rect` | warn | P0 |
| `mask-on-text-containing-element` | `shaped-cutouts` | `styles.visual`, `aria.has_text` | warn | P0 |
| `justify-without-hyphens` | `typography` | `styles.typography`, `aria.lang`, `rect` | warn | P0 |
| `blend-mode-without-isolation` | `visual-effects` | `styles.visual`, `styles.layout` | warn | P0 |

**F — dependem de `declared` / `__env` / at-rules:**

| check_id | Guias | Facets | Sev | Prio |
|---|---|---|---|---|
| `intrinsic-size-transition-no-reduced-motion` | `animate-to-intrinsic-sizes` | `styles.animation`, `declared` | warn | P2 |
| `calc-size-missing-fallback` | `calculate-with-intrinsic-sizes` | `styles.custom`, `declared` | warn | P2 |
| `has-critical-state-without-fallback` | `child-state-based-styling`, `content-based-styling` | `declared.at_rules`, `styles.layout` | warn | P2 |
| `inline-nontrivial-style` | `css` | `attrs` (style), `declared` | warn | P2 |
| `style-query-without-selector-fallback` | `design-token-reactivity`, `usage-aware-component-variations` | `declared.at_rules` | warn | P2 |
| `sibling-functions-without-feature-detect` | `dynamic-sibling-styling` | `styles.animation`, `declared` | warn | P2 |
| `fluid-type-clamp-exceeds-2_5x` | `fluid-scaling` | `styles.typography`, `rect`, `__env` | warn | P2 |
| `color-scheme-mismatch` | `selector-atrule-combinations` | `styles.accessibility`, `declared`, `__env` | warn | P2 |
| `has-user-invalid-without-aria-invalid` | `style-parent-with-has` | `declared`, `attrs`, `aria` | warn | P2 |
| `scrollbar-contrast-ignores-prefers-contrast` | `adapt-scrollbar-to-contrast-preferences` | `__env`, `styles.visual` | warn | P2 |
| `forced-colors-unhandled` | `color` | `__env`, `styles.visual` | warn | P2 |
| `mask-no-graceful-fallback` | `complex-shapes`, `soft-edge-content-fade`, `visually-texture-content` | `styles.visual`, `declared` | warn | P2 |
| `dark-mode-ignored` | `dark-mode` | `__env`, `styles.visual`, `declared` | warn | P2 |
| `drawable-3d-geometry-unverifiable` | `interactive-content-in-3d-scenes` | `ax`, `rect`, `tag` | warn | P3 |
| `font-size-adjust-missing-fallback` | `visually-stable-font-fallbacks`, `visually-stable-mixed-fonts` | `styles.typography`, `declared` | warn | P2 |

**Extensão descoberta nesta fatia:** o `declared` descrito em
`03-facets.md` coleta `CSSStyleRule`, mas 4 guias precisam também dos
**prelúdios** de `@container style(...)`/`@supports(...)`. Especificar
`declared.at_rules: [{conditionText, rules}]` na implementação de F0.
