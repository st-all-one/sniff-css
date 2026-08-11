# Guia de uso — otimizado para IA

Este guia mostra como usar `sniff-computed-style` + `sniff-diff` do jeito certo
quando o consumidor final é um modelo de IA (agente, MCP, pipeline de regressão).

## Filosofia: a IA deve receber só o delta

```
sniff-computed-style  (extração determinística: a verdade exata)   ─┐
                                                                    ├─► sniff-diff (determinístico) ─► delta pequeno ─► LLM
sniff-computed-style  (segunda execução, mesmos parâmetros)       ─┘
```

A extração e o diff são **sem IA** e custam ~0 tokens. O LLM só vê o delta
(medido: ~79% menos tokens que os snapshots completos).

## 1. Flag‑set recomendado para captura

Para uso com IA, capture sempre com o mesmo conjunto de flags nas duas runs:

```bash
sniff-computed-style \
  --url "http://localhost:3000/checkout" \
  --selector ".checkout-form" \
  --depth 2 \
  --categories all \
  --compact \
  --custom-props \
  --stable-key data-testid
```

| Flag | Por quê |
|---|---|
| `--categories all` | Só os ~250 props do catálogo, nunca as ~400 do navegador. |
| `--compact` | Dedup lógico/físico + supressão de defaults + `css_variables` escopado → ~55% menos tokens. |
| `--custom-props` | Captura as variáveis CSS (`--*`); global no `__meta`, overrides por nó. |
| `--stable-key data-testid` | Seletores estáveis entre deploys → diff confiável. |
| `--depth N` | Subárvore controlada; `0` = só o elemento. |

Saída (padrão `jsonl`, árvore aninhada):

```jsonc
{"__meta":{"css_variables":{/* global :root, uma única vez */}}}
{"id":1,"tag":"DIV","selector":"div[data-testid=\"form\"]",
 "path":"div[data-testid=\"form\"]","depth":0,
 "is_visible":true,
 "computed_style_hash":"afbd33ba764bb8d4",
 "rect":{...},"metrics":{...},
 "styles":{"box_model":{...},"visual":{...}, ...},
 "children":[/* aninhado */]}
```

Campos derivados que a IA **não precisa inferir**:

- `is_visible` — derivado de `display`/`visibility`/`opacity`/rect.
- `computed_style_hash` — xxHash64 dos estilos efetivos; igual entre runs
  idênticos (mesmo modo), diferente quando algo mudou.
- `id` / `parent_id` — pre-order; use `jsonl-flat` quando quiser um nó por linha.

> ⚠️ **Determinismo**: para diffar, use **o mesmo modo** nos dois lados
> (`--compact` + `--compact`, ou full + full). O hash e o conteúdo dependem do modo.

## 2. Diff determinístico (antes da IA)

```bash
sniff-diff base.jsonl head.jsonl --tolerance 0.5 > delta.jsonl
```

O que sai: só os nós que mudaram.

- `CHANGED` → `changes` com `before`/`after` por propriedade (`styles`, `pseudo`,
  `rect`, `metrics`, `is_visible`).
- `ADDED`/`REMOVED` → `snapshot` completo do nó.
- `--tolerance 0.5` absorve jitter de subpixel (`16px`→`16.2px`); unidades
  diferentes (`16px` vs `16rem`) **nunca** são consideradas iguais.
- `--stats-only` → só `nodes: N -> M | changed/added/removed` (varredura em escala).

```bash
# resumo para centenas de páginas
sniff-diff base.jsonl head.jsonl --stats-only 2>&1
# nodes: 14 -> 14 | changed: 1 | added: 0 | removed: 0
```

Exemplo de linha de delta:

```jsonc
{"status":"CHANGED",
 "selector":"button[data-testid=\"submit\"]",
 "tag":"BUTTON","depth":1,
 "changes":{"styles":{"visual":{"background-color":
   {"before":"#2563eb","after":"#16a34a"}}},
            "rect":{"before":{"x":141.8,...},"after":{"x":121.1,...}}}}
```

## 3. Avaliação semântica por IA

Só agora o LLM entra. Envie **apenas** `delta.jsonl` + o prompt de
`docs/eval-prompt.md`. A resposta deve validar contra `docs/sniff-eval.schema.json`:

```jsonc
{
  "page_url": "https://exemplo.com/checkout",
  "status": "REGRESSION_DETECTED",   // IMPROVEMENT | NEUTRAL | REGRESSION_DETECTED
  "score_change": -15,               // -100..+100
  "summary": "O botão de checkout perdeu contraste e ficou inacessível.",
  "changes_evaluated": [
    {"node_selector":"button[data-testid=\"submit\"]","impact":"NEGATIVE",
     "category":"ACCESSIBILITY",
     "reason":"Contraste caiu de 4.5:1 para 2.1:1 após mudança de fundo."}
  ]
}
```

Validação mecânica antes de confiar na resposta:

```bash
jq -e 'has("status") and has("score_change") and (.changes_evaluated|length)>0' resposta.json
```

## 4. Padrões de uso por cenário

### Agente/MCP (função de ferramenta)

Exponha os dois binários como ferramentas `execute`/`bash`:

1. **sniff** → captura o estado real de um seletor (args: url, selector, depth,
   stable-key, compact).
2. **diff** → compara dois estados e retorna o delta (args: base, head, tolerance).
3. **eval** → (opcional) o próprio modelo lê o delta e responde o schema.

Use `--output jsonl-flat` quando o agente preferir iterar um nó por linha com
`id`/`parent_id` em vez de árvore aninhada.

### Monitor de regressão (CI)

```bash
# guarda o estado atual como base de comparação
sniff-computed-style --url "$URL" --selector "$SEL" --stable-key data-testid \
  --compact > snapshots/base.jsonl

# ... no build seguinte ...
sniff-computed-style --url "$URL" --selector "$SEL" --stable-key data-testid \
  --compact > snapshots/head.jsonl

sniff-diff snapshots/base.jsonl snapshots/head.jsonl --stats-only
# falha o job se changed/added/removed > limiar
```

### Debug de um elemento pontual

```bash
sniff-computed-style -u http://localhost:3000 -s ".btn-primary" \
  --categories visual,typography --compact \
  | jq '{color:.styles.visual.color, font:.styles.typography."font-size"}'
```

## 5. Boas práticas / armadilhas

1. **Mesma viewport** entre runs (default `1366x768`). Mudou o viewport? Media
   queries e valores `%`/`vh` mudam e o diff acusa falso-positivo.
2. **Mesmo modo** (`--compact` dos dois lados) — o hash e o conteúdo dependem do modo.
3. **Âncora estável** — prefira `--stable-key data-testid`; `id` gerados
   (`react-aria-123`) quebram o match entre deploys.
4. **Tolerância** — comece com `--tolerance 0.5`; suba se houver jitter de layout
   real (fontes não carregadas, animações). Não use tolerância cega: ela também
   "engole" mudanças pequenas de verdade.
5. **Espere a página estabilizar** — use `--wait` (network-idle, element-ready,
   fonts-loaded) para capturar sempre no mesmo estado; `document.fonts.ready`
   evita variação de métricas por troca de fonte.
6. **Conecte no seu dev server** — `--connect ws://127.0.0.1:9222/devtools/browser/<id>`
   evita subir outro Chrome e captura exatamente o que você está vendo.
7. **Não use `--no-rect/--no-metrics` no pipeline de regressão** — `rect`/`is_visible`
   são parte valiosa do sinal de CLS/visibilidade.

## Referência rápida

| Ação | Comando |
|---|---|
| Capturar | `sniff-computed-style -u URL -s SEL [flags]` |
| Achatado | `--output jsonl-flat` |
| Resumo de mudanças | `sniff-diff base.jsonl head.jsonl --stats-only` |
| Delta completo | `sniff-diff base.jsonl head.jsonl > delta.jsonl` |
| Schema da resposta IA | `docs/sniff-eval.schema.json` |
| Prompt de avaliação | `docs/eval-prompt.md` |
