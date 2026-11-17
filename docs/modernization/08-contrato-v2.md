# 8. Contrato v2 (minor v0.6) — quebras deliberadas e migração

> Decisão do mantenedor: **é autorizado um upgrade de minor com quebra de
> contrato**, desde que a quebra eleve significativamente a qualidade das
> análises produzidas a partir do `sniff`. Este documento registra quais
> quebras foram escolhidas, o ganho analítico de cada uma e como migrar.
>
Critério aplicado: uma quebra só entra se ela muda **o que o agente consegue
concluir**, não apenas o formato. Mudança cosmética não justifica quebra.

## 8.1 Por que um minor com quebra aqui

O gargalo atual não é precisão das regras existentes — é **falta de contexto**:
o snapshot diz o valor computado de um elemento, mas não diz se a página optou
por escala de texto, qual é a preferência de cor do usuário, quais atributos
de formulário existem nem se a recomendação tem origem normativa. O agente
recebe uma medida órfã e precisa adivinhar o resto.

Adicionar isso como flags opt-in (abordagem conservadora do plano original)
manteria compatibilidade, mas deixaria a maioria das capturas sem o contexto —
e, portanto, sem a melhoria. A escolha é **tornar o contexto parte do default**
e aceitar a quebra de formato.

## 8.2 As três quebras

### B1 — Snapshot v2: contexto ligado por default

**Antes:** `sniff capture` emite computed styles + facets de ARIA/AX/contraste;
`__document`, `__env`, `attrs` e `__meta.schema` não existem ou são opt-in.

**Depois:** toda captura emite, por default:

```
__meta.schema: 2
__document          # meta/title/lang/scripts/links/style_sheets (1 linha)
__env               # preferências normalizadas + emulação (1 linha)
attrs               # preset `modern` (≈20 atributos curtos por nó)
```

`--minimal` restaura o perfil enxuto antigo (sem `__document`/`__env`/`attrs`
de default), para casos de token budget extremo.
`__dom_stats` **não** é emitido no capture: é derivado offline em `check` a
partir da floresta, com custo de uma passagem e sem bytes no snapshot.

**Ganho analítico:** o agente passa a distinguir `html{font-size:16px}` do
default do browser (via `__document` + `__declared` quando pedido), a ver
`autocomplete`/`loading`/`width`/`height`, e a saber se dark mode / reduced
motion / text scale foram optados — sem depender de quem chamou a CLI saber
quais flags ligar. É a maior melhora única de sinal por captura.

**Custo:** mais bytes/tokens por snapshot. O preset `modern` é curado (≈20,
não 31) justamente para conter isso; medir overhead alvo < 10% em modo
`compact`. `__declared` **continua opt-in** (custo in-page).

### B2 — `sniff check` reporta as duas famílias por default

**Antes:** default roda `uniformity` + `rules`; oportunidades exigiriam
`--modern`.

**Depois:** default roda `uniformity` + `rules` + `modern`. Cada linha ganha
`class: "bug" | "opportunity"`. Flags:

| Flag | Efeito |
|---|---|
| *(nenhuma)* | tudo |
| `--rules` | somente quebras (novo significado: antes era "rode regras") |
| `--modern` | somente oportunidades |
| `--no-modern` | desliga oportunidades (equivalente ao antigo default) |

`__check_summary` ganha `modern: N` e `opportunities_by_class`.

**Ganho analítico:** uma única chamada devolve o quadro completo — quebra +
dívida de plataforma. Hoje o chamador precisa saber que a segunda família
existe; na prática, não saberá, e a análise ficará pela metade.

**Custo:** linhas extras por default. Mitigado por `class` (o agente pode
filtrar) e por `--rules` como escape hatch.

### B3 — `evidence` estruturado com `text`

**Antes:**

```jsonc
{"check":"contrast-aa","status":"fail",
 "evidence":"ratio 2.1:1 on #212529 against #020842 (need 4.5:1 text AA)"}
```

**Depois:**

```jsonc
{"check":"contrast-aa","status":"fail","class":"bug","severity":"fail",
 "guide":null,
 "evidence":{
   "text":"ratio 2.1:1 on #212529 against #020842 (need 4.5:1 text AA)",
   "metric":"contrast_ratio",
   "observed":2.1,
   "threshold":4.5,
   "comparator":">=",
   "source":"contrast_facet"
 },
 "metrics":{"font_size_px":16,"text_size":"normal"}}
```

Para checks sem limiar numérico, `observed`/`threshold` são omitidos e só
`text`/`source` existem. `source` diz qual facet sustenta a afirmação —
fundamental para o agente calibrar confiança.

**Ganho analítico:** filtros e agregações determinísticos
(`jq 'select(.evidence.observed < .evidence.threshold)'`), zero parsing de
prosa, e proveniência explícita. A análise deixa de depender de regex sobre
frases em português/inglês.

**Custo:** refatoração mecânica dos ~25 checks existentes para um construtor
`Evidence::new(...)` que gera `text` + campos. Migrar `uniformity` também.
Estimativa: 2–3 dias. Vale a pena porque padroniza **todas** as evidências,
não só as novas.

## 8.3 Bump do `prime`

`sniff prime` é byte-idêntico por versão. Com o minor v0.6 ele passa a
explicar: `class`, `evidence.text`/`metrics`, `__document`/`__env`, o preset de
`attrs`, `--minimal`, `--rules`/`--modern` e o `guide` dos checks. Sem isso,
o agente continuaria interpretando o novo formato com o protocolo antigo —
o que anularia parte do ganho.

## 8.4 O que NÃO muda (mesmo com quebra autorizada)

- **Exit codes** (2/3/5/70) e envelope `--json`
  (`{success,command,data?,error{code,message,retryable},warnings?}`).
- **stdout = dado, stderr = log.**
- **`--params` / `--batch` / stdin** e o formato de operação.
- **Operadores posicionais** (`capture <URL>`, `check <SNAP>`, `diff A B`).
- **Determinismo:** nenhuma quebra depende de relógio, ambiente ou aleatoriedade.
- **`sniff diff` continua offline e com `--tolerance`.** O schema v2 é
  aceito; o v1 é lido via migração em memória.
- **`--no-*` legados** permanecem (deprecados, não removidos neste minor).

## 8.5 Matriz de migração

| Consumidor | Antes | Depois |
|---|---|---|
| `jq` lendo `.evidence` como string | `.evidence \| contains("ratio")` | `.evidence.text \| contains("ratio")` |
| `jq` comparando limiar | regex na string | `.evidence.observed < .evidence.threshold` |
| Script que contava linhas de `check` | todas eram regras | filtrar `.class == "bug"` |
| Script que queria só regras | default já bastava | `sniff check … --rules` |
| Golden de snapshot | bytes exatos da v1 | regenerar baseline v2; `golden-run.md` documenta o bump |
| Diff entre snapshot v1 e v2 | — | loader sobe v1 em memória; resultado equivalente ao v2 |
| Consumidor do `prime` | protocolo v0.5 | regenerar ao atualizar para v0.6 |

## 8.6 Compatibilidade de leitura (migração, não rejeição)

Quebramos a **escrita**, não a **leitura**:

```
snapshot v1 ──► loader ──► upgrade em memória (__meta.schema=2, facets vazias)
snapshot v2 ──► loader ──► uso direto
 difusão de checks ──► facet ausente ⇒ check não emite linha
```

Isso preserva a possibilidade de fazer `sniff diff base-v1.jsonl head-v2.jsonl`
— essencial para quem já persiste snapshots (`--persist`) e quer comparar
antes/depois do upgrade. Snapshots v1 **não** são rejeitados; apenas não
contêm as novas facetas, então os checks que delas dependem silenciam.

## 8.7 Gate de aceite da quebra

A quebra só é considerada justificada se:

1. `sniff capture URL -s body` (sem flags extras) produzir `__document`,
   `__env` e `attrs` utilizáveis em ≥ 95% das páginas do corpus.
2. `sniff check snap.jsonl` (sem flags) emitir pelo menos uma linha
   `class:"opportunity"` com `guide` válido em páginas que contêm padrões
   legados conhecidos — e **nenhuma** em páginas modernas do corpus.
3. `.evidence.observed`/`.threshold` estiverem presentes em todo check
   numérico, e `.evidence.source` em todo check.
4. A migração v1→v2 passar no teste de diff equivalente.
5. Overhead de snapshot < 10% em `compact`; regressão de `check` < 5% no
   benchmark.

Se algum item falhar, a quebra correspondente é revertida para opt-in e o
minor segue sem ela — a autorização é condicional ao ganho real, não
automática.
