# Diff determinístico (`sniff diff`) e checks (`sniff check`)

Dois binários **sem IA**: reduzem dois snapshots ao que mudou (`sniff diff`) e
descobrem problemas num snapshot (`sniff check`). O LLM só avalia o resultado.

> Ambos aceitam o snapshot por **stdin** (`-`) e entrada estruturada
> (`--params '<json>'` / `--batch file.jsonl`), e `--dry-run` para validar sem
> executar. Ver [`cli-surface.md`](cli-surface.md) §5–6.

## Pipeline de diff

```bash
# 1. Extração em dois momentos, com âncora estável
sniff capture URL -s ".widget" --stable-key data-testid > base.jsonl
# ... (deploy/tempo passa) ...
sniff capture URL -s ".widget" --stable-key data-testid > head.jsonl

# 2. Diff determinístico
sniff diff base.jsonl head.jsonl --tolerance 0.5 > delta.jsonl
#   --ignore-props transform,opacity   # props voláteis não marcam o nó
#   --no-structural                    # suprime ADDED/REMOVED (listas variáveis)

# 3. Resumo em escala (centenas de páginas, zero tokens)
sniff diff base.jsonl head.jsonl --stats-only
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

## Checks determinísticos (`sniff check`)

```bash
sniff check snap.jsonl --uniform --tolerance 0.5   # o "card estranho"
sniff check snap.jsonl --rules                     # só quebras (PASS/WARN/FAIL)
sniff check snap.jsonl --modern                    # só oportunidades de plataforma
```

### Flags de `sniff check`

| Flag | Efeito |
|---|---|
| *(nenhuma)* | roda `uniformity` + `rules` + `modern`: quebras **e** oportunidades |
| `--rules` | somente quebras (`class:"bug"`) — **mudança de significado**: antes significava "rode as regras" |
| `--modern` | somente oportunidades (`class:"opportunity"`) |
| `--no-modern` | desliga oportunidades (equivalente ao antigo default) |
| `--uniform` | mantido por compatibilidade; a detecção de outlier já roda no default |
| `--guide ID` | filtra oportunidades por id de guia do corpus |
| `--ignore CHECK` | suprime um `check_id` da execução (repetível) |
| `--profile default\|strict` | `default` mantém oportunidades como `warn`; `strict` eleva todas as oportunidades a `fail` para gate de CI (registrado em `__check_summary.modern_profile`) |
| `--responsive` \| `--no-responsive` | checks que comparam estados de `__meta.matrix`; **ligados por padrão**, `--no-responsive` desliga |

`--rules` e `--modern` são mutuamente exclusivos na intenção (quebras x
oportunidades); `--no-modern` é o escape hatch para quem quer só o comportamento
anterior.
O resumo `__check_summary` ganha `modern: N`, `class` (`bug`/`opportunity`),
`modern_profile` e `opportunities_by_class: {"opportunity": N}` — a
classificação D/F/N/E do corpus fica em
[`modernization/05-inventario-guias.md`](modernization/05-inventario-guias.md).

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

Saída JSONL com `evidence` tipado e `__check_summary`:

```jsonc
{"check":"contrast-aa","selector":"footer .text","tag":"P","status":"fail",
 "class":"bug","severity":"fail",
 "evidence":{"text":"ratio 2.1:1 on #212529 against #020842 (need 4.5:1 text AA)",
   "metric":"contrast_ratio","observed":2.1,"threshold":4.5,
   "comparator":">=","source":"contrast_facet"},
 "metrics":{"font_size_px":16,"text_size":"normal"}}
{"check":"uniformity","selector":"div.card:nth-child(3)","status":"fail",
 "class":"bug",
 "evidence":{"text":"deviates from the 3/3 group norm: box_model.height: 80px (norm 120px ±40.00)",
   "metric":"group_deviation","observed":80,"threshold":40,
   "comparator":"<=","source":"uniformity"}}
{"check":"native-dialog-reimplemented","selector":"div.modal","tag":"DIV","status":"warn",
 "class":"opportunity","guide":"dialog","severity":"warn",
 "evidence":{"text":"div.modal reimplements a dialog with role=dialog instead of <dialog>",
   "source":"attrs+aria"}}
{"__check_summary":{"uniformity_instances":3,"uniformity_outliers":1,"rules":12,"modern":4,
 "class":{"bug":13,"opportunity":4},"opportunities_by_class":{"opportunity":4},
 "modern_profile":"default"}}
```

`observed`/`threshold`/`comparator`/`metric` aparecem só em checks com limiar
numérico; `text` e `source` estão sempre presentes. `source` diz qual facet
sustenta a afirmação (contraste, `attrs`, `__document`, `declared`, ...) para o
agente calibrar confiança. Filtros determinísticos não precisam mais de regex
sobre prosa:

```bash
sniff check snap.jsonl | jq -c 'select(.evidence.observed < .evidence.threshold)'
sniff check snap.jsonl | jq -c 'select(.class == "opportunity" and .guide == "dialog")'
```

O resultado vira **evidência** para o `reason` da avaliação IA (veja
[`eval-prompt.md`](eval-prompt.md) e [`sniff-eval.schema.json`](sniff-eval.schema.json)).

## Oportunidades de plataforma

Além das **quebras** (bugs), `sniff check` reporta **dívida de plataforma**:
padrões que funcionam, mas lutam contra recursos nativos da web. São
oportunidades determinísticas, derivadas somente do snapshot — nenhuma LLM e
nenhuma leitura de código-fonte. O catálogo completo está em
[`modernization/04-catalogo-de-checks.md`](modernization/04-catalogo-de-checks.md);
as flags estão na tabela acima.

- **`class`** — toda linha diz `class:"bug"` (quebra detectada: contraste AA,
  alvo pequeno, oclusão, ...) ou `class:"opportunity"` (dívida de plataforma:
  modal reimplementado sem `<dialog>`, `autocomplete` ausente, imagem sem
  dimensões reservadas, ...). É o campo mais barato para filtrar num pipeline.
- **`evidence` tipado** — objeto com `text` e `source` sempre presentes e, em
  checks com limiar numérico, `metric`/`observed`/`threshold`/`comparator`.
  `metrics` traz fatos auxiliares. Permite agregação determinística sem
  parsing de prosa (ex.: `jq 'select(.evidence.observed < .evidence.threshold)'`).
- **`guide`** — id do guia no corpus de modernização (ex.: `dialog`, `forms`,
  `respect-os-text-scale`); presente só em oportunidades. É a ponte para a
  recomendação exata: consulte o guia antes de agir, pois ele descreve o
  fallback e o Baseline que a recomendação preserva.
- **`keepWhen`** — condição que **suprime** o check para evitar falso
  positivo. Faz parte da especificação e dos testes de cada check (ver os
  relatórios em [`modernization/findings/`](modernization/findings/)), mas
  **não vai no JSON**: é documentação, não ruído de token. Se uma
  oportunidade parecer errada, confira o `keepWhen` da origem antes de usar
  `--ignore`.
- **Padrão e escape hatches** — sem flags, `uniformity` + `rules` + `modern`
  rodam juntos. Use `--rules` para só quebras, `--modern` para só
  oportunidades, `--no-modern` para o comportamento anterior,
  `--guide ID`/`--ignore CHECK` para recortar o conjunto e `--profile strict`
  para tratá-las como gate de CI. Facetas ausentes silenciam o check que delas
  depende — nunca viram `pass` por omissão.
- **Captura padrão completa** — `__document`/`__env`/`__declared`, attrs e
  matriz de 12 estados (3 viewports × light/dark/reduce/coarse) já vêm sem
  flags. `--emulate K=V` reduz a um perfil, `--no-responsive` desliga as
  comparações e `--minimal` remove todas as fatias v2. Checks por nó rodam só
  no primeiro estado; comparações de matriz usam os 12.
- **Heurísticas que exigem confirmação humana** — `breakpoint-jump`,
  `dark-mode-ignored`, `overflow-only-at-width`, `inline-nontrivial-style` e
  checks de ausência de `@view-transition` são oportunidades, não bugs.
  Confira `evidence.source`/`guide` e o `keepWhen` da origem antes de agir.
  Falsos positivos já corrigidos: SRI same-origin com `:443` explícito,
  `content:none` em pseudo-elemento e reflow vertical proporcional ao viewport.
  Se aparecer outro, adicione `keepWhen` + teste de negação — não globalize o
  `--ignore`.


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

Cada oportunidade da família `modern` tem **triplet de teste**: caso positivo,
caso de negação pelo `keepWhen` e verificação de que o `guide` existe no corpus.
Checks com facet ausente silenciam (teste de negação por facet), e
`__dom_stats` é derivado offline com teste de contagem.

Para rodar: `cargo test --workspace` (baseline ~413 testes) ·
`cargo clippy --workspace --all-targets -- -D warnings` ·
`cargo fmt --all -- --check`.
