# Padrão ouro de execução (golden run)

Receita determinística que esta ferramenta foi otimizada para executar.
O objetivo: **capturar sempre o mesmo estado**, **diffar só o que mudou de
verdade** e **deixar a IA julgar o mínimo possível de tokens**.

```
captura determinística ──► diff determinístico ──► checks determinísticos ──► IA (só interpreta)
  sniffCSS       sniffCSS-diff              sniffCSS-check / sniffCSS_check      eval-prompt
```

Todas as etapas antes da IA são **zero tokens de LLM** e reproduzíveis byte a
byte (mesmo hash entre runs).

## 1. Contrato de determinismo

Para que duas runs sejam comparáveis, os seguintes parâmetros devem ser
**idênticos** nas duas:

| Parâmetro | Padrão ouro | Por quê |
|---|---|---|
| `--viewport` | `1366x768` | Media queries e `vh/%/rem` mudam com o viewport. |
| default otimizado | `compact`+`custom-props`+`stabilize`+`contrast`+`ax` (todos já ON) | Mesmo modo = mesmo hash/conteúdo. Não misture default com `--full`. |
| `--stable-key data-testid` | sempre que houver | Seletores estáveis entre deploys. |
| Wait strategy | mesmo em ambas | `network-idle`/`element-ready`/`delay` devem esperar o mesmo estado. |
| Ações (`--click`/`--hover`/`--type`/`--upload`/`--action`, MCP `actions`) | mesmas em ambas | A interação define o estado revelado; runs com e sem ação capturam coisas diferentes. |
| Auth/sessão | mesmos em ambas | `--header`/`SNIFF_DEFAULT_HEADERS` e `--storage-state` definem **quem** é o request; lados com credenciais diferentes capturam estados de usuário diferentes. |

> **Áreas restritas:** para um CMS com middleware stateless de IA, o token vai
> por header (`--header "X-CMS-AI-Token: <token>"` ou `SNIFF_DEFAULT_HEADERS`
> no MCP) — o request já nasce autenticado, sem token em URL. Um login por
> formulário sobrevive entre capturas via `--save-storage-state` (exporta
> cookies + `localStorage`) → `--storage-state` (restaura antes da navegação).

> `__actions` (o mapa de efeito de UI) é **aditiva**: ligada/desligada num lado
> só não quebra o diff de nós — só deixa de comparar `__actions` se um dos
> lados não o tiver.

## 2. Pipeline completo (CLI)

```bash
# 1. Baseline (antes da mudança) — default otimizado já inclui
#    compact + custom-props + stabilize + contrast + ax
sniffCSS \
  --url "$URL" --selector "$SEL" \
  --depth 2 --stable-key data-testid \
  > snapshots/base.jsonl

# 2. Após a mudança/deploy (mesmos flags)
sniffCSS \
  --url "$URL" --selector "$SEL" \
  --depth 2 --stable-key data-testid \
  > snapshots/head.jsonl

# 3. Diff determinístico
sniffCSS-diff snapshots/base.jsonl snapshots/head.jsonl \
  --tolerance 0.5 \
  --ignore-props transform,translate,opacity \
  --no-structural \
  > delta.jsonl
#   ^--no-structural: listas de contagem variável não poluem o delta.
#   ^--ignore-props:  props animadas não marcam o nó como changed.

# 4. Checks determinísticos (descoberta, sem IA)
sniffCSS-check --input snapshots/head.jsonl --uniform --rules

# 5. Avaliação IA (só agora o LLM vê algo)
#   Envie delta.jsonl + docs/eval-prompt.md; valide a resposta contra
#   docs/sniffCSS-eval.schema.json.
```

## 2b. Pipeline MCP (mesmo contrato, zero JSONL no contexto)

O MCP segue o mesmo pipeline, mas o snapshot fica no disco
(`sniffCSS/[domain]/[UTC]-[path]-[selector].jsonl`) e só a referência trafega:

```text
1. sniffCSS_page  (url, selector, mesmos flags, persist default ON)
   -> {"__sniff": {"path": "localhost_3000/checkout-form-...-Z.jsonl", "nodes": N}}
2. sniffCSS_page  (mesmos params, após a mudança) -> outro __sniff reference
3. sniffCSS_diff  base_path="<path base>"  head_path="<path head>"  tolerance 0.5
   -> só o delta + __diff_summary (o JSONL completo nunca entra no LLM)
4. sniffCSS_check      path="<path head>" uniform rules   -> PASS/WARN/FAIL
5. sniffCSS_snapshots  domain/localhost_3000              -> acha pares base/head
```

## 3. Variações por cenário

| Cenário | Ajuste |
|---|---|
| Acessibilidade a11y | já ON (`contrast` + `ax` por nó); para a subárvore AX completa adicione `--ax-tree`. |
| Página animada (jitter) | já congelada (`--stabilize` default); se o jitter persistir, suba `--tolerance` ou amplie `--ignore-props`. |
| Full-fidelity | `--full` nos dois lados (nunca misturar com o default). |
| Feed/lista com contagem variável | `--no-structural` no diff (só CHANGED). |
| Cards repetidos (grid) | `sniffCSS-check --uniform` acha o card estranho. |
| Conteúdo que some após load | capture a subárvore estável: `--selector footer --depth 2` ou `--wait delay:N`. |
| Elementos revelados por ação (modal/dropdown/menu) | `--click "#open"` (ou `--hover`/`--type`; no MCP, `actions`). O mesmo alvo `display:none` falha com timeout de `element-ready` sem a ação. |
| Conteúdo oculto por animação (WOW.js) | `--no-visible --wait "delay:3000"` (inclui o invisível + espera fixa) ou `--no-stabilize --wait "delay:3000"`; no MCP `include_invisible:true` + `wait:["delay:3000"]`. |
| Volumetria / contexto DOM barato | `--summary` (ou MCP `return:"summary"`) — esqueleto token-lean; o JSONL completo continua persistido para diff/check por path. |
| Prova visual (olho humano) | `--screenshot out.png` (+ `--fullpage-screenshot`); no MCP `screenshot:true` → `screenshot_path` no `__sniff`. |
| Validar reindexação de forms / atributos DOM | `--attrs name,value` (ou MCP `attributes:["name","value"]`) → mapa `attrs` por nó; o diff compara `attrs` por chave. |
| Regressão de UI entre deploys | `sniffCSS-diff` compara os blocos `__actions` quando ambos os lados têm ações → deltas `ACTION_CHANGED` (o modal abriu fora da tela? `onscreen: false`?) + `actions_changed` no resumo. Os arrays compactos (`css_*_values`) são reidratados no diff, então o delta usa nomes de props (`appeared[0].css_after.display`). |
| Interações encadeadas (modal → mini-modal → input) | `--action` ordenado, um passo por seletor; cada passo gera sua entrada em `__actions` (before = estado do passo anterior). |

## 4. Falha de job (CI)

```bash
sniffCSS-diff base.jsonl head.jsonl --stats-only
# nodes: 14 -> 14 | changed: 1 | added: 0 | removed: 0
# falha se changed/added/removed > limiar; reexecuta sniffCSS-check para evidência.
```

## 5. Validação da resposta IA

```bash
jq -e 'has("status") and has("score_change") and (.changes_evaluated|length)>0' resposta.json
```

---

Aceite apenas evidência **medida** no `reason` da IA: `contrast.ratio`,
`aria.role/name`, desvios de `sniffCSS-check --uniform`, deltas de `ax`. Se a IA
citar um número que a ferramenta não emitiu, peça o delta/facet correto.
