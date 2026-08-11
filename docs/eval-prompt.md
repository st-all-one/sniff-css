# Prompt de avaliação semântica (camada IA)

Use este template quando a camada determinística (`sniff-diff`) já tiver
produzido o delta. O delta contém **apenas** os nós que mudaram; não mande
snapshots completos para o LLM.

```text
Você é um avaliador de qualidade de UI. Avalie o delta JSONL abaixo,
produzido entre duas execuções de sniffing de computador style.

Retorne SOMENTE um JSON que valide contra docs/sniff-eval.schema.json.

Pilares de avaliação:
A) Acessibilidade: contraste (color vs background-color, WCAG), área
   clicável, atributos ARIA removidos/adicionados.
B) Estabilidade de layout (CLS/UX): dimensões fixas, position/z-index,
   sobreposição de elementos, is_visible mudando sem razão.
C) Hierarquia visual / design system: alinhamento a escala (múltiplos de
   4px/8px), unificação de tipografia, fontes não mapeadas.

Instruções:
- "status" = REGRESSION_DETECTED se qualquer mudança for negativa,
  senão IMPROVEMENT se houver melhoria clara, senão NEUTRAL.
- "score_change" de -100 a +100 refletindo o balanço geral.
- "changes_evaluated": um item por nó do delta; "reason" deve citar os
  valores antes/depois (ex: "contraste caiu de 4.5:1 para 2.1:1").

Delta JSONL:
<cole o output de `sniff-diff base.jsonl head.jsonl` aqui>
```

## Uso em escala

1. Extração (determinística): `sniff-computed-style --stable-key data-testid ...`
2. Diff (determinística): `sniff-diff base.jsonl head.jsonl > delta.jsonl`
3. Avaliação (IA): envie `delta.jsonl` + este prompt; valide a resposta com
   `jq -e 'has("status") and has("score_change")'` ou um validator JSON Schema.

Só etapa 3 gasta tokens de LLM, e apenas sobre o delta (dezenas de linhas,
não milhares de nós).
