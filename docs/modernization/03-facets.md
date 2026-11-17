# 3. Facetas — especificação detalhada

Convenção da v0.6: facetas globais são **uma linha** e vêm **ligadas por
default** quando dão contexto ao agente (`attrs`, `__document`, `__env`);
`--minimal` desliga esse conjunto. Facetas caras (`declared`, matriz) continuam
opt-in e têm flag própria. A leitura de v1 é migrada em memória.

---

## F-A. `attrs` preset (extensão de facet existente)

**Situação atual:** já existe `attrs` verbatim por nó, opt-in via `--attrs NOME`
(repetível). O que falta é um conjunto padrão para detecção de oportunidades.

**Preset `forms`** (14 nomes, dedupe com `aria` facet):

```
type, name, id, value, autocomplete, inputmode, required, disabled, min, max,
pattern, placeholder, form, for
```

**Preset `media`** (9 nomes):

```
alt, src, srcset, sizes, loading, fetchpriority, width, height, poster
```

**Preset `platform`** (8 nomes):

```
popover, popovertarget, inert, open, role, tabindex, href, target
```

**Preset `modern`** = união curada das três (≈20 nomes, excluindo atributos já
cobertos por `aria`). **Preset `all`** = todos os atributos
(`getAttributeNames()`), diagnóstico, nunca default.

**Flag:** `--attrs-preset forms|media|platform|modern|all` (comportamento atual
de `--attrs` preservado; presets somam-se aos nomes explícitos). No v0.6,
`modern` é o default; `--no-attrs` ou `--minimal` desligam.

**Custo:** ~8–14 strings curtas por nó nos presets parciais; ≈20 em `modern`.
Medido com `--depth 10` antes do release (gate < 10% de overhead). Não incluir
atributos já presentes no facet `aria` (`aria-label`, `title`, `alt` em
`aria.alt`, etc.).

**Falha:** atributo ausente simplesmente não aparece no mapa (comportamento
atual de `getAttribute`). Nenhum erro.

**Checks destravados:** `autofill-*`, `image-reserved-space`, `image-lazy`,
`fetchpriority-lcp`, `native-popover`, `native-dialog`, `inert-usage`,
`link-rel`, `inputmode`, `form-label-association`.

---

## F-B. `__document` — contexto do documento

**Flag:** `--document` — **ligado por default no v0.6**; `--no-document` ou
`--minimal` desligam.

```jsonc
{"__document": {
  "title": "Dashboard | Exemplo",
  "lang": "pt-BR",
  "doctype": "html",
  "meta": {
    "viewport": "width=device-width, initial-scale=1",
    "text-scale": "scale",
    "color-scheme": "light dark",
    "theme-color": "#0b5fff"
  },
  "links": [
    {"rel":"manifest","href":"/manifest.webmanifest"},
    {"rel":"preload","as":"image","href":"/hero.avif","fetchpriority":"high"}
  ],
  "scripts": [
    {"src":"/app.js","type":"module","defer":true,"async":false},
    {"src":"/legacy.js","type":null,"defer":false,"async":false}
  ],
  "style_sheets": [
    {"href":"/app.css","media":"all","blocked":false},
    {"href":"https://cdn.outro/theme.css","media":"all","blocked":true}
  ]
}}
```

**Mecanismo:** mesma chamada JS da extração (`document.title`,
`document.documentElement.lang`, `document.doctype?.name`, `meta`/`link`/
`script` via `querySelectorAll`, `document.styleSheets` com `try/catch` por
folha). Cross-origin lança `SecurityError`: registrar `blocked: true` e seguir.

**Ordem determinística:** ordem do DOM para `meta`/`link`/`scripts`; ordem de
`document.styleSheets` para `style_sheets`. Chaves de `meta` em ordem
alfabética (`BTreeMap`). Nunca serializar `document` inteiro.

**Custo:** 1 linha, tipicamente 0,3–2 KB.

**Checks destravados:** `text-scale-optin`, `root-font-size-override`,
`color-scheme-declared`, `missing-lang`, `missing-title`, `duplicate-main`
(via árvore + title), `script-blocking`, `manifest-present`, `lcp-preload`,
`image-dimensions` (com `attrs`).

**Testes:** parser puro com JSON sintético (sem browser) + integração
Chrome-gated com folha cross-origin simulada para o ramo `blocked`.

---

## F-C. `__env` — preferências do usuário e emulação

**Flag:** `--env` — **ligado por default no v0.6**; `--no-env` ou `--minimal`
desligam.

```jsonc
{"__env": {
  "media": {
    "prefers-color-scheme": "dark",
    "prefers-reduced-motion": "no-preference",
    "prefers-contrast": "no-preference",
    "forced-colors": "none",
    "pointer": "fine",
    "hover": "hover",
    "any-pointer": "fine",
    "any-hover": "hover"
  },
  "emulated": {"prefers-reduced-motion": "reduce"},
  "device": {"dpr": 2, "width": 1366, "height": 768}
}}
```

**Normalização obrigatória** (nunca serializar `MediaQueryList` cru):

| Chave | Consulta | Valores canônicos |
|---|---|---|
| `prefers-color-scheme` | `(prefers-color-scheme: dark)` e `(prefers-color-scheme: light)` | `dark`, `light`, `no-preference` |
| `prefers-reduced-motion` | `(prefers-reduced-motion: reduce)` | `reduce`, `no-preference` |
| `prefers-contrast` | `(prefers-contrast: more)`, `: less`, `: custom` | `more`, `less`, `custom`, `no-preference` |
| `forced-colors` | `(forced-colors: active)` | `active`, `none` |
| `pointer` / `any-pointer` | `(pointer: fine)`, `(pointer: coarse)` | `fine`, `coarse`, `none` |
| `hover` / `any-hover` | `(hover: hover)` | `hover`, `none` |

`emulated` registra **o que a CLI pediu**, não o que o browser reporta — é o
que permite distinguir "usuário prefere reduce" de "nós emulamos reduce".
Quando os dois diferem, o check de preferência usa `media`; quando iguais,
registra a origem em `evidence` para transparência.

**Interação com estabilização (importante):** `sniffer.rs` já emula
`prefers-reduced-motion: reduce` para estabilizar. Se o usuário pedir
`--emulate prefers-reduced-motion=no-preference`, a emulação explícita
**vence** a estabilização, e `__env.emulated` registra o valor final. Sem
essa regra, o check "a página ignora reduced motion" daria falso positivo.

**Checks destravados:** `ignores-reduced-motion`, `no-dark-mode`,
`no-forced-colors-support`, `coarse-pointer-targets`, `color-scheme-mismatch`.

**Testes:** todos os valores canônicos via `Emulation.setEmulatedMedia` em
teste Chrome-gated; parser puro para os demais.

---

## F-D. `__declared` — origem das declarações (CSSOM)

**Flag:** `--declared`, com `--declared-limit N` (default 200).

```jsonc
{"__declared": {
  "rules": [
    {"selector":"html","href":"/app.css","index":12,"origin":"author",
     "declarations":{"font-size":"16px"}},
    {"selector":"html","href":null,"index":0,"origin":"inline",
     "declarations":{"color":"red"}}
  ],
  "blocked_sheets": ["https://cdn.outro/theme.css"],
  "at_rules": [{"conditionText":"@supports (anchor-name: --x)","index":3,"rules":[]}],
  "truncated": false
}}

**Mecanismo:** na mesma chamada JS, para cada seletor capturado, percorrer
`document.styleSheets`; para cada `CSSStyleRule`, testar
`element.matches(rule.selectorText)` em `try/catch` (seletores com suporte
parcial podem lançar) e coletar as declarações casadas. Inline via
`element.style`. Ordem: `(href, index, selector)`. Prelúdios de at-rules
(`@container style(...)`, `@supports(...)`) são capturados em
`at_rules: [{conditionText, index, rules}]` — necessário para 4 guias de CSS.

**Por que é necessário:** ver `01-diagnostico.md` §L1 — `font-size:16px` no
`:root` é simultaneamente o default e o anti-padrão; só a origem distingue.

**Custo:** O(nº de regras × nº de seletores capturados) **na página**, por isso
o limite. Não é feito por nó na engine — é devolvido como linha global.

**Falha:** folha cross-origin → nome em `blocked_sheets`; não é erro, mas os
checks dependentes devem emitir `evidence` dizendo que a origem não pôde ser
verificada. `truncated: true` desliga checks que exigem certeza.

**Checks refinados:** `root-font-size-override`, `focus-outline-suppressed`,
`native-appearance-reset`, `animation-on-layout-props` (via `@keyframes`),
`redundant-aria` (quando o valor vem do HTML, não do CSS).

---

## F-E. `__meta.matrix` — matriz de viewport / emulação

**Flags:** `--viewport-matrix 360x800,768x1024,1366x768`,
`--emulate prefers-color-scheme=dark`, `--emulate pointer=coarse`.

```jsonc
{"__meta": {
  "viewport": {"width": 360, "height": 800},
  "matrix": {
    "index": 0,
    "total": 3,
    "viewport": {"width": 360, "height": 800},
    "emulation": {"prefers-color-scheme": "dark"}
  },
  "modern_profile": "default"
}}
```

**Mecanismo:** `sniffer.rs` repete o pipeline por estado, aplicando
`Emulation.setDeviceMetricsOverride` e `Emulation.setEmulatedMedia` antes da
navegação/captura, e restaura ao final. Cada estado mantém o **mesmo**
`selector`, `depth` e `stable-key` — sem isso o diff entre estados é inválido.

**Ordem:** a ordem da config, com `index` começando em 0. Nunca ordenar por
conteúdo. `total` permite ao `check` detectar matriz incompleta e reclamar.

**Custo:** linear em N (cada estado é uma captura completa). Default N=1.
`--json` deve avisar em `warnings` quando N>1.

**Checks destravados:** `breakpoint-jump`, `overflow-only-at-width`,
`dark-mode-ignored`, `reduced-motion-ignored`, `coarse-targets`.

---

## F-F. Métricas agregadas de DOM (derivadas, sem captura)

Não é facet de captura: é calculada **offline** em `sniff check` a partir da
floresta já carregada.

```jsonc
{"__dom_stats": {
  "nodes": 412, "max_depth": 9,
  "images": 38, "images_without_dims": 12,
  "interactive": 54, "interactive_without_name": 3,
  "iframes": 2, "scripts": 7
}}
```

Exposta em `--json.data.dom_stats` no default do `sniff check` (e também com
`--modern`). Custo: uma passagem sobre a floresta, reaproveitada pelo
`flatten` de `rules.rs`.

**Checks destravados:** `content-visibility-candidate`,
`heavy-dom-region`, `image-dimensions-summary`.

---

## Matriz de dependência: facet × checks

| Facet | Checks que destrava | Custo |
|---|---|---|
| `attrs` (preset) | forms, mídia, plataforma, priorização | por nó |
| `__document` | text scale, color-scheme, lang/title, scripts, manifest | 1 linha |
| `__env` | preferências do usuário | 1 linha |
| `__declared` | desambiguação de defaults e resets | 1 linha + custo in-page |
| `__meta.matrix` | responsividade e dark mode comparativos | N × captura |
| `__dom_stats` | peso de DOM / imagens (derivado) | 1 passagem offline |

## Regras comuns de falha

1. Facet ausente → check não emite linha; nunca "passa" por omissão.
2. Facet parcial (`blocked`, `truncated`) → `evidence` declara a limitação.
3. Nenhuma facet nova pode introduzir leitura de `env`, relógio ou aleatoriedade.
4. Toda facet nova precisa de teste de parser puro + teste Chrome-gated do
   caminho que a produz.
