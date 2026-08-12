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
    efetivo resolvido); `fail` = falha real, `warn` = fundo-imagem.
  - `target-size` — alvo clicável < 24×24px (WCAG 2.2).
  - `focus-indicator` — focusable sem sinal de foco visível.
  - `hidden-focusable` — focusable com `accessibility_grade == NONE` (tab trap).
  - `empty-alt-image` — imagem grande com `alt=""` (não decorativa?).

Saída JSONL com evidência + `__check_summary`:

```jsonc
{"check":"contrast-aa","selector":"footer .text","tag":"P","status":"fail",
 "evidence":"ratio 2.1:1 on #212529 against #020842 (need 4.5:1 text AA)"}
{"check":"uniformity","selector":"div.card:nth-child(3)","status":"fail",
 "evidence":"deviates from the 3/3 group norm: box_model.height: 80px (norm 120px ±40.00)"}
{"__check_summary":{"uniformity_instances":3,"uniformity_outliers":1,"rules":12}}
```

O resultado vira **evidência** para o `reason` da avaliação IA (veja
[`eval-prompt.md`](eval-prompt.md) e [`sniffCSS-eval.schema.json`](sniffCSS-eval.schema.json)).
