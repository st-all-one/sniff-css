# 5. Inventário dos 210 guias — o que o sniff pode ver

> Inventário completo e auditável: um guia por linha nos
> [`findings/`](findings/). Este arquivo é o **consolidador de contagens e
> fronteiras**, não uma segunda cópia das tabelas.

## 5.1 Totais

| Classe | Guias | % | Significado |
|---|---:|---:|---|
| **D** — detectável hoje | **96** | 45,7% | Basta o snapshot atual (incluindo `attrs`/`pseudo`/`--props`, já existentes). |
| **F** — detectável com extensão factível | **63** | 30,0% | Exige `declared`, `__document`, `__env`, matriz, walker de shadow root, nova propriedade ou nova action em `__actions`. |
| **N** — não observável | **48** | 22,9% | Exige intenção do autor, origem do código, header HTTP, credencial, telemetria de campo ou estado transitório. |
| **E** — já coberto | **3** | 1,4% | Check existente cobre a violação central (`ellipsis-without-clip`, `contrast-aa`/`-aaa`, facet `accessibility`). |
| **Total** | **210** | 100% | 7 fatias, 7 relatórios, nenhum guia omitido. |

**Leitura:** quase **46% do corpus já é observável sem mudar nada** na captura,
e 30% adicional entra com extensões bem delimitadas. Os 23% restantes são a
fronteira legítima do sniff — classificar algo como N é decisão de qualidade,
não de preguiça.

## 5.2 Por fatia

| Fatia | Guias | D | F | N | E | Relatório |
|---|---:|---:|---:|---:|---:|---|
| `ui-behaviors` (1–20) | 20 | 16 | 3 | 1 | 0 | [`ui-behaviors-a.md`](findings/ui-behaviors-a.md) |
| `ui-behaviors` (21–40) | 20 | 11 | 6 | 3 | 0 | [`ui-behaviors-b.md`](findings/ui-behaviors-b.md) |
| `ui-components` + `ui-atoms` | 35 | 27 | 6 | 1 | 1 | [`ui-components-atoms.md`](findings/ui-components-atoms.md) |
| `performance` | 26 | 8 | 6 | 12 | 0 | [`performance.md`](findings/performance.md) |
| `forms` + `html` + `accessibility` + `user-experience` | 34 | 16 | 16 | 1 | 1 | [`forms-html-a11y-ux.md`](findings/forms-html-a11y-ux.md) |
| `css` + `visual-design` | 40 | 15 | 20 | 4 | 1 | [`css-visual.md`](findings/css-visual.md) |
| `security` + `js` + `built-in-ai` + `webmcp` + `pwa` + `privacy` + `wasm` | 35 | 3 | 6 | 26 | 0 | [`security-etc.md`](findings/security-etc.md) |
| **Total** | **210** | **96** | **63** | **48** | **3** | |

## 5.3 Onde o sniff já é forte

- **`ui-components`/`ui-atoms` (27 D de 35)** — o terreno mais fértil: quase
  todo componente reimplementado deixa rastro em `tag`/`aria`/`styles`/
  `attrs`. `native-dialog-reimplemented`, `native-progress-reimplemented`,
  `menu-composite-reimplemented`, `accordion-custom-instead-of-details` são
  detectáveis **hoje**.
- **`ui-behaviors` (27 D de 40)** — transições, snap, ARIA de indicadores,
  gestos sem alternativa: todos com sinal em `styles.animation`,
  `styles.interaction`, `aria` e `pseudo`.
- **`css`/`visual-design` (15 D de 40)** — mesmo sendo majoritariamente
  authoring-time, há oportunidades runtime exclusivas:
  `drawable-without-geometry` (elemento `drawable` focável na AX com rect
  0×0) é algo que o scan de fonte **não consegue** ver.

## 5.4 Onde a captura precisa crescer

A extensão com maior densidade é **`declared` (CSSOM com origem)** — citada
em ~15 guias da fatia CSS/visual e em várias das outras. Ela resolve a
ambiguidade computed vs. declarado que bloqueia checks como
`root-color-scheme-hardcoded`, `reduced-motion-guard-missing` e
`has-critical-state-without-fallback`.

Segunda onda:

| Extensão | Guias que destrava (exemplos) |
|---|---|
| `__document` (1 linha) | `document-metadata-basics`, `render-blocking-script-misuse`, `os-text-scale-optin`, `lcp-preload`, `web-install-manifest-required` |
| `__env` (1 linha) | `dark-mode-ignored`, `coarse-pointer-target-size`, `scroll-effect-under-reduced-motion`, `forced-colors-unhandled` |
| Walker `el.shadowRoot` (open) | 5 guias de Web Components — sem round-trip extra |
| Novo action `key` em `__actions` | `dialog-dismiss-unverified` |
| Novas propriedades no catálogo | `field-sizing`, `interpolate-size`, `scroll-initial-target`, `scroll-target-group`, `view-transition-scope/group/class`, `will-change`, `effective_background` |
| `declared.at_rules` (prelúdios `@container style`/`@supports`) | 4 guias de CSS que o `declared` de `CSSStyleRule` sozinho não cobre |

**Correção importante sobre custo:** `--props NOME` **já existe** e lê
qualquer propriedade computada via `getPropertyValue`
([`capture.rs`, `properties_for`](../crates/sniff-core/src/properties.rs)).
Portanto `container-type`, `text-fit`, `text-box` e `interpolate-size` são **D**,
não F — basta pedir a propriedade, não alterar a engine. O plano só precisa
adicionar ao catálogo as propriedades que devem vir **no default** (v0.6).

## 5.5 A fronteira do observável (os 48 N)

Concentração: `security` + `js` + `built-in-ai` + `webmcp` + `pwa` +
`privacy` + `wasm` respondem por **26 dos 48 N**, e `performance` por outros
**12**. Isso é coerente:

- **Runtime/credencial:** passkeys, WebCrypto, assinaturas, WebAuthn,
  verificação de e-mail, `Trusted Types`, sanitização — a operação acontece,
  mas o DOM final é idêntico ao de uma implementação incorreta.
- **Header/servidor:** CSP, `Cross-Origin-*`, `Permissions-Policy`,
  `local-network-access` — não estão no DOM; `meta CSP` é exceção que vira F.
- **Campo/telemetria:** LCP/INP/CLS, LoAF, Event Timing, `fetchLater`,
  `scheduler.*`, `fetch(priority)` — exigem `PerformanceObserver` ou dados de
  campo; introduzi-los quebraria o contrato offline e determinístico do
  `sniff check`.
- **Intenção do autor:** `moveBefore()`, `startViewTransition({types})`,
  handlers `keydown`/`isComposing`, `CSS.highlights`, técnicas de autoria como
  `reduce-style-repetition` e `@supports` puro.
- **Pixels fora do DOM:** WebGL, canvas, exportação de mídia — o snapshot
  descreve nós e estilos, não o framebuffer.

Casos limítrofes que **valem a pena acompanhar** (hoje N, com caminho F
documentado): `highlight-text-ranges` (novo passada lendo `CSS.highlights`),
`position-aware-tooltips` (qual fallback `position-try` está ativo) e
`sanitize-untrusted-html` (instrumentação de runtime). Nenhum entra no v0.6.

## 5.6 Sobreposições com checks existentes (E e parciais)

- **E puro:** `single-line-overflow` → `ellipsis-without-clip`;
  `contrast-color` → `contrast-aa`/`contrast-aaa`; guia `accessibility` →
  facets `aria`/`ax`/`contrast` já cobrem a superfície determinística.
- **E parcial / complementar:** `focus-indicator`, `target-size`,
  `overflow-x-hidden-on-body`, `text-not-selectable`, `infinite-fast-animation`,
  `transition-all`, `backdrop-over-modal` aparecem como supressores ou sinais
  irmãos em vários guias. O catálogo (`04`) evita duplicá-los.
- **Falso positivo legítimo a corrigir:** `text-not-selectable` dispara no
  handle de `drag`, onde `user-select:none` é prescrito pelo guia. O check
  `drag-handle-affordance-misapplied` passa a ser o autorizado e o antigo deve
  ser suprimido nesse contexto.

## 5.7 Validação do inventário

Cada relatório foi verificado pelo subagent com: contagem de linhas da tabela,
soma das classes batendo com o total da fatia, unicidade de ids e presença de
uma seção `###` por check proposto. A consolidação aqui recontou as sete
fatias: **96 + 63 + 48 + 3 = 210**.
