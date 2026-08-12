# Changelog

Todos os lançamentos seguem [Semantic Versioning](https://semver.org/) e cada
versão publicada recebe uma tag `vX.Y.Z` no GitHub. Os binários de cada
arquitetura, o instalador e a imagem Docker são publicados a partir da mesma tag.

## [0.2.1] — 2026-08-12

### Changed

- **Nome dos snapshots persistidos pelo MCP** — o padrão passou de
  `[path]-[selector]-[UTC].jsonl` para `[UTC]-[path]-[selector].jsonl`. O
  timestamp UTC agora vem na frente do nome, então as execuções de um mesmo
  alvo ficam **ordenadas cronologicamente** no diretório
  `sniffCSS/[domain]/` (a execução mais recente é o último arquivo, e a busca
  pelo snapshot mais novo de um target fica trivial). A escrita continua
  atômica e `list_snapshots`/diff/check seguem operando por caminho; snapshots
  antigos (sufixo de UTC) deixam de aparecer em `list_snapshots`.

## [0.2.0] — 2026-08-12

### Added

- **Mapa de efeito de UI por interação (`__actions`)** — ao interagir
  (`click`/`hover`/`type`), cada ação passa a mapear **o que** houve a nível UI
  e **onde**, numa linha reservada `__actions` do JSONL (default ON com ações;
  `--no-effects`/`effects:false` omite):
  - `effect`: `revealed` / `hidden` / `changed` / `moved` / **`no_effect`**
    (interação que não mudou nada — possível falha de lógica).
  - `appeared` / `removed` / `changed` com `tag`, `path`, `rect`, `onscreen`,
    `out_of_view.{above,below,left,right}` (px além de cada borda da viewport),
    `distance_from_action` + `direction` (posição relativa ao ponto da ação),
    `css_before`/`css_after` (assinatura curada de ~38 props visuais/layout) e
    `css_changed`.
  - `summary` determinística (ex.: `"1 element(s) appeared · biggest: TABLE
    1430px below — 2146px from click"`).
  - Implementação: `sniff_engine::effects` (captura de página inteira em um
    `Runtime.evaluate` + diff determinístico em Rust), `SniffConfig.effects`/
    `effects_limit`, `--effects`/`--no-effects`/`--effects-limit N`, MCP
    `effects`/`effects_limit`.
- **Regressão de UI no diff** — `sniffCSS-diff`/`sniffCSS_diff` comparam os
  blocos `__actions` quando ambos os lados os carregam: deltas
  `ACTION_CHANGED`/`ACTION_ADDED`/`ACTION_REMOVED` (ex.: `appeared[0].rect.y:
  8 → 900`, `onscreen: true → false`, `effect: revealed → no_effect`) e
  `actions_changed` no `__diff_summary`.
- **Interações encadeadas robustas** — com `--action` ordenado (MCP `actions`),
  cadeias modal → mini-modal → input funcionam passo a passo: cada passo espera
  o próprio alvo (agora exigindo **visível + com tamanho**, não só existência),
  e cada passo gera a própria entrada em `__actions` (before = estado do passo
  anterior). Passo quebrado → erro nomeando o índice, o seletor e os passos
  anteriores (ex.: `action #1 (click:#open-mini) failed ... Prior steps:
  click:#open`). `stable_key` também estabiliza as chaves dos mapas de efeito
  entre deploys.
- Interações reais antes da captura (`click` / `hover` / `type`) para revelar
  elementos que só existem após uma ação (modais, dropdowns, menus de hover,
  sugestões de busca):
  - CLI: `--click SEL[:timeout[:settle]]`, `--hover SEL[:timeout[:settle]]`,
    `--type SEL:text` (repetíveis) e `--action <spec>` (ordenado, para fluxos
    mistos). Cada ação espera o seletor alvo, rola até o centro e dispara um
    evento confiável via `Input.dispatchMouseEvent`/`Input.insertText`; o
    pipeline de waits roda depois das ações contra o DOM pós-interação e o
    `--stabilize` é reaplicado para snapshots determinísticos.
  - MCP: parâmetro `actions` (`[{type, selector, text?, timeout_ms?,
    settle_ms?}]`, ordenado) no `sniffCSS_page`.
  - `SniffConfig.actions`, `Action` e `parse_action` em `sniff-core`; helpers
    `input_click`/`input_hover`/`input_insert_text` em `sniff-cdp`; módulo
    `sniff_engine::action` (split `prepare`/`perform`) e novo
    `Phase::Interacting` (progresso 0.35).
- Distribuição oficial:
  - Workflow de release (`release.yml`): build de binários otimizados para
    Linux (glibc + musl, x86_64 + aarch64), macOS (aarch64 + x86_64) e Windows
    (x86_64), anexados ao GitHub Release junto com `sha256sums.txt`.
  - Instalador `install.sh` estilo `curl | sh` (como o rustup): detecta
    OS/arquitetura, baixa do Release (latest ou `VERSION=vX.Y.Z`), verifica
    checksum SHA-256 e instala em `~/.local/bin`.
  - Imagem Docker publicada no Docker Hub (`stallonels/sniffcss`) multi-arch
    (linux/amd64 + linux/arm64), construída a partir dos binários do Release.
  - `rust-version` corrigido para `1.88` (MSRV real exigida pelo `rmcp`).

## [Unreleased]

(nada ainda)
