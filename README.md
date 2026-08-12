# SniffCSS

Utilitário em Rust de alta performance para capturar o **computed style real** de elementos de uma página durante o desenvolvimento — em especial no localhost — com saída estruturada, compacta e otimizada para consumo por IA.

Fala diretamente com o navegador via **Chrome DevTools Protocol (CDP) raw sobre WebSocket** (sem dependência de frameworks de automação), o que garante máxima performance e flexibilidade.

## O que você pode fazer

- **Capturar o estado real** de qualquer elemento (computed styles, rect, métricas) com recursão controlada e waits combináveis — o **default já é otimizado para IA** (compact, custom-props, stabilize, contrast e ax ligados).
- **Diffar duas versões** (`sniffCSS-diff`): só o que mudou, sem IA.
- **Auditar acessibilidade**: contraste WCAG **medido** (fundo efetivo resolvido), role/nome/focusable, árvore AX do Chrome e grade de perceptibilidade `is_user_noticeable` — vêm por padrão; `--ax-tree` adiciona a subárvore AX completa.
- **Descobrir problemas** (`sniffCSS-check`): contraste AA, alvos ≥24px, indicador de foco, cards fora do padrão.
- **Servir tudo como MCP** (`sniffCSS-mcp`) para agentes de IA.

## Documentação

| Doc | Conteúdo |
|---|---|
| [`docs/usage.md`](docs/usage.md) | Referência completa da CLI: opções, defaults otimizados, `--full`/`--no-*`, categorias, waits, formatos de saída. |
| [`docs/ai-usage.md`](docs/ai-usage.md) | Guia otimizado para IA: default otimizado, pipeline captura→diff→checks→avaliação, cenários MCP/CI. |
| [`docs/accessibility.md`](docs/accessibility.md) | Auditoria de acessibilidade: facetas, `is_user_noticeable`, workflow validado em páginas reais. |
| [`docs/diff-checks.md`](docs/diff-checks.md) | `sniffCSS-diff` (diff determinístico) e `sniffCSS-check` (regras PASS/WARN/FAIL). |
| [`docs/golden-run.md`](docs/golden-run.md) | Padrão ouro de execução (contrato de determinismo). |
| [`docs/architecture.md`](docs/architecture.md) | Arquitetura interna dos crates. |

## Instalação

Binários pré-compilados para **Linux (glibc + musl), macOS e Windows** são
publicados em cada [GitHub Release](https://github.com/st-all-one/sniff-css/releases)
vinculado a uma tag semver (`v0.1.0`, `v0.2.0`, …). O instalador abaixo é o
jeito mais rápido (estilo rustup) — baixa, verifica checksum e instala em
`~/.local/bin`:

```bash
curl --proto '=https' --tlsv1.2 -sSf \
  https://raw.githubusercontent.com/st-all-one/sniff-css/main/install.sh | sh
```

Por padrão instala o **latest**; para uma versão específica:

```bash
curl --proto '=https' --tlsv1.2 -sSf \
  https://raw.githubusercontent.com/st-all-one/sniff-css/main/install.sh \
  | VERSION=v0.1.0 sh
```

> O instalador é `pipefail`-seguro, verifica o `sha256sums.txt` do Release antes
> de instalar e nunca roda com sudo. Personalize com `INSTALL_DIR` (destino) ou
> `SNIFF_TARGET` (triple).

Compilar do fonte (dev, exige Rust):

```bash
cargo build --release
# binários: target/release/sniffCSS, sniffCSS-diff, sniffCSS-check, sniffCSS-mcp
scripts/install.sh --no-build   # instala os binários compilados em ~/.local/bin
```

Requisito em todos os casos: Chrome/Chromium disponível (ou defina
`SNIFF_CHROME_PATH` / use `--chrome`).

## Docker (self-contained Chromium)

A imagem [`stallonels/sniffcss`](https://hub.docker.com/r/stallonels/sniffcss)
(Docker Hub) é publicada no mesmo release, multi-arch (**linux/amd64 +
linux/arm64**) e contém Chromium + toolchain. O Chromium da GUI
(`http://localhost:3001`) roda com **FullColor 4:4:4** por padrão e expõe CDP
em `127.0.0.1:9222`; `sniffCSS` e `sniffCSS-mcp` anexam a esse mesmo browser
(`SNIFF_CONNECT` já é o default), então o que você vê na tela é exatamente o
que é capturado.

```bash
docker compose -f docker/docker-compose.yml up -d
docker compose -f docker/docker-compose.yml exec sniffcss \
  sniffCSS -u "$URL" -s "$SEL" --stable-key data-testid
docker compose -f docker/docker-compose.yml exec -i sniffcss sniffCSS-mcp   # MCP (stdio)
```

Detalhes e opções (GPU, headless, volume) em [`docs/usage.md`](docs/usage.md#docker).

## Uso rápido

```bash
# Computed styles de um botão — default otimizado para IA
# (compact + custom-props + stabilize + contrast + ax ligados)
sniffCSS --url http://localhost:3000 --selector ".btn-primary"

# Subárvore com 1 nível, só box-model + tipografia
sniffCSS --url http://localhost:3000 --selector ".card" \
  --depth 1 --categories box-model,typography

# Auditoria de acessibilidade de uma seção (contraste medido + AX tree já
# vêm por padrão; --ax-tree adiciona a subárvore AX completa)
sniffCSS --url "$URL" --selector "main" --depth 5 \
  --ax-tree | sniffCSS-check --rules -
```

Controle fino **opcional**: `--full` desliga os 5 otimizadores de uma vez
(full-fidelity); `--no-compact`/`--no-contrast`/`--no-ax`/`--no-stabilize`/
`--no-custom-props` desligam individualmente.

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
O `sniffCSS_page` já captura com os defaults otimizados (compact, contrast, ax, stabilize,
custom-props) e persistindo o snapshot; o JSONL completo nunca entra no contexto do LLM.

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

## Desenvolvimento e releases

- **Testes**: `cargo test` · `cargo clippy --all-targets -- -D warnings` · `cargo fmt --check`
- **Release**: envie uma tag semver → o workflow `.github/workflows/release.yml`
  compila os binários de todas as arquiteturas, publica o GitHub Release e a
  imagem Docker no Docker Hub (requer os secrets `DOCKERHUB_USERNAME`/
  `DOCKERHUB_TOKEN`, configure com `scripts/set-secrets.sh`):

  ```bash
  git tag v0.1.0 && git push origin v0.1.0
  ```

- Registro das mudanças em [`CHANGELOG.md`](CHANGELOG.md).

## Licença

CC0
