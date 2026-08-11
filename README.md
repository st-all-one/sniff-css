# sniff-computed-style

Utilitário em Rust de alta performance para capturar o **computed style real** de elementos de uma página durante o desenvolvimento — em especial no localhost — com saída estruturada, compacta e otimizada para consumo por IA.

Fala diretamente com o navegador via **Chrome DevTools Protocol (CDP) raw sobre WebSocket** (sem dependência de frameworks de automação), o que garante máxima performance e flexibilidade.

## O que você pode fazer

- **Capturar o estado real** de qualquer elemento (computed styles, rect, métricas) com recursão controlada e waits combináveis.
- **Diffar duas versões** (`sniff-diff`): só o que mudou, sem IA.
- **Auditar acessibilidade** (`--contrast --ax --ax-tree`): contraste WCAG **medido** (fundo efetivo resolvido), role/nome/focusable, árvore AX do Chrome e grade de perceptibilidade `is_user_noticeable`.
- **Descobrir problemas** (`sniff-check`): contraste AA, alvos ≥24px, indicador de foco, cards fora do padrão.
- **Servir tudo como MCP** (`sniff-mcp`) para agentes de IA.

## Documentação

| Doc | Conteúdo |
|---|---|
| [`docs/usage.md`](docs/usage.md) | Referência completa da CLI: opções, categorias, waits, formatos de saída, `--compact`. |
| [`docs/ai-usage.md`](docs/ai-usage.md) | Guia otimizado para IA: flag-set recomendado, pipeline captura→diff→checks→avaliação, cenários MCP/CI. |
| [`docs/accessibility.md`](docs/accessibility.md) | Auditoria de acessibilidade: facetas, `is_user_noticeable`, workflow validado em páginas reais. |
| [`docs/diff-checks.md`](docs/diff-checks.md) | `sniff-diff` (diff determinístico) e `sniff-check` (regras PASS/WARN/FAIL). |
| [`docs/golden-run.md`](docs/golden-run.md) | Padrão ouro de execução (contrato de determinismo). |
| [`docs/architecture.md`](docs/architecture.md) | Arquitetura interna dos crates. |

## Build e instalação

```bash
cargo build --release
# binários: target/release/sniff-computed-style, sniff-diff, sniff-check, sniff-mcp
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
sniff-computed-style --url http://localhost:3000 --selector ".btn-primary"

# Subárvore com 1 nível, só box-model + tipografia
sniff-computed-style --url http://localhost:3000 --selector ".card" \
  --depth 1 --categories box-model,typography

# Auditoria de acessibilidade de uma seção (contraste medido + AX tree)
sniff-computed-style --url "$URL" --selector "main" --depth 5 \
  --compact --contrast --ax-tree | sniff-check --rules -
```

Veja [`docs/usage.md`](docs/usage.md) para a referência completa de flags.

## Binários

| Binário | Papel |
|---|---|
| `sniff-computed-style` | Captura o estado real dos elementos → JSONL. |
| `sniff-diff` | Diff determinístico entre dois snapshots → delta mínimo para a IA. |
| `sniff-check` | Checks determinísticos offline: uniformidade + regras (contraste, alvo, foco, alt). |
| `sniff-mcp` | Servidor MCP (stdio): `sniff_page`, `diff_snapshots`, `run_checks`, `list_categories`. |

## Integração com IA

Qualquer ferramenta pode chamar o binário e consumir o stdout:

```bash
sniff-computed-style --url http://localhost:3000 --selector ".btn-primary" \
  | jq '.styles.box_model.width'
```

Para agentes, exponha `sniff-mcp` como servidor MCP. O fluxo recomendado é
**captura determinística → diff determinístico → checks determinísticos → IA
(só interpreta o delta)**; o passo a passo está em [`docs/ai-usage.md`](docs/ai-usage.md).

## Estrutura do projeto

```
crates/
├── sniff-core/     # tipos, config, catálogo de ~250 propriedades CSS, contraste WCAG
├── sniff-cdp/      # cliente CDP raw (WebSocket), protocolo, gestão do processo Chrome
├── sniff-engine/   # orquestração: espera, filtro, extração (1 chamada Runtime.evaluate), AX via CDP, saída
├── sniff-cli/      # binário clap (sniff-computed-style)
├── sniff-diff/     # diff determinístico JSONL (binário sniff-diff) + delta p/ IA
├── sniff-check/    # checks determinísticos (binário sniff-check): uniformidade + regras
└── sniff-mcp/      # servidor MCP (stdio): sniff_page, diff_snapshots, run_checks, list_categories
```

## Testes

```bash
cargo test                    # unit tests (sem Chrome)
cargo test -p sniff-engine --test integration   # e2e com Chrome real (auto-skip se ausente)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## Licença

MIT
