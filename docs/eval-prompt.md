# Prompt de avaliação semântica (camada IA)

Use este template quando a camada determinística (`sniffCSS-diff` + `sniffCSS-check`)
já tiver produzido o delta e as evidências. O delta contém **apenas** os nós
que mudaram; não mande snapshots completos para o LLM. Funciona igual para os
dois backends: web (`sniffCSS_page`) e Flutter/Dart (`sniffFlutter_page`) —
ambos emitem o mesmo modelo JSONL (no Flutter o `tag` é a classe do widget,
ex. `FilledButton`, e os selectors usam `[<'key'>]` quando há `ValueKey`).

```text
Você é um avaliador de qualidade de UI. Avalie o delta JSONL abaixo,
produzido entre duas execuções de sniffing (web ou Flutter), e as evidências
de checks determinísticos (sniffCSS-check).

Retorne SOMENTE um JSON que valide contra docs/sniffCSS-eval.schema.json.
Preencha o bloco opcional "measured" apenas com valores que o delta, o
snapshot ou os checks realmente emitiram — usando os NOMES DE CAMPO da
ferramenta (contrast.ratio/aa/aaa, aria.role/name/focusable, ax.role/name/
ignored, is_user_noticeable.display_visible/accessibility_grade, rect,
uniformity.property/group_norm/value, rule.check/status, action.effect).

Pilares de avaliação:
A) Acessibilidade: contraste (color vs background-color, WCAG AA/AAA). Use o
   facet `contrast` do delta quando presente — ratio e AA/AAA são MEDIDOS
   (4.5:1 normal, 3.0:1 texto grande, 7.0/4.5 para AAA). Cite o ratio real.
   ARIA: deltas de `aria.role`/`aria.name`/`aria.focusable` e do nó `ax`
   (role/name/ignored computados pelo Chrome).
B) Estabilidade de layout (CLS/UX): dimensões fixas, position/z-index,
   sobreposição, `is_user_noticeable` mudando sem razão (`display_visible`
   flip-flop ou `accessibility_grade` caindo), deltas de `rect`. Se o check
   `occluded` apontar que o elemento está coberto por outro (o elemento fica
   **atrás** de um overlay/elemento sobreposto), trate como regressão de
   layout/visibilidade — é a evidência de que o usuário não enxerga o elemento
   mesmo ele existindo no DOM.
C) Hierarquia visual / design system: alinhamento a escala (múltiplos de
   4px/8px), unificação de tipografia, fontes não mapeadas.
D) Evidências de sniffCSS-check: outliers de `uniformity` (instância que desvia
   da norma do grupo) e `fail`/`warn` de regras (contrast-aa/contrast-aaa,
   target-size, focus-indicator, hidden-focusable, empty-alt-image, occluded,
   e erros comuns de UI/UX como sticky-in-overflow-hidden, fixed-broken-by-transform,
   backdrop-over-modal, horizontal-overflow, etc.).
E) Interações (deltas `ACTION_CHANGED`/`ACTION_ADDED`/`ACTION_REMOVED` das
   `__actions`): o que cada ação revelou/ocultou/moveu e onde — effect,
   onscreen vs out-of-view, `distance_from_action`. Uma interação que mudou de
   efeito entre base e head (ex. o modal não abre mais, ou abre fora da dobra)
   é regressão de UX mesmo sem mudança de estilo.
   No Flutter, `accessibility.enabled` (widget desabilitado) também conta.

Instruções:
- "status" = REGRESSION_DETECTED se qualquer mudança for negativa,
  senão IMPROVEMENT se houver melhoria clara, senão NEUTRAL.
- "score_change" de -100 a +100 refletindo o balanço geral.
- "category" por item: ACCESSIBILITY | LAYOUT_STABILITY | VISUAL_HIERARCHY |
  DESIGN_SYSTEM | INTERACTION | PERFORMANCE | OTHER.
- "changes_evaluated": um item por nó do delta (ou por linha ACTION_*);
  "reason" deve citar os valores antes/depois (ex: "contrast.ratio caiu de
  4.5 para 2.1 (fail AA)") e "node_selector" deve reproduzir o selector do
  delta (no Flutter, ex. `FilledButton-[<'counter'>][0]`; para interações,
  `__actions[N]`).
- NUNCA invente números: cite apenas ratios/roles/desvios presentes no delta
  ou nas evidências de checks.

Delta JSONL:
<cole o output de `sniffCSS-diff base.jsonl head.jsonl` aqui>

Evidências de checks (se disponíveis):
<cole o output de `sniffCSS-check --input head.jsonl --uniform --rules` aqui>
```

## Uso em escala

1. Extração (determinística): `sniffCSS --stable-key data-testid ...` (default otimizado: compact, stabilize, contrast e ax já ON); no Flutter `sniffCSS -u flutter://<serial> --project DIR --depth N`.
2. Diff (determinística): `sniffCSS-diff base.jsonl head.jsonl --ignore-props ... --no-structural ... > delta.jsonl`
3. Descoberta (determinística): `sniffCSS-check --input head.jsonl --uniform --rules > checks.jsonl`
4. Avaliação (IA): envie `delta.jsonl` + `checks.jsonl` + este prompt; valide a
   resposta com `jq -e 'has("status") and has("score_change")'`.

Só a etapa 4 gasta tokens de LLM, e apenas sobre o delta + checks (dezenas de
linhas, não milhares de nós).
