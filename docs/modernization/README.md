# Plano de modernização do `sniff` — detecção de oportunidades na página

> **Status:** implementado na v0.6 — todas as famílias de checks e as fatias v2
> (`__document`/`__env`/`__declared`, attrs, matriz 12 estados, pseudo e
> `__actions`) estão ativas por padrão. Este é o documento de decisão; a
> referência de flags continua em [`usage.md`](../usage.md) e
> [`cli-surface.md`](../cli-surface.md).
>
> **Base de conhecimento:** os 210 guias do
> `modern-web-guidance` (instalação local em
> `~/.omp/agent/skills/modern-web-guidance/guides/`), revisados integralmente
> pelos relatórios em [`findings/`](findings/).
>
> **Decisão de versão:** o plano prevê um **minor v0.6 com quebras
> deliberadas de contrato**, autorizadas porque elevam significativamente a
> qualidade das análises. Escopo e migração em
> [`08-contrato-v2.md`](08-contrato-v2.md).

## Tese

O `sniff` hoje responde bem a **"isso está quebrado?"**. Falta responder a
**"isso está lutando contra a plataforma?"**.

O `modern-web-guidance` codifica essa segunda pergunta em 210 guias — mas seu
`legacy-scan` lê **código-fonte** e cobre só ~20% do corpus com regras
determinísticas. O `sniff` captura o **resultado renderizado** (computed styles,
caixa, ARIA, AX, contraste, atributos), o que o torna o complemento natural:
detecta o sintoma mesmo em bundle minificado, CSS de framework, estilo injetado
por JS ou estado pós-interação — onde o scan de fonte não enxerga.

```
                    intenção do autor                 resultado renderizado
                    (código-fonte)                    (computed + DOM + AX)
legacy-scan (MWG)   ──────────────────►     ◄───────────── sniff (proposto)
   ~20% dos guias                            210 guias, filtrados por observabilidade
```

O objetivo não é transformar o `sniff` em linter de código nem em medidor de
Core Web Vitals. É fazer dele o **detector determinístico de dívida de
plataforma na página renderizada**.

## Como navegar

| Doc | Conteúdo |
|---|---|
| [`01-diagnostico.md`](01-diagnostico.md) | O que já existe, onde estão as lacunas estruturais, critérios de sucesso. |
| [`02-arquitetura-alvo.md`](02-arquitetura-alvo.md) | Onde cada mudança vive no workspace, fluxo de dados, compatibilidade, orçamento de performance. |
| [`03-facets.md`](03-facets.md) | Especificação de cada nova facet (JSON, mecanismo CDP/JS, custo, ativação, falha). |
| [`04-catalogo-de-checks.md`](04-catalogo-de-checks.md) | Catálogo consolidado: gatilho exato, severidade, `keepWhen`, evidência, fixture. |
| [`05-inventario-guias.md`](05-inventario-guias.md) | Inventário dos 210 guias com classes D/F/N/E e contagens por categoria. |
| [`06-roadmap.md`](06-roadmap.md) | Fases F0–F5, dependências, esforço, critérios de aceite. |
| [`07-validacao-e-riscos.md`](07-validacao-e-riscos.md) | Controle de falso positivo, determinismo, testes, rollout, registro de riscos. |
| [`08-contrato-v2.md`](08-contrato-v2.md) | Quebras deliberadas do minor v0.6, ganho analítico de cada uma e guia de migração. |
| [`findings/`](findings/) | Relatórios integrais de revisão, um por fatia do corpus (audit trail). |

## Princípios que norteiam o plano

1. **WARN por padrão.** Oportunidade não é defeito. `FAIL` só com critério
   normativo explícito (WCAG/WAI-ARIA).
2. **Determinismo antes de cobertura.** Nada de LLM, nada de heurística
   dependente de tempo/ambiente. Evidência sempre medida no snapshot.
3. **`keepWhen` obrigatório.** Toda regra nasce com a explicitação de quando
   **não** disparar — o equivalente ao par `keepWhen`/`dropWhen` do próprio
   `modern-web-guidance`.
4. **Custo explícito.** Facet por nó tem orçamento; facetas globais são uma
   linha; novas passadas CDP precisam de justificativa e gate de ativação.
5. **Quebra quando há ganho analítico.** O v0.6 quebra formato de escrita onde
   isso dá contexto ao agente (`08-contrato-v2.md`); a **leitura** de v1 é
   mantida por migração em memória. Mudança meramente cosmética não justifica
   quebra.
6. **Não duplicar documentação.** Detalhe de flag só em `usage.md`/
   `cli-surface.md`; este diretório é plano, não referência.

## Resultado esperado (mensurável)

| Métrica | Hoje | Alvo |
|---|---:|---:|
| Guias com mapeamento explícito para o sniff | 0 | 210 (100% inventariados) |
| Guias detectáveis (D) ou com extensão factível (F) | — | ver `05-inventario-guias.md` |
| Checks de oportunidade implementados | 0 | catálogo priorizado F1–F4 |
| Cobertura automatizada de WCAG estrutural (h1/headings/landmarks) | manual | automatizada |
| Regressão de performance no caminho quente | — | < 5% (gate de `criterion`) |
| Falsos positivos em corpus de validação | — | 0 em casos `keepWhen` documentados |
| Contexto disponível sem flags extras | parcial | completo: `__document` + `__env` + `__declared` + attrs + matriz 12 estados |
