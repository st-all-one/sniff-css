# 1. Diagnóstico — onde o `sniff` está e onde faltam olhos

## 1.1 O que já funciona

O pipeline `capture → diff → check` é sólido e, sobretudo, **determinístico e
sem IA no caminho de medição**:

- `sniff capture` extrai, em uma única chamada JS na página
  ([`sniff-engine/src/extractor.rs`](../../crates/sniff-engine/src/extractor.rs)),
  computed styles em 8 categorias (`box_model`, `layout`, `typography`,
  `visual`, `transform`, `animation`, `interaction`, `accessibility`), rect,
  `metrics`, `is_user_noticeable`, `aria`, `contrast`, `ax`, pseudo-elementos e
  atributos verbatim opt-in (`--attrs`, ver
  [`sniff-core/src/config.rs:43`](../../crates/sniff-core/src/config.rs)).
- `sniff diff` compara florestas `DiffNode` com tolerância numérica e ignora
  mudanças voláteis; lê `__meta.viewport` para checagens relativas à viewport.
- `sniff check` concentra as regras em
  [`sniff-css-check/src/rules.rs`](../../crates/sniff-css-check/src/rules.rs)
  (~2.600 linhas, grande parte de testes). São ~25 regras: WCAG (contraste,
  target-size, foco, alt, `aria-hidden-focusable`) e armadilhas de CSS
  (`sticky-in-overflow-hidden`, `fixed-broken-by-transform`, `occluded`,
  `backdrop-over-modal`, `z-index-on-static`, `transition-all`, …), além de
  `uniformity` para o "card estranho" entre irmãos.
- A engine **já emula** `prefers-reduced-motion: reduce` para estabilizar a
  captura ([`sniff-engine/src/sniffer.rs:207`](../../crates/sniff-engine/src/sniffer.rs)),
  e descarta o resultado. É um sinal gratuito esperando para ser medido.
- O contrato de saída é estável: JSONL, `__meta` global, envelope `--json`,
  stdout = dado / stderr = log. Qualquer extensão precisa preservar isso.

**Leitura correta:** o `sniff` é um excelente *detector de quebra*. Ele não é,
hoje, um *detector de dívida de plataforma*.

## 1.2 A lacuna conceitual

O `modern-web-guidance` organiza 210 guias em 17 categorias:

| Categoria | Guias | Categoria | Guias |
|---|---:|---|---:|
| `ui-behaviors` | 40 | `accessibility` | 4 |
| `performance` | 26 | `built-in-ai` | 5 |
| `visual-design` | 22 | `webmcp` | 3 |
| `ui-components` | 22 | `user-experience` | 3 |
| `forms` | 19 | `pwa` | 2 |
| `css` | 18 | `privacy` | 1 |
| `security` | 17 | `wasm` | 1 |
| `html` | 8 | **Total** | **210** |
| `js` | 6 | | |

O `legacy-scan` do próprio projeto detecta padrões legados no **código-fonte**
via AST, mas apenas nos guias que possuem regras determinísticas (~20%).
Duas consequências:

1. **Cego ao runtime.** Bundle minificado, utility classes de framework,
   estilo injetado por JS, estado depois de `--action`, e a composição final
   de cascade não existem para um scan de fonte.
2. **Cego ao resultado.** Muitos guias não perguntam *como* foi escrito, e sim
   *o que o usuário recebe*: um modal sem `top layer`, texto que não escala,
   uma imagem sem dimensões reservadas, um scrim sobre o diálogo. É exatamente
   o que o `sniff` vê.

Por outro lado, **nem todo guia é observável na página renderizada**. WebAuthn,
headers de segurança, CSP, intenção de validação, encryptamento client-side e
APIs de IA não deixam assinatura estável no DOM. Forçar esses casos geraria
falso positivo e destruiria a confiança na ferramenta. O
[`05-inventario-guias.md`](05-inventario-guias.md) registra essa fronteira guia
a guia.

## 1.3 Limitações estruturais (a causa raiz das lacunas)

### L1 — Computed ≠ declarado

`getComputedStyle` devolve o valor **resolvido**, sem origem. Assim:

- `html { font-size: 16px }` (anti-padrão que bloqueia escala de texto do OS) e
  o default do browser produzem o mesmo `font-size: 16px`.
- `outline: none` explícito e um outline suprimido por outro mecanismo são
  indistinguíveis pelo valor final.
- `appearance: none` em `select`/`checkbox` é declarado, mas o valor computado
  não diz se veio do autor ou do UA stylesheet.

**Implicação:** para um subconjunto importante de oportunidades é preciso uma
facet `declared` (CSSOM + origem), não mais computed styles.

### L2 — Atributos existem, mas são ocasionais

`attrs` já é capturado verbatim via `getAttribute`, porém **opt-in** e
limitado aos nomes passados em `--attrs`. Hoje não há conjunto padrão para
`autocomplete`, `loading`, `srcset`, `width`/`height`, `popover`, `inert`,
`inputmode`, `type`, `href`, `role`, `aria-*`. Como boa parte dos guias de forms
e priorização de recursos é dirigida por atributo, essa é a extensão de maior
alavancagem por linha de código.

### L3 — Não há contexto de documento

O snapshot sabe o que cada nó é, mas não sabe:

- se a página optou por `<meta name="text-scale">`, `color-scheme`, viewport;
- se há `<title>` único, `lang` no `<html>`, manifesto linkado;
- se scripts são `defer`/`async`/`module` ou bloqueantes;
- se o user prefere dark mode, reduced motion, contraste aumentado ou
  ponteiro coarse.

Tudo isso mora em `document`/`matchMedia` e custa **uma linha global**, não
um campo por nó.

### L4 — Uma viewport, um estado

`--viewport` existe, mas a captura é single-state. Não dá para responder
"o layout quebra só em 360px?" ou "a página ignora `prefers-color-scheme`?"
sem duas capturas e um diff entre elas. A matriz de estados é uma extensão
natural do que `sniff diff` já faz — só precisa de ordem determinística e
metadados de cada estado.

### L5 — Os globais ainda não chegam ao `check`

`DiffDocument` já carrega `viewport` e `__actions`, mas `__meta.css_variables`
e `style_defaults` são reconstruídos nos nós. Novas facetas globais
(`__document`, `__env`, `__declared`) precisam de um caminho explícito no
modelo de diff e no envelope de check, senão ficarão presas ao JSONL bruto.

### L6 — Fronteira de observação

Algumas coisas simplesmente não devem ser inferidas:

- **Shadow DOM:** confirmado nos findings — o `buildNode` de `extractor.rs`
  percorre só `.children` e **não desce em `el.shadowRoot`**. Abrir a recursão
  no mesmo `Runtime.evaluate` é uma extensão de baixo custo; `mode:'closed'`
  permanece fora de alcance.
- **Canvas/WebGL:** o conteúdo pintado não é DOM computed style.
- **Rede/credenciais/CWV de campo:** exigem `Performance`/`Network` CDP ou
  telemetria real — fora do contrato offline do `sniff check`.

## 1.4 O que NÃO é objetivo

- Virar linter de código-fonte (o `legacy-scan` do MWG já faz isso).
- Medir LCP/INP/CLS de campo ou emitir notas (score) tipo Lighthouse.
- Usar LLM dentro de `check` para decidir se há oportunidade.
- Mudar formato **sem ganho analítico**. As quebras do v0.6 são deliberadas e
  condicionais (ver `08-contrato-v2.md`); cosmética não justifica quebra.
- Duplicar documentação de flags em mais de um arquivo.

## 1.5 Critérios de sucesso

1. **Cobertura inventariada:** 210/210 guias classificados como D/F/N/E, com
   justificativa auditable em `findings/`.
2. **Sinal limpo e completo:** `sniff check` reporta bugs + oportunidades numa
   única passada, com `class` e `evidence` tipado; `--rules` isola quebras.
3. **Zero mistério:** toda oportunidade tem `keepWhen` documentado e teste de
   negação (caso em que **não** deve disparar).
4. **Determinismo preservado:** mesmo snapshot + mesmas flags → mesma saída,
   bytes por bytes; suíte `golden-run` continua verde.
5. **Custo controlado:** nenhuma nova facet por nó sem orçamento medido;
   regressão < 5% nos benches existentes.
6. **Contexto no default:** `__document`, `__env` e `attrs` presentes em toda
   captura; `--minimal` é o escape hatch. Validação em corpus antes do release.
