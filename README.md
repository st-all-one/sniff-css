# sniffCSS

Utilitário em Rust de alta performance para capturar o **computed style real** de elementos de uma página durante o desenvolvimento — em especial no localhost — com saída estruturada, compacta e otimizada para consumo por IA.

Fala diretamente com o navegador via **Chrome DevTools Protocol (CDP) raw sobre WebSocket** (sem dependência de frameworks de automação), o que garante máxima performance e flexibilidade.

## O que você pode fazer

- **Capturar o estado real** de qualquer elemento (computed styles, rect, métricas) com recursão controlada e waits combináveis.
- **Diffar duas versões** (`sniffCSS-diff`): só o que mudou, sem IA.
- **Auditar acessibilidade** (`--contrast --ax --ax-tree`): contraste WCAG **medido** (fundo efetivo resolvido), role/nome/focusable, árvore AX do Chrome e grade de perceptibilidade `is_user_noticeable`.
- **Descobrir problemas** (`sniffCSS-check`): contraste AA, alvos ≥24px, indicador de foco, cards fora do padrão.
- **Servir tudo como MCP** (`sniffCSS-mcp`) para agentes de IA.

## Documentação

| Doc | Conteúdo |
|---|---|
| [`docs/usage.md`](docs/usage.md) | Referência completa da CLI: opções, categorias, waits, formatos de saída, `--compact`. |
| [`docs/ai-usage.md`](docs/ai-usage.md) | Guia otimizado para IA: flag-set recomendado, pipeline captura→diff→checks→avaliação, cenários MCP/CI. |
| [`docs/accessibility.md`](docs/accessibility.md) | Auditoria de acessibilidade: facetas, `is_user_noticeable`, workflow validado em páginas reais. |
| [`docs/diff-checks.md`](docs/diff-checks.md) | `sniffCSS-diff` (diff determinístico) e `sniffCSS-check` (regras PASS/WARN/FAIL). |
| [`docs/golden-run.md`](docs/golden-run.md) | Padrão ouro de execução (contrato de determinismo). |
| [`docs/architecture.md`](docs/architecture.md) | Arquitetura interna dos crates. |

## Build e instalação

```bash
cargo build --release
# binários: target/release/sniffCSS, sniffCSS-diff, sniffCSS-check, sniffCSS-mcp
```

Instalar no computador (fica disponível em `~/.local/bin`, com PATH automático):

```bash
scripts/install.sh          # compila e instala
scripts/install.sh --no-build   # re-instala sem recompilar
```

Requisito: Chrome/Chromium disponível (ou defina `SNIFF_CHROME_PATH` / use `--chrome`).

## Uso rápido

```bash
# Computed styles de um botão
sniffCSS --url http://localhost:3000 --selector ".btn-primary"

# Subárvore com 1 nível, só box-model + tipografia
sniffCSS --url http://localhost:3000 --selector ".card" \
  --depth 1 --categories box-model,typography

# Auditoria de acessibilidade de uma seção (contraste medido + AX tree)
sniffCSS --url "$URL" --selector "main" --depth 5 \
  --compact --contrast --ax-tree | sniffCSS-check --rules -
```

Veja [`docs/usage.md`](docs/usage.md) para a referência completa de flags.

## Binários

| Binário | Papel |
|---|---|
| `sniffCSS` | Captura o estado real dos elementos → JSONL. |
| `sniffCSS-diff` | Diff determinístico entre dois snapshots → delta mínimo para a IA. |
| `sniffCSS-check` | Checks determinísticos offline: uniformidade + regras (contraste, alvo, foco, alt). |
| `sniffCSS-mcp` | Servidor MCP (stdio): `sniffCSS_page`, `sniffCSS_diff`, `sniffCSS_check`, `sniffCSS_snapshots`, `sniffCSS_categories`. |

## Integração com IA

Qualquer ferramenta pode chamar o binário e consumir o stdout:

```bash
sniffCSS --url http://localhost:3000 --selector ".btn-primary" \
  | jq '.styles.box_model.width'
```

Para agentes, exponha `sniffCSS-mcp` como servidor MCP. O fluxo recomendado é
**captura determinística → diff determinístico → checks determinísticos → IA
(só interpreta o delta)**; o passo a passo está em [`docs/ai-usage.md`](docs/ai-usage.md).

Por padrão o MCP persiste cada captura em
`sniffCSS/[domain]/[path]-[selector]-[UTC].jsonl` (raiz via `SNIFF_SNAPSHOT_DIR`)
e o `sniffCSS_page` retorna só um `__sniff` reference; `sniffCSS_diff`/`sniffCSS_check`
leem por `base_path`/`head_path`/`path` — o JSONL completo nunca entra no contexto
do LLM.

## Estrutura do projeto

```
crates/
├── sniff-core/     # tipos, config, catálogo de ~250 propriedades CSS, contraste WCAG
├── sniff-cdp/      # cliente CDP raw (WebSocket), protocolo, gestão do processo Chrome
├── sniff-engine/   # orquestração: espera, filtro, extração (1 chamada Runtime.evaluate), AX via CDP, saída
├── sniff-css/         # binário clap (sniffCSS)
├── sniff-css-diff/    # diff determinístico JSONL (binário sniffCSS-diff) + delta p/ IA
├── sniff-css-check/   # checks determinísticos (binário sniffCSS-check): uniformidade + regras
└── sniff-css-mcp/     # servidor MCP (stdio): sniffCSS_page, sniffCSS_diff, sniffCSS_check, sniffCSS_snapshots, sniffCSS_categories
```

## Testes

```bash
cargo test                    # unit tests (sem Chrome)
cargo test -p sniff-engine --test integration   # e2e com Chrome real (auto-skip se ausente)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## Licença

CC0
