# Changelog

Todos os lançamentos seguem [Semantic Versioning](https://semver.org/) e cada
versão publicada recebe uma tag `vX.Y.Z` no GitHub. Os binários de cada
arquitetura, o instalador e a imagem Docker são publicados a partir da mesma tag.

## [Unreleased]

### Added

- **Headers HTTP por sessão (`Network.setExtraHTTPHeaders`)** — `SniffConfig.headers`,
  CLI `--header "Name: Value"` (repetível), MCP `headers` (`{"X-CMS-AI-Token": "<token>"}`)
  e env `SNIFF_DEFAULT_HEADERS` (JSON). Aplicados a **todo** request antes da
  navegação, permitindo autenticar áreas restritas (middleware stateless de CMS)
  sem token em URL, `.env` ou proxy.
- **Ação `upload` (`DOM.setFileInputFiles`)** — novo `Action::Upload`: anexa
  arquivos locais a um `<input type=file>` (inclusive visualmente ocultos) e o
  browser dispara `change` sozinho, então handlers reais de upload (ex. o
  cropper de imagem de um CMS) rodam. CLI `--upload sel:file1,file2` /
  `--action upload:<sel>:<file1,file2>`, MCP `{"type":"upload","selector":...,"files":[...]}`.
  O `prepare` relaxa a visibilidade para upload (file inputs costumam estar com
  `display:none`).
- **Estado de sessão persistente (storage state)** — `SniffConfig.storage_state_path`
  / `save_storage_state`, CLI `--storage-state` / `--save-storage-state`, MCP
  `storage_state` / `save_storage_state` e env `SNIFF_STORAGE_STATE`. Formato
  Playwright-compatível (`cookies` + `origins[].localStorage`): cookies
  restaurados via `Network.setCookies` e `localStorage` via
  `Page.addScriptToEvaluateOnNewDocument` (roda antes dos scripts da página) —
  tudo **antes** da navegação. `--save-storage-state` exporta cookies +
  `localStorage` da origem atual ao fim do pipeline, então um login feito por
  `actions` sobrevive a restarts do browser/servidor.
- **Defaults configuráveis no MCP** — `ServerDefaults` lidos do ambiente uma vez:
  `SNIFF_DEFAULT_HEADERS` (JSON), `SNIFF_STORAGE_STATE` (path) e `SNIFF_BASE_URL`
  (prefixo para URLs relativas). O agente não precisa repetir auth/session por
  chamada; valores explícitos por chamada sobrescrevem os defaults.

## [0.3.0] — 2026-08-12

### Added

- **Screenshot de primeira classe** — captura a página como PNG
  (`Page.captureScreenshot`) no estado final do pipeline (pós-stabilize,
  pós-interações): CLI `--screenshot PATH` (+ `--fullpage-screenshot` para o
  documento inteiro em vez da viewport) e MCP `screenshot:true` /
  `screenshot_full_page:true`, que persiste o PNG junto do snapshot como
  `[UTC]-[path]-[selector].png` e devolve `screenshot_path` no `__sniff`.
  Complementa o snapshot calculado com o "como a página realmente parece".
- **`--summary` / MCP `return:"summary"`** — novo `OutputFormat::Summary`:
  digest token-lean de 1 linha por nó com só `{tag, selector, path, depth,
  rect, visible}` (sem o payload de estilos). Elimina a necessidade de
  ferramentas externas (python/jq) para reduzir a volumetria antes de mandar
  ao modelo; o JSONL completo continua persistido para diff/check por path.
- **Atributos DOM opt-in** — `--attrs name,value` no CLI (repetível ou
  comma-separated) / `attributes:["name"]` no MCP capturam `getAttribute` de
  cada atributo por nó, emitidos sob `attrs` (ex.:
  `"attrs":{"name":"parameters[items][0][title]"}`). Valida reindexação de
  forms sem `curl`/scraping do HTML. O diff passa a comparar `attrs` por chave.
- **MCP: filtros de visibilidade e geometria** — `sniffCSS_page` ganhou
  `include_invisible` (equivalente ao CLI `--no-visible`), `exclude`,
  `min_width` e `min_height`, fechando a lacuna de paridade com o CLI que
  obrigava a sair do MCP para capturar conteúdo oculto por animação (WOW.js).
- **Browser lançado com `--remote-allow-origins=*`** — o Chromium próprio do
  toolset agora aceita clientes DevTools de qualquer origem, então ferramentas
  externas (websocket-client, Playwright/Puppeteer anexando via CDP) conseguem
  usar o mesmo browser de `--connect` e do companion GUI do docker.
- **Persistência no CLI (`--persist`) e pasta auto-ignorada pelo git** — o
  `sniffCSS` agora aceita `--persist`, que grava o snapshot no mesmo layout do
  store MCP (`sniffCSS/[domain]/[UTC]-[path]-[selector].<ext>` no CWD ou em
  `SNIFF_SNAPSHOT_DIR`, no formato de `--output` selecionado). A lógica de
  nomeação/raiz foi unificada em `sniff_core::snapshot`, compartilhada entre
  CLI e MCP. Tanto o store do MCP quanto o `--persist` criam um `.gitignore`
  com `*` na raiz `sniffCSS/`, então a árvore gerada **nunca é rastreada pelo
  git** — a pasta de snapshots fica fora do versionamento por padrão.
- **Summary virou o digest intermediário (`--summary`/`--output slim`)** — em
  vez de só o esqueleto estrutural (`tag/selector/path/depth/rect/visible`),
  cada linha agora carrega os facetos que respondem perguntas reais: `css`
  (subconjunto curado: display, position, width/height, cores, font-size/weight,
  overflow, z-index), `contrast` (`ratio`/`aa`/`aaa`) e `aria`
  (`role`/`name`/`focusable`) + `grade`. Constantes globais saem numa linha
  `__meta.style_defaults` inicial. ~5-10x menor que o full, mas responde
  "qual a cor/fonte/contraste/role?" sem o JSONL completo.
- **Summary é o formato padrão (`--summary`) e `--no-summary` extrai o full** —
  `sniffCSS` agora emite o digest summary por padrão (`--output summary`);
  `--no-summary` (ou `--output jsonl`) retorna o snapshot completo
  não-sumarizado. No MCP, `sniffCSS_page` responde o summary por padrão
  (`return:"summary"`); `return:"reference"` dá só o handle `__sniff` e
  `return:"jsonl"` o JSONL completo inline. O CLI é documentado como a
  interface preferida (o MCP é o wrapper para clientes que exigem tools).
- **Dedup de props constantes no compact (`__meta.style_defaults`)** — props
  com o mesmo valor em todos os nós capturados são hoisted uma única vez para
  o `__meta` e omitidas dos `styles` de cada nó. Medido no site real: **~50-80%
  menos JSONL (80% dos bytes de estilos)**. `sniffCSS-diff`/`sniffCSS-check`
  (e MCP `sniffCSS_diff`/`sniffCSS_check`) mesclam os defaults de volta, então
  mudanças de página inteira (ex.: `font-family`) continuam sendo detectadas;
  o `computed_style_hash` sempre cobre os estilos efetivos completos.

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
