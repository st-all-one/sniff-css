# Diff determinístico (`sniffCSS-diff`) e checks (`sniffCSS-check`)

Dois binários **sem IA**: reduzem dois snapshots ao que mudou (`sniffCSS-diff`) e
descobrem problemas num snapshot (`sniffCSS-check`). O LLM só avalia o resultado.

## Pipeline de diff

```bash
# 1. Extração em dois momentos, com âncora estável
sniffCSS -u URL -s ".widget" --stable-key data-testid > base.jsonl
# ... (deploy/tempo passa) ...
sniffCSS -u URL -s ".widget" --stable-key data-testid > head.jsonl

# 2. Diff determinístico
sniffCSS-diff base.jsonl head.jsonl --tolerance 0.5 > delta.jsonl
#   --ignore-props transform,opacity   # props voláteis não marcam o nó
#   --no-structural                    # suprime ADDED/REMOVED (listas variáveis)

# 3. Resumo em escala (centenas de páginas, zero tokens)
sniffCSS-diff base.jsonl head.jsonl --stats-only
# nodes: 14 -> 14 | changed: 1 | added: 0 | removed: 0
```

### Como funciona

- **Match de nós** por `selector` estável, com fallback posicional (estrutura).
- **Comparação por propriedade** com tolerância numérica (`--tolerance`) para
  absorver jitter de subpixel (`16px` → `16.2px`); unidades diferentes nunca
  são tratadas como iguais.
- **Saída JSONL** com `CHANGED` (deltas `before`/`after` por propriedade, incluindo
  `styles`, `pseudo`, `aria`, `contrast`, `ax`, `rect`, `metrics`, `is_user_noticeable`)
  e `ADDED`/`REMOVED` (snapshot completo do nó).

```jsonc
{"status":"CHANGED",
 "selector":"button[data-testid=\"submit\"]",
 "tag":"BUTTON","depth":1,
 "changes":{"styles":{"visual":{"background-color":
   {"before":"#2563eb","after":"#16a34a"}}},
            "rect":{"before":{"x":141.8,...},"after":{"x":121.1,...}}}}
```

### Flags

| Flag | Efeito |
|---|---|
| `--tolerance N` | Absorve jitter numérico na mesma unidade (padrão `0.5`). |
| `--ignore-props a,b` | Mudanças nessas props nunca marcam o nó como changed. |
| `--no-structural` | Suprime `ADDED`/`REMOVED` (reporta só `CHANGED`) — feeds de listas variáveis. |
| `--stats-only` | Só o resumo `nodes: N -> M | changed/added/removed`. |

> ⚠️ **Determinismo**: use o **mesmo modo** nos dois lados (`--compact` + `--compact`,
> ou full + full). O hash e o conteúdo dependem do modo.

## Checks determinísticos (`sniffCSS-check`)

```bash
sniffCSS-check --input snap.jsonl --uniform --tolerance 0.5   # o "card estranho"
sniffCSS-check --input snap.jsonl --rules                     # PASS/WARN/FAIL
```

- **`--uniform`**: entre instâncias irmãs do mesmo selector, computa a norma do
  grupo (mediana para números, moda caso contrário) e reporta os **outliers**
  com as propriedades e magnitudes que desviam (ex.: um card de altura 80px
  numa fila de 120px).
- **`--rules`** (regras derivadas, com evidência medida):
  - `contrast-aa` / `contrast-aaa` — usa o facet `contrast` da engine (fundo
    efetivo resolvido); `fail` = falha real, `warn` = fundo-imagem. Nós sem
    texto direto (`has_text=false`) reportam `unknown` em vez de um ratio
    potencialmente incorreto (cor herdada de pai sem visibilidade).
  - `target-size` — alvo clicável < 24×24px (WCAG 2.2).
  - `focus-indicator` — focusable sem sinal de foco visível.
  - `hidden-focusable` — focusable com `accessibility_grade == NONE` (tab trap).
  - `empty-alt-image` — imagem grande com `alt=""` (não decorativa?).
  - `occluded` — o elemento está **visualmente atrás** de outro que o cobre:
    detecta sobreposição de `rect` entre nós não-ancestrais/não-descendentes
    dentro da árvore capturada, com quem pinta por cima decidido por heurística
    determinística (`z-index` numérico de `metrics`, senão a ordem no DOM).
    `fail` = ≥75% da área coberta por um único nó; `warn` = ≥50%. Otimizado por
    sweep no eixo x (só pares com sobreposição em x são testados em y).
    Elementos SVG irmãos dentro do mesmo `<svg>` são ignorados (SVG renderiza
    em ordem do documento por design). Limite:
    um elemento *fora* da profundidade de captura pode ocultar sem ser visto.
  - **Erros comuns de UI/UX** (heurísticas determinísticas de CSS; `warn`
    exceto onde indicado):
    - `sticky-in-overflow-hidden` — `position:sticky` com ancestral
      `overflow != visible` → a fixação nunca engata.
    - `fixed-broken-by-transform` — `position:fixed` com ancestral com
      `transform`/`filter`/`will-change`/`perspective`/`contain:paint` → o
      elemento fica relativo a esse ancestral, não à viewport.
    - `absolute-without-insets` — `position:absolute` sem `top/right/bottom/left`
      → fica na posição estática (dropdown/overlay deslocado).
    - `interactive-pointer-events-none` — **`fail`**: interativo com
      `pointer-events:none` não pode ser clicado.
    - `aria-hidden-focusable` — **`fail`** (WCAG 4.1.2): `aria-hidden="true"`
      em elemento focusable → foco em conteúdo invisível.
    - `ellipsis-without-clip` — `text-overflow:ellipsis` sem `overflow:hidden`
      (e sem `white-space:nowrap`) → o ellipsis não aplica.
    - `width-100-with-padding` — `width:100%` + `box-sizing:content-box` +
      padding → overflow horizontal garantido (use `border-box`).
    - `small-text` — `font-size < 12px` em texto visível (pula `font-size:0`,
      que é técnica de ocultação de texto).
    - `small-thumbnail` — thumbnail de imagem com `background-image` e
      `cursor:pointer` (ou tag interativa) menor que 150×150px; galerias com
      imagens pequenas demais para visualização confortável.
    - `line-height-below-font-size` — `line-height < font-size` (unitless
      resolvido contra o font-size) → glifos cortados/sobrepostos.
    - `z-index-on-static` — `z-index` numérico em `position:static` (pai não
      flex/grid) → ignorado pelo navegador.
    - `control-without-name` — interativo visível sem nome acessível (icone-only
      sem `aria-label`/label). Verifique se não há um `<img alt>` descendente
      antes de reportar.
    - `text-not-selectable` — `user-select:none` em texto de corpo → usuário
      não copia.
    - `infinite-fast-animation` — animação infinita com ciclo < 0.5s → risco de
      flashing (WCAG 2.3.1).
    - `transition-all` — `transition-property:all` → repaint caro.
    - `overflow-x-hidden-on-body` — `overflow-x:hidden` no documento mascara o
      scroll horizontal.
    - `horizontal-overflow` — elemento visível ultrapassa a largura da viewport
      (grava no `__meta.viewport`) sem ancestral que o clipe → scroll horizontal
      / CLS.
    - `backdrop-over-modal` — **`fail`**: um backdrop escuro translúcido
      (`position:fixed`, ~viewport, `rgba(0,0,0,<1)`) pinta **por cima** do
      conteúdo do modal. No CSS um filho pinta *acima* do fundo do pai, então a
      regra só dispara quando o diálogo pinta **abaixo** do scrim (z-index
      negativo) — o caso ancestral/descendente que o `occluded` ignora por
      design. Requer `__meta.viewport`.

Saída JSONL com evidência + `__check_summary`:

```jsonc
{"check":"contrast-aa","selector":"footer .text","tag":"P","status":"fail",
 "evidence":"ratio 2.1:1 on #212529 against #020842 (need 4.5:1 text AA)"}
{"check":"uniformity","selector":"div.card:nth-child(3)","status":"fail",
 "evidence":"deviates from the 3/3 group norm: box_model.height: 80px (norm 120px ±40.00)"}
{"check":"occluded","selector":"button.save","tag":"BUTTON","status":"fail",
 "evidence":"100% of button.save is covered by div.modal-backdrop — the element is visually behind an overlapping element"}
{"check":"backdrop-over-modal","selector":"div.modal-content","tag":"DIV","status":"fail",
 "evidence":"100% of div.modal-content is covered by the modal backdrop div.backdrop — the dialog paints below its own dark scrim (negative z-index); check the stacking order"}
{"__check_summary":{"uniformity_instances":3,"uniformity_outliers":1,"rules":12}}
```

O resultado vira **evidência** para o `reason` da avaliação IA (veja
[`eval-prompt.md`](eval-prompt.md) e [`sniffCSS-eval.schema.json`](sniffCSS-eval.schema.json)).

## Qualidade e não-regressão

O baseline de regressão da suíte (`cargo test --workspace`) cobre cada regra
com **testes unitários determinísticos sem navegador** (fixtures `DiffNode`
sintéticos) + testes de integração Chrome-gated (auto-skip sem Chromium).
Cada regra tem um caso positivo e um de negação, incluindo os
falsos-positivos que surgiram em página real:

- `line-height: normal` resolve contra o `font-size` (~1.2×), não como `1.2px`.
- `backdrop-over-modal` só dispara com z-index negativo no diálogo (filho
  acima do scrim é o comportamento correto; conteúdo de página atrás de um
  modal real é normal).
- Scrim normalizado pela engine (`rgba(0,0,0,0.6)` → `#00000099`) é
  reconhecido pela regra.
- `horizontal-overflow` exige `__meta.viewport` e ignora nós clippados por
  ancestral `overflow`.
- `occluded` ignora SVGs irmãos dentro do mesmo `<svg>` (ordem do documento).
- `small-text` ignora `font-size:0` (técnica de ocultação de texto).
- `contrast` pula nós sem `has_text` (texto em filho, não no nó direto).

Para rodar: `cargo test --workspace` (baseline ~413 testes) ·
`cargo clippy --workspace --all-targets -- -D warnings` ·
`cargo fmt --all -- --check`.
