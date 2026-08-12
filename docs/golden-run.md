# Padrão ouro de execução (golden run)

Receita determinística que esta ferramenta foi otimizada para executar.
O objetivo: **capturar sempre o mesmo estado**, **diffar só o que mudou de
verdade** e **deixar a IA julgar o mínimo possível de tokens**.

```
captura determinística ──► diff determinístico ──► checks determinísticos ──► IA (só interpreta)
  sniff-computed-style       sniff-diff              sniff-check / run_checks      eval-prompt
```

Todas as etapas antes da IA são **zero tokens de LLM** e reproduzíveis byte a
byte (mesmo hash entre runs).

## 1. Contrato de determinismo

Para que duas runs sejam comparáveis, os seguintes parâmetros devem ser
**idênticos** nas duas:

| Parâmetro | Padrão ouro | Por quê |
|---|---|---|
| `--viewport` | `1366x768` | Media queries e `vh/%/rem` mudam com o viewport. |
| `--categories all` + `--compact` | sempre | Mesmo modo = mesmo hash/conteúdo. |
| `--stable-key data-testid` | sempre que houver | Seletores estáveis entre deploys. |
| `--stabilize` | páginas animadas/carrosséis | Congela `animation`/`transition` (`prefers-reduced-motion` + cancelamento). |
| Wait strategy | mesmo em ambas | `network-idle`/`element-ready`/`delay` devem esperar o mesmo estado. |

## 2. Pipeline completo (CLI)

```bash
# 1. Baseline (antes da mudança)
sniff-computed-style \
  --url "$URL" --selector "$SEL" \
  --depth 2 --categories all --compact --custom-props \
  --stable-key data-testid --stabilize \
  > snapshots/base.jsonl

# 2. Após a mudança/deploy (mesmos flags)
sniff-computed-style \
  --url "$URL" --selector "$SEL" \
  --depth 2 --categories all --compact --custom-props \
  --stable-key data-testid --stabilize \
  > snapshots/head.jsonl

# 3. Diff determinístico
sniff-diff snapshots/base.jsonl snapshots/head.jsonl \
  --tolerance 0.5 \
  --ignore-props transform,translate,opacity \
  --no-structural \
  > delta.jsonl
#   ^--no-structural: listas de contagem variável não poluem o delta.
#   ^--ignore-props:  props animadas não marcam o nó como changed.

# 4. Checks determinísticos (descoberta, sem IA)
sniff-check --input snapshots/head.jsonl --uniform --rules

# 5. Avaliação IA (só agora o LLM vê algo)
#   Envie delta.jsonl + docs/eval-prompt.md; valide a resposta contra
#   docs/sniff-eval.schema.json.
```

## 2b. Pipeline MCP (mesmo contrato, zero JSONL no contexto)

O MCP segue o mesmo pipeline, mas o snapshot fica no disco
(`sniff-css/[domain]/[path]-[selector]-[UTC].jsonl`) e só a referência trafega:

```text
1. sniff_page  (url, selector, mesmos flags, persist default ON)
   -> {"__sniff": {"path": "localhost_3000/checkout-form-...-Z.jsonl", "nodes": N}}
2. sniff_page  (mesmos params, após a mudança) -> outro __sniff reference
3. diff_snapshots  base_path="<path base>"  head_path="<path head>"  tolerance 0.5
   -> só o delta + __diff_summary (o JSONL completo nunca entra no LLM)
4. run_checks      path="<path head>" uniform rules   -> PASS/WARN/FAIL
5. list_snapshots  domain/localhost_3000              -> acha pares base/head
```

## 3. Variações por cenário

| Cenário | Ajuste |
|---|---|
| Acessibilidade a11y | `--contrast` (facet medido por nó) e `--ax` (nó AX do Chrome) ou `--ax-tree` (subárvore completa). |
| Página animada (jitter) | `--stabilize`; se o jitter persistir, suba `--tolerance` ou amplie `--ignore-props`. |
| Feed/lista com contagem variável | `--no-structural` no diff (só CHANGED). |
| Cards repetidos (grid) | `sniff-check --uniform` acha o card estranho. |
| Conteúdo que some após load | capture a subárvore estável: `--selector footer --depth 2` ou `--wait delay:N`. |

## 4. Falha de job (CI)

```bash
sniff-diff base.jsonl head.jsonl --stats-only
# nodes: 14 -> 14 | changed: 1 | added: 0 | removed: 0
# falha se changed/added/removed > limiar; reexecuta sniff-check para evidência.
```

## 5. Validação da resposta IA

```bash
jq -e 'has("status") and has("score_change") and (.changes_evaluated|length)>0' resposta.json
```

---

Aceite apenas evidência **medida** no `reason` da IA: `contrast.ratio`,
`aria.role/name`, desvios de `sniff-check --uniform`, deltas de `ax`. Se a IA
citar um número que a ferramenta não emitiu, peça o delta/facet correto.
