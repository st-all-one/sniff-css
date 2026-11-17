# 7. Validação, riscos e rollout

## 7.1 O problema real não é detectar — é acusar errado

Um check de oportunidade que dispara em código correto destrói a confiança em
todo o pipeline. Por isso a regra mais importante deste plano é a do
**triplet de teste**: todo check só entra se houver

1. **fixture positiva** — o padrão legado está presente, o check dispara;
2. **fixture de negação** — o padrão moderno está presente, o check silencia;
3. **fixture `keepWhen`** — o caso legítimo em que o sinal existe mas a
   recomendação não se aplica, o check silencia.

É o mesmo rigor já aplicado em `docs/diff-checks.md` §"Qualidade e
não-regressão" para `backdrop-over-modal`, `line-height: normal`,
`font-size:0`, etc. A novidade é apenas que o `keepWhen` passa a ser
**explícito e obrigatório** para oportunidades.

Exemplo de `keepWhen` que deve ser escrito antes do código:

| Check | Gatilho | `keepWhen` (não disparar se…) |
|---|---|---|
| `hand-rolled-modal` | scrim + `role=dialog` + `position:fixed` | já existe `<dialog popover>` no path; é um toast; é loading overlay sem interação |
| `custom-checkbox` | `appearance:none` em checkbox | o `appearance` veio de UA stylesheet; a folha do autor está bloqueada (CORS) |
| `px-only-typography` | `font-size` em px em texto | é ícone/ícone-fonte; é `font-size:0` (ocultação); o elemento é SVG |
| `root-font-size-override` | `font-size` no `:root` | `__declared` ausente (não é possível afirmar origem) |
| `dark-mode-ignored` | cores idênticas nos dois estados | `__env` ausente; a página não tem `__document.color-scheme` e usa apenas imagem de fundo |
| `image-without-lazy` | imagem abaixo da dobra sem `loading=lazy` | é LCP/hero; é o único `fetchpriority=high`; `__meta.viewport` ausente |

**Regra de ouro:** na dúvida, **não emitir linha**. Um check ausente custa uma
oportunidade perdida; um check falso custa a ferramenta inteira.

## 7.2 Estratégia de testes em camadas

| Camada | O que cobre | Onde |
|---|---|---|
| Parser puro | `DocumentFacet`/`EnvFacet`/`DeclaredRule`/matriz a partir de `Value` sintético | `sniff-engine/src/{document,env,declared}.rs` (`#[cfg(test)]`) |
| Unidade de check | triplet positivo/negação/`keepWhen` com `DiffNode` sintético | `sniff-css-check/src/modern/*.rs` |
| Migração | snapshot v1 → v2 em memória; v2 → parser antigo ignora `__*` | teste de integração em `sniff-css/tests/` |
| Golden v2 | `capture` sem flags emite `__meta.schema=2` + contexto; `--rules` equivalente salvo `evidence` | `sniff-engine/tests/golden.rs` + `sniff-css/tests/api_cli.rs` |
| Integração Chrome-gated | emulação, CSSOM, shadow DOM, folha cross-origin; auto-skip sem Chromium | `sniff-engine/tests/integration.rs` |
| Miri / fuzz | novos parsers de JSONL e CDP | `sniff-core/tests/miri_smoke.rs`, `fuzz/` |
| Benchmark | custo de `attrs` preset, `__declared`, `check --modern` | `crates/*/benches/` + `docs/benchmarks.md` |

**Gate por fase:** `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, `cargo deny check`. Gates adicionais antes do
release: Miri, `cargo audit`, `cargo machete`, `cargo bench`.

## 7.3 Corpus de validação

1. **Sintético controlado:** uma página fixture por família de check, com
   versão "antes" (padrão legado) e "depois" (padrão moderno). É a base dos
   testes unitários e do cálculo de falsos positivos.
2. **Páginas reais já usadas no projeto:** portais de câmara/governo citados
   em `docs/accessibility.md`, que historicamente produziram falsos positivos
   reais (texto transparente, scrim normalizado, SVG irmão).
3. **Páginas com framework:** pelo menos um bundle React/Vue com utility CSS,
   para validar que `__declared` e `attrs` não confundem estilo gerado.
4. **Matriz de estados:** as mesmas páginas em 360/768/1366px e com
   `prefers-color-scheme: light|dark`, `prefers-reduced-motion: reduce`.

Métrica de aceite antes do release v0.6: **zero falsos positivos** nos itens 1–3
e zero divergência de determinismo no item 4. Como o contexto e as
oportunidades já vêm ligados por default, não há estágio de "ativar depois":
o corpus é gate de entrada, não de rollout.

## 7.4 Determinismo

- `check` é função pura de `(JSONL, flags)`. Nenhuma chamada de rede, relógio
  ou ambiente.
- `__env` e `__declared` normalizam para conjuntos fechados e ordenam chaves;
  nunca serializam objeto cru do browser.
- Matriz ordenada por config, com `index`; nunca por conteúdo ou hash.
- Novos mapas usam `BTreeMap`/`BTreeSet` (política já consolidada em
  `docs/phase-3.md` A1).
- `--stable-key` continua sendo a âncora quando a matriz repete a captura.

**Teste de determinismo:** rodar a mesma captura 3×, concatenar, rodar
`check --modern`; as três saídas devem ser iguais bytes por bytes.

## 7.5 Registro de riscos

| Risco | Prob. | Impacto | Mitigação |
|---|---|---|---|
| Quebra de contrato v2 sem migração de leitura | baixa | alto | loader aceita v1; `08-contrato-v2.md` §8.5; teste de diff v1↔v2 |
| `__declared` caro em página com milhares de regras | média | médio | `--declared-limit`, cap por seletor, desligado por default |
| CSSOM cross-origin gera `SecurityError` | alta | baixo | `try/catch` por folha + `blocked_sheets`; check não emite linha se a origem importa |
| Emulação de reduced motion conflita com estabilização | alta | médio | precedência documentada em `03-facets.md` §F-C + teste |
| Falso positivo em modal/toast/loading | alta | alto | `keepWhen` explícito, triplet de teste, shadow mode |
| Matriz N× estoura tempo de CI | média | médio | N=1 default, aviso no envelope, matriz só em job dedicado |
| `__env` expõe informação sensível do usuário | baixa | médio | conjuntos fechados e normalizados; nada de user-agent cru |
| Documentação duplicada / `SKILL.md` fora do limite | média | médio | flags só em `usage.md`/`cli-surface.md`; `SKILL.md` como índice; copiar para a skill do OpenCode |
| `prime` muda sem bump de versão | baixa | alto | tocar `prime` somente em F5 com bump |
| Checks de performance interpretados como CWV medido | média | médio | `evidence` sempre prefixada com "proxy" |

## 7.6 Rollout do v0.6 (qualidade antes de versão)

1. **Implementação com gate por fase (F0–F4):** cada fase termina com o
   triplet de teste e o gate de determinismo. Nada é mesclado sem os testes
   negativos do `keepWhen`.
2. **Shadow interno (antes do tag):** rodar `check` sobre o corpus de
   validação completo e revisar **toda** linha `class:"opportunity"`; ajustar
   gatilhos até zero falso positivo.
3. **Release v0.6:** contexto e oportunidades já no default; `--minimal` e
   `--rules` são as rotas de escape. `prime` regenerado no mesmo bump.
4. **Promoção eventual a `fail`:** só para checks com critério normativo e
   depois de pelo menos uma versão estável em `warn`. Nunca promover
   oportunidade de estilo a `fail`.

**Mecanismo de supressão:** `--ignore <CHECK>` (por execução) e, se necessário,
uma entrada no bloco gravado por `sniff init` no `AGENTS.md` do projeto-alvo —
**não** um novo arquivo de configuração, para não criar segunda fonte de
verdade.

## 7.7 Métricas de acompanhamento

| Métrica | Como medir | Meta |
|---|---|---|
| Cobertura do corpus | guias D+F+N+E / 210 | 100% |
| Precisão em corpus | linhas corretas / linhas emitidas | 100% em fixtures, ≥ 95% em páginas reais |
| Skip rate | checks que não emitiram por facet ausente (v1 migrado) | monitorar; queda indica base atualizada |
| Custo de captura | tempo e bytes default vs. `--minimal` | < 10% de overhead |
| Custo de check | `criterion` antes/depois | < 5% de regressão |
| Estabilidade | 3 execuções idênticas | 100% byte-idêntico |
| Migração | diff v1↔v2 e leitura v1 sem erro | 100% dos casos do corpus |
