# Arquitetura

## Visão geral

```
CLI (sniff-css) → SniffConfig → Sniffer (sniff-engine) → CdpSession (sniff-cdp) → Chrome
                                        │
                                        └─ Runtime.evaluate (1 chamada JS) → snapshots → JSONL
```

O utilitário fala diretamente com o Chrome via Chrome DevTools Protocol. Nenhum
framework de automação é usado: a camada `sniff-cdp` implementa o protocolo
WebSocket do zero, com multiplexação de sessões por `sessionId`.

## Decisões-chave

### 1. Extração em uma única chamada JS

Toda a captura — matching, filtros, `getComputedStyle`, walk recursivo da
árvore, pseudo-elementos, métricas — acontece em **uma** chamada
`Runtime.evaluate`. Isso minimiza round-trips CDP (o gargalo dominante) e
mantém a memória do processo em O(1) relativa à árvore.

O snippet JS é uma função arrow que recebe um objeto de argumentos com:

```json
{
  "selector": ".card",
  "depth": 0,
  "categories": { "box_model": ["width", "height", "..."], "...": [...] },
  "pseudo": ["::before"],
  "filter": { "visible": true, "minWidth": null, "minHeight": null, "excludeSelectors": [] },
  "opts": { "includeRect": true, "includeMetrics": true, "normalizeColors": true }
}
```

As chaves são `camelCase` no JS; o teste `build_args_keys_match_js_reads`
protege contra divergência entre o Rust e o snippet.

### 2. CDP raw

`CdpClient` (sniff-cdp/src/client.rs):

- Conecta via `tokio-tungstenite`.
- Envia comandos com `id` crescente; respostas são roteadas por `id` para
  `oneshot` channels; eventos (sem `id`) vão para um `broadcast`.
- `sessionId` é propagado nos comandos (flatten protocol), permitindo
  multiplexar N páginas numa única conexão.

`CdpSession` (sniff-cdp/src/session.rs):

- `Target.createTarget` → `Target.attachToTarget { flatten: true }`.
- Habilita `Page`, `Runtime`, `DOM`, `Network`.
- `navigate()` aguarda `Page.loadEventFired`.

`BrowserProcess` (sniff-cdp/src/browser.rs):

- Lança `chrome --headless=new --remote-debugging-port=0` e parseia o endpoint
  `ws://` do stderr.
- Usa `--user-data-dir` temporário, removido no `Drop` (com retry).
- `--connect` permite conectar num browser já rodando (dev server com
  remote debugging). Aceita `ws://` direto ou um origin HTTP
  (`http://host:port` / `host:port`) que é resolvido via `GET /json/version`
  (`webSocketDebuggerUrl`), com default da env `SNIFF_CONNECT` — o que permite
  anexar ao Chromium da GUI no container Docker.

### 3. Wait strategies como dados

Em vez de traits/dinamic dispatch, as estratégias de espera são um enum de
configuração (`sniff-core::WaitStrategy`) executado pelo `waiter`:

- `Selector`/`ElementReady`/`AppFlag` → polling JS.
- `NetworkIdle` → contagem de requisições em voo via eventos `Network.*`.
- `FontsLoaded` → `document.fonts.ready` com `awaitPromise: true`.
- `Delay` → sleep.

Isso mantém o motor simples, testável e 100% estático.

### 3b. Ações de interação como dados

Da mesma forma, as interações de usuário são um enum de configuração
(`sniff-core::Action`: `Click`/`Hover`/`Type`/`Upload`) executado por
`sniff_engine::action` via o domínio `Input`/`DOM` do CDP
(`Input.dispatchMouseEvent` para click/hover, `Input.insertText` para type,
`DOM.setFileInputFiles` para upload). Cada ação espera o próprio seletor
aparecer (polling), rola até o centro (`scrollIntoView` + leitura do rect) e
dispara um **evento confiável** — não um `el.click()` sintético, então
`:hover`/`:active`/`pointer`/`mouse`/`click` disparam de verdade. Upload aceita
alvos **ocultos** (file inputs costumam ter `display:none`; o `prepare` relaxa
visibilidade para essa variante) e o próprio Chromium dispara `change`, então
handlers reais (cropper de imagem) rodam.

Quando `config.actions` não está vazio, o `sniffer` pula o wait inicial e roda
as ações em ordem após `navigate`/`stabilize`; o pipeline de waits (`waiter`)
roda **depois**, contra o DOM pós-interação (ex.: o `.modal` que o clique abriu),
e o `STABILIZE_JS` é reaplicado para congelar animações iniciadas pela
interação (determinismo). O novo `Phase::Interacting` (progresso 0.35) alimenta
os `notifications/progress` do MCP. Em passo quebrado, o erro ganha contexto da
cadeia (`action #N ... failed` + passos anteriores).

### 3c. Mapa de efeito de UI (`effects` / `__actions`)

Para responder "o que houve a nível UI e onde", cada ação é dividida em
`prepare` (espera alvo visível+tamanho, `scrollIntoView`, resolve path/rect/
centro) e `perform` (dispatch + settle) — o `sniffer` captura um snapshot leve
de página inteira entre as duas (`sniff_engine::effects::capture`, um único
`Runtime.evaluate` sobre `querySelectorAll('*')` com uma assinatura de ~38 props
visuais/layout) e difa em Rust (`effects::diff`). O report (`what` + `where`) é
emitido na linha reservada `__actions`:

- O que: `appeared`/`removed`/`changed`, classificação `effect` (`revealed`/
  `hidden`/`changed`/`moved`/`no_effect`) e `summary` determinística.
- Onde: `rect`, `onscreen`, `out_of_view.{above,below,left,right}`,
  `distance_from_action` (px do ponto clicado) e `direction` (bússola).
- Assinatura CSS compacta e orientada a tokens:
  - `css_keys`: a lista de ~38 props emitida **uma vez por entrada** com
    records `appeared`/`removed`; os records referenciam por índice em
    `css_after_values`/`css_before_values` (arrays alinhados).
  - `changed` usa `css_diff` — só as props que mudaram além da tolerância
    numérica (`16px` vs `16.2px` = igual), com `before`/`after` por prop.
  - Campos vazios/ausentes são omitidos (`out_of_view`, `text`, `aria`,
    `rect_before`/`rect_after`, `visible_before`/`visible_after`).
  - Nós raiz (`html`/`body`) só reportam mudanças de tema/visual
    (`ROOT_SIGNATURE_PROPS`): reflow geométrico (scrollbar, altura, padding)
    não entra em `changed`; `changed` é ordenado semântico-primeiro.

O `sniffCSS-diff`/`sniffCSS_diff` comparam os blocos `__actions` quando presentes
(`diff_actions` em `sniff-css-diff`) com tolerância numérica nos rects, **reidratando
os arrays compactos de volta para nomes legíveis** (`appeared[0].css_after.display`)
→ `ACTION_CHANGED`/`ACTION_ADDED`/`ACTION_REMOVED` + `actions_changed` no resumo —
vira teste de regressão de UI ("o modal continuou abrindo na tela, no mesmo
lugar?").

### 4. Catálogo de propriedades
`sniff-core::properties` define 8 categorias com ~250 propriedades padrão da
web. O catálogo evita serializar as ~400 propriedades computadas de cada
elemento — só as solicitadas são extraídas (ganho ~80% em payload).

### 4b. Identidade de sessão (headers + storage state)

A sessão é configurada **antes** do `Page.navigate` no `sniff_session_with_progress`
(`sniff_engine::sniffer`):

- **Headers HTTP** (`config.headers` → `Network.setExtraHTTPHeaders`): aplicados
  a todo request da sessão; cobre auth stateless por header (ex. middleware IA
  de CMS `X-CMS-AI-Token`) sem token em URL/proxy.
- **Estado de sessão** (`config.storage_state_path` → `sniff_core::storage`):
  formato Playwright `storageState` (cookies + `origins[].localStorage`).
  Restauração = `Network.setCookies` (cookies escopados por domain/path) +
  `Page.addScriptToEvaluateOnNewDocument` com um script **origin-guarded** que
  grava o `localStorage` antes dos scripts da página (timing correto para SPAs).
- **Exportação** (`config.save_storage_state`): ao fim do pipeline, lê
  `Network.getAllCookies` + `localStorage` da origem atual e grava o mesmo JSON
  — um login feito por `actions` vira estado persistente entre restarts.

## Formato de saída

- `JsonLines` (padrão): uma linha por elemento raiz (árvore aninhada), streaming-friendly.
- `JsonLinesFlat`: um nó por linha com `id`/`parent_id` (achatado).
- `Json`: array único (com `--pretty`).
- Todos os nós carregam `id` (pre-order) e `parent_id`.
- Agrupamento por categoria ou achatado (`--no-group`).
- Normalização de cores `rgb(...)` → `#hex` (feita no JS durante a extração).

### Modo compacto (`--compact`)

Reduz tokens em ~55% em três frentes (implementadas em `output.rs`):

1. **Deduplicação lógico/físico** — remove `*-block-*`/`*-inline-*`/`inset-*`
   quando idênticos ao físico (`physical_equivalent`).
2. **Supressão de defaults** — remove valores de ruído (`0px`, `none`, `normal`,
   `auto`, ...) fora da `KEEP_DEFAULTS` allowlist.
3. **`css_variables` escopado** — o mapa global de `:root` é extraído uma vez
   (`extractor` retorna `SniffOutcome::global_css_variables`) e emitido como linha
   `__meta`; cada nó mantém apenas os overrides locais (`scope_variables`).

### AI-friendly derived fields

- `is_user_noticeable` é calculado **na página** durante a extração, reutilizando o único
  `getComputedStyle` por elemento (refatorado em `buildNode`) + `getBoundingClientRect`
  já exigido por `rect` — custo marginal ≈ zero. Divide o antigo `is_visible` em dois
  eixos ortogonais:
  - `display_visible` — renderizado de fato (`display`≠`none`, `visibility`≠`hidden`,
    `opacity`>0, tamanho≠0). **Sem interseção com o viewport**: conteúdo fora da dobra
    (rodapé, skip-links) continua `display_visible:true`.
  - `accessibility_grade` — `NONE` (não exposto à AT: `aria-hidden`, `hidden`/`inert`,
    `display:none`, zero-size), `AA` (exposto mas fora da tela/transparente/sem nome
    acessível) ou `AAA` (na tela, exposto e nomeado quando o role exige).
- `computed_style_hash` é um **xxHash64** (feature `xxh3` do `xxhash-rust`, ~20–30 GB/s,
  ~40× mais rápido que SHA-1) calculado em `output.rs` sobre a serialização canônica
  dos estilos efetivos de cada nó (estável graças à ordem determinística do `Map` com
  `preserve_order`). Uma mudança no hash == mudança no output; usado para
  diffing entre execuções sem re-serializar.
- `--stable-key attr` (`stable_key` no config): o snippet JS prefere esse atributo
  (ex.: `data-testid`) como âncora ao montar `selector`/`path`, escapando aspas.
  Seletores estáveis são a chave do diffing entre deploys que regeneram `id`.

## Pipeline de diff determinístico (sniffCSS-diff)

Separa o "o que mudou" (determinístico, sem IA) da avaliação de impacto (IA):

```
base.jsonl ─┐
            ├─ sniffCSS-diff (match por selector + posição, tolerância) → delta.jsonl → IA
head.jsonl ─┘
```

`crates/sniff-css-diff`:

- **`model`** — parseia `jsonl` (árvore) e `jsonl-flat` (reconstrói floresta por
  `parent_id`); ignora linhas `__meta`. Nós com `parent_id` órfão viram raiz
  (nenhum nó se perde).
- **`diff`** — pareamento por `selector` estável (fallback posicional por ordem de
  irmãos), depois diff por propriedade com tolerância numérica (`--tolerance`):
  `16px` vs `16.2px` são iguais dentro de `0.5`, mas `16px` vs `16rem` **nunca**
   (a unidade é comparada). Detecta mudanças em `styles`, `pseudo`, `rect`,
   `metrics` e `is_user_noticeable`.
- **`output`** — JSONL de deltas: `CHANGED` com `{before, after}` por propriedade,
  `ADDED`/`REMOVED` com snapshot completo; `--stats-only` para varrer centenas de
  páginas sem gastar tokens.

A avaliação semântica (positiva/negativa) é responsabilidade do LLM: contrato em
`docs/sniffCSS-eval.schema.json`, template em `docs/eval-prompt.md`.

## Servidor MCP (sniffCSS-mcp)

`crates/sniff-css-mcp` envolve engine + diff como tools MCP sobre stdio (`rmcp`):

- **`sniffCSS_page`** — um `ChromePool` (um Chrome headless + `Semaphore(3)`)
  executa o pipeline via `Sniffer::new_session` +
  `sniff_session_with_progress`; cada fase emite `notifications/progress`
  (`ProgressReporter` → `Peer::notify_progress`) e o JSONL final volta como
  resultado da tool. Sem block do pipeline: as notificações são enviadas
  assincronamente entre as fases. Por padrão o `SnapshotStore` persiste o
  snapshot em `sniffCSS/[domain]/[UTC]-[path]-[selector].jsonl` (escrita
  atômica, UTC por `SystemTime`, raiz via `SNIFF_SNAPSHOT_DIR`) e a tool
  retorna apenas o `__sniff` reference; `return:"jsonl"`/`persist:false`
  optam pelo comportamento inline.
- **`sniffCSS_diff`** — aceita `base_path`/`head_path` (resolvidos pelo
  `SnapshotStore` com guarda anti path-traversal) ou `base_jsonl`/`head_jsonl`
  inline → `sniff_diff::load_file`/`load_str` + `diff_trees` + `write_delta`,
  com linha `__diff_summary` ao final.
- **`sniffCSS_check`** — idem: `path` (persistido) ou `jsonl` inline.
- **`sniffCSS_snapshots`** — enumera os arquivos do `SnapshotStore`
  (domain/target/path/created_at/size), para o agente escolher o par base/head.
- **Recursos** — `sniffCSS://prompts/eval`, `sniffCSS://schemas/eval`
  (`include_str!` das docs).
- Transporte: `stdio()` (todos os clientes). Um browser morto é relançado
  transparentemente (detecção de erro de transporte no `ChromePool`).

### Cuidados de concorrência

- `Sniffer::sniff`/`new_session` são `&self`; o `CdpClient` multiplexa
  sessões por `sessionId`, então chamadas concorrentes usam uma única conexão WS.
- O `semaphore` limita a carga no Chrome; o pool não precisa de `Mutex` global —
  apenas um `RwLock` para trocar o `Sniffer` no relaunch.

## Performance

| Técnica | Onde | Efeito |
|---|---|---|
| 1 `Runtime.evaluate` por sniff | engine/extractor | elimina round-trips |
| Browser pool (`Sniffer` reutilizável) | engine/sniffer | elimina cold-start ~2s |
| Filtro de propriedades por categoria | engine/extractor | ~80% menos dados |
| Normalização de cores no JS | engine/extractor | sem pós-processamento |
| JSONL streaming | engine/output | memória constante |

## Testes

- **Unit**: catálogo, config, parse de args, serialização de saída, roteamento CDP.
- **Integração** (`sniff-engine/tests/integration.rs`): Chrome real + fixtures;
  auto-skip se não houver Chrome. Uso de semáforo global + retry de launch para
  robustez em CI/containers.
