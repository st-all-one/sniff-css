# Fase 3 — Plano de rigor (consistência, estabilidade, segurança, performance)

> Complementa [`quality.md`](quality.md). A Fase 2 zerou o débito de
> documentação e qualificação. A Fase 3 paga o débito de **comportamento**
> (erros, determinismo, aritmética, `unsafe`, alocação) e liga os grupos
> `pedantic`/`nursery`.
>
> **Base de referência:** `.agents/skills/rust/` (v1.99.1, Edition 2024).
> Cada item abaixo cita o capítulo da skill que fundamenta a decisão.

## 0. Como usar a skill

`.agents/skills/rust/SKILL.md` é o índice; os capítulos relevantes para esta
fase são:

| Capítulo | Uso na Fase 3 |
|---|---|
| `06-error-handling.md` | `Result`/`?`, banir `unwrap`/`expect`/`panic`, thiserror × anyhow |
| `05-collections.md` | `get()` vs indexação, `BTreeMap` × `HashMap`, `with_capacity`, entry API |
| `08-closures-iterators.md` | pipelines > loops com índice, `filter_map`, lazy, `enumerate` |
| `03-ownership.md` / `10-smart-pointers.md` | `&str`/`&[T]`, evitar `clone`, `Cow`, `Arc`/`Mutex` |
| `11-concurrency.md` / `12-async.md` | `Send`/`Sync`, guardas fora do `.await`, cancellation |
| `14-unsafe-macros-ffi.md` | isolar `unsafe`, `// SAFETY:`, Miri |
| `13-testing.md` | testes com `Result`, `criterion` |
| `15-cargo-deep.md` / `16-project-practical.md` | features/profiles, CI, performance |

## 1. Objetivos por eixo

- **Consistência** — determinismo de saída, estilo uniforme, sem lugares-comuns
  perigosos (`unreachable!`), `pedantic`/`nursery` ligados.
- **Estabilidade** — todo erro recuperável como `Result`; sem panics acidentais;
  aritmética sem overflow silencioso; entrada/saída determinística.
- **Segurança** — `unsafe` isolado e documentado; nada de `Rc`/`RefCell`/
  `transmute`; supply chain auditada; Miri/fuzz nos parsers.
- **Performance** — iteradores no lugar de indexação; menos alocações e clones;
  capacidade pré-reservada; mapas adequados; benchmark contínuo.

## 2. Workstreams

> **Status:** fase 3 **concluída** — A1–A4, B1–B7, C1–C5, D1–D6 ✅ (ver §7).

### A — Consistência (estilo e determinismo)

| # | Item | Lint(s) | Skill | Abordagem |
|---|---|---|---|---|
| A1 | Determinismo de `HashMap`/`HashSet` | `disallowed_types` | 05 | ✅ 46 usos convertidos a `BTreeMap`/`BTreeSet`. Decisão de projeto: **determinismo sempre**, mesmo onde a ordem não afeta a saída — zero `HashMap`/`HashSet` no código. Lint promovida a `deny`. |
| A2 | Estilo global | `absolute_paths`, `min_ident_chars`, `let_underscore_untyped`, `excessive_nesting`, `allow_attributes`, `missing_docs_in_private_items` | 02, 09 | ✅ `allow_attributes` e `let_underscore_untyped` → `deny`; 21 `#[allow]` justificados viraram `#[expect]` (falham se a lint sumir); `let _` passou a `drop`/`let _: T`. **Não adotadas** (ruído de domínio): `min_ident_chars` (1729), `excessive_nesting` (262), `absolute_paths` (183), `missing_docs_in_private_items` (156) — docs públicas já em `warn`. |
| A3 | Grupos de lint | `pedantic`, `nursery`, `cargo` | 15 | ✅ Ligados em `warn` (prioridade `-1`, como a referência knudge); o gate `-D warnings` exige **zero**. 489 diagnósticos → 0: `clippy --fix` para os machine-applicable, correções reais no resto, allowlist comentada para ruído de domínio (`too_many_lines`, `too_long_first_doc_paragraph`, `similar_names`, `unused_async_trait_impl`, `redundant_pub_crate`). |
| A4 | Macros de "impossível" | `disallowed_macros`, `unreachable`, `panic` | 06 | ✅ 0 em produção: `parse_action` usa `.last()` em vez de `unreachable!`; `Backend::Auto` → novo `EffectiveBackend` (main sem ramo impossível); `flutter::action::perform` delega o ramo web-only a `unsupported()`. `unreachable!`/`panic!` de teste via `allow-panic-in-tests`. Lints promovidas a `deny`. |

**Aceite:** lints promovidos a `deny` (ou `warn` para pedantic/nursery),
`cargo clippy -- -D warnings` verde.

### B — Estabilidade (erros e aritmética)

| # | Item | Lint(s) | Skill | Abordagem |
|---|---|---|---|---|
| B1 | `unwrap`/`expect` de produção | `unwrap_used`, `expect_used` | 06 | ✅ Produção real: **5 sites** (`sniff-flutter/src/{inspector,machine}.rs`) → `unwrap_or_default`. A estimativa de ~384 vinha de código de teste dentro de `src/`. Lint promovida a `deny`. |
| B2 | `unwrap`/`expect` em testes | `unwrap_used`, `expect_used` | 06, 13 | ✅ Por decisão de escopo: `.unwrap()`/`.expect()` em **corpo de `#[test]`** é o mecanismo de falha → `allow-*-in-tests = true`; helpers fora de `#[test]` ganharam `#![allow]` de arquivo justificado (8 arquivos). 415 testes seguem verdes. |
| B3 | Indexação/slicing | `indexing_slicing` | 05, 08 | ✅ 0 em produção (158→0). `serde_json::Value["k"]` em testes é isento (`allow-indexing-slicing-in-tests = true`, o panic É a falha); em produção: `.get()`/`first()`/`split_once`/`strip_prefix`/`split_at`. Lint promovida a `deny`. |
| B4 | Aritmética | `arithmetic_side_effects` | 02 | ✅ 0 no workspace (119→0). `saturating_*`/`checked_*`/`elapsed()`+`saturating_sub`; `f32`/`f64` isentos. Lint promovida a `deny`. |
| B5 | Fontes não-determinísticas | `disallowed_methods` | 12, 16 | ✅ 34→0. `sniff_core::env::{var,var_os}` é o **único** ponto de leitura de ambiente (sniff-cdp tem uma exceção local, pois não depende de `sniff-core`); `utc_now()` é a **única** leitura de relógio, e `snapshot_file_name_at` permite injetar o timestamp; `mem::forget` (emulador) com `#[expect]`; sem `process::exit`. Lint promovida a `deny`. |
| B6 | stdout/stderr | `print_stdout`, `print_stderr` | 16 | ✅ stdout é **dado** (JSON da CLI), nunca `println!` → `print_stdout` em `deny`. stderr é diagnóstico: escrita única auditada em `sniff_core::diagnostic::write` (`#[expect]`), usada pelos binários; testes isentos (`allow-print-in-tests` + bloco de arquivo nos helpers). |
| B7 | Casts numéricos | `as_conversions`, `cast_possible_truncation`, `cast_sign_loss`, `cast_possible_wrap`, `cast_precision_loss` | 02, 06 | ✅ 121→0 (25 expressões). Inteiros via `From`/`TryFrom`; `f64`→int auditado em `sniff_core::numeric::{f64_to_u32,f64_to_u64}`; sort keys por `f64::total_cmp`/`to_bits` (precisão total + ordem determinística). Lints promovidas a `deny`. |

**Aceite:** `unwrap_used`/`expect_used`/`panic`/`unreachable`/`indexing_slicing`/
`arithmetic_side_effects`/`as_conversions`/`cast_*` em `deny`; determinismo
assegurado por porta/injeção.

### C — Segurança (memória, concorrência, supply chain)

| # | Item | Lint/ferramenta | Skill | Abordagem |
|---|---|---|---|---|
| C1 | `unsafe` eliminado (não apenas documentado) | `undocumented_unsafe_blocks`, `unsafe_code` | 14 | ✅ Os 6 blocos eram `env::set_var`/`remove_var` em testes de `sniff-css`. Trocados por **injeção do `root`** em `persist_snapshot` (skill 12/16): os testes passam o diretório, não mutam o ambiente. Zero `unsafe`, sem `ENV_LOCK`. |
| C2 | `unsafe` proibido no núcleo | `unsafe_code` + `#![forbid(unsafe_code)]` | 14 | ✅ `forbid` nos roots de `sniff-core` (lib), `sniff-css-diff` (lib+bin) e `sniff-css-check` (lib+bin). |
| C3 | Concorrência | `await_holding_lock`, `await_holding_refcell_ref` (deny) | 11, 12 | ✅ Auditoria: só `tokio::sync::Mutex` (async-aware) cruza `.await` (`sink`); `std::sync::Mutex` (`pending`) e locks de stdout têm escopo curto, sem `.await` junto. Lints em `deny`. |
| C4 | Verificação dinâmica | Miri + fuzz | 14 | ✅ `sniff-core/tests/miri_smoke.rs` (3 testes, Miri verde, sem UB); crate de fuzz standalone com `jsonl_decode` e `jsonrpc_decode`; o decoder JSON-RPC virou `decode_incoming` (puro/total, testável). |
| C5 | Supply chain | `cargo deny` + `cargo audit` | 16 | ✅ `deny.toml` + `cargo audit` e `cargo machete` no CI (`.github/workflows/ci.yml`); deps não usadas removidas (`sniff-engine`, `sniff-flutter`, `sniff-css`). |

**Aceite:** `unsafe` documentado/isolado, `forbid` no núcleo, Miri smoke verde,
fuzz build verde, `cargo deny` verde.

### D — Performance

| # | Item | Lint/ferramenta | Skill | Abordagem |
|---|---|---|---|---|
| D1 | Loops com índice → iteradores | (habilitou `indexing_slicing`) | 05, 08 | ✅ `cands: Vec<&Flat>` + `skip`, `zip`, `filter_map`, `first`/`get`. Oclusão/backdrop/uniformidade/`build_forest`/`nest` reescritos sem índices. |
| D2 | Alocações e clones | `pedantic`/`nursery`: `needless_pass_by_value`, `redundant_clone`, `needless_collect`, `str_to_string`, `unnecessary_to_owned` | 03, 10 | ✅ Pagos no A3 (os lints ficaram em **0**). Revisão extra: `Vec::with_capacity` onde o tamanho é conhecido (`parse_elements`); `collect()` sobre `slice::iter` já reserva via `size_hint`. |
| D3 | Saída em buffer | `write!` + buffers; `format_into`/`NumBuffer` (1.98+) | 02, 05 | ✅ A saída já é `BufWriter` + `serde_json::to_writer` (nunca `format!` de JSON inteiro); `format_push_string` resolvido no A3. Nenhum `format!` intermediário em loop quente. |
| D4 | Escolha de mapa | — | 05 | `HashMap` para lookup quente; `BTreeMap` onde ordem/determinismo importa; medir antes de trocar. |
| D4 | Escolha de mapa | — | 05 | ✅ Determinismo como padrão (diretriz): todo `HashMap`/`HashSet` virou `BTreeMap`/`BTreeSet` (A1); medição mostrou igual-ou-mais-rápido nesses tamanhos. |
| D5 | Benchmark contínuo | `criterion` | 13, 16 | ✅ 4 benches `harness = false` (diff, check, color, ax) + baseline em [`docs/benchmarks.md`](benchmarks.md). `cargo bench -p <crate> --bench <name>`; regressão > 5% exige justificativa. |
| D6 | Profile/lint de tamanho | `large_futures`, `large_stack_arrays`, `result_large_err`, `large_enum_variant` | 15, 16 | ✅ Ativos desde a fase 2; `Box::pin` nos futures grandes e enums/erros grandes tratados. |
| D6 | Profile/lint de tamanho | `large_futures`, `large_stack_arrays`, `result_large_err`, `large_enum_variant` | 15, 16 | Já ativos; `Box::pin` para futures grandes; `Box` para erros grandes. |

**Aceite:** `cargo bench` publicado como baseline; sem regressão > 5% sem
justificativa; lints de performance de `pedantic`/`nursery` em `deny`.

## 3. Ordem de execução

1. **A1 + A2** (determinismo/estilo) — mecânico, baixo risco. ✅ concluído.
2. **C1 + C2** (`unsafe` documentado + `forbid` no núcleo) — baixo risco. ✅ concluído.
3. **B3 + B4 + D1** (indexação/aritmética/iteradores) — alto ganho, feito junto.
4. **B1 + B2** (`unwrap`/`expect`): por crate, libs antes de bins, testes por último.
5. **B5 + B6** (portas `Env`/`Clock`, stdout como dado).
6. **D2 + D3 + D5** (alocação/buffers/benchmarks).
7. **A3** (`pedantic`/`nursery`/`cargo`).
8. **C4 + C5** (Miri/fuzz/audit no CI).

## 4. Orçamento de `unwrap`/`expect` (produção)

| Crate (`src/`) | Ocorrências | Observação |
|---|---:|---|
| `sniff-engine` | 84 | extração/serialização; muitos `.unwrap_or_*` já existem |
| `sniff-css` | 84 | CLI `main.rs` + `args.rs` |
| `sniff-core` | 57 | config/storage/snapshot |
| `sniff-css-diff` | 40 | parser/model |
| `sniff-cdp` | 33 | jsonrpc/client |
| `sniff-css-mcp` | 32 | server/store |
| `sniff-css-check` | 31 | rules/uniformity |
| `sniff-flutter` | 23 | device/machine/inspector |
| **Total** | **384** | + ~199 em testes |

## 5. Política de exceções

- `#[expect(lint, reason = "…")]` é preferível a `#[allow]`: se a lint deixar de
  disparar, o build **falha** e a supressão não vira zumbi.
- Toda exceção aponta para o capítulo da skill ou para uma restrição de
  plataforma (ex.: `env::set_var` é `unsafe` na Edition 2024).
- **Nunca** desligar um lint de workspace para "limpar" o CI.

## 6. Definição de pronto (Fase 3)
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` e `cargo deny check` verdes.
- `unwrap_used`, `expect_used`, `panic`, `unreachable`, `indexing_slicing`,
  `arithmetic_side_effects`, `as_conversions`/`cast_*`, `disallowed_types`,
  `disallowed_methods`, `disallowed_macros`, `print_*`, `undocumented_unsafe_blocks`
  promovidos de `allow` para `deny`.
- `pedantic`/`nursery`/`cargo` em `warn` (ou `deny` onde já zerados).
- CI com `clippy`, `test`, `deny`, `miri` (smoke) e `fuzz build`.
- Baseline de `criterion` para os caminhos quentes.

## 7. Progresso

### C1 + C2 — concluídos

**C1 (eliminação de `unsafe`)** — `crates/sniff-css/src/main.rs`:

- `persist_snapshot` passou a receber `root: &Path` em vez de ler
  `SNIFF_SNAPSHOT_DIR` internamente; `emit` resolve o root uma vez com
  `snapshot_root()`.
- Os testes `persist_writes_to_sniff_dir_with_gitignore` e
  `persist_uses_json_extension_for_json_output` passam o diretório temporário
  diretamente. Removidos os seis blocos `unsafe`, o `#[allow(unsafe_code)]` do
  módulo de testes e o `ENV_LOCK`.
- Resultado: **zero `unsafe` no workspace** (não apenas documentado) — a
  abordagem de injeção de dependência do capítulo 12/16 da skill prevaleceu
  sobre apenas anotar `// SAFETY:`.

**C2 (`forbid`)** — `#![forbid(unsafe_code)]` adicionado aos crate roots de:

- `crates/sniff-core/src/lib.rs`
- `crates/sniff-css-diff/src/lib.rs` e `crates/sniff-css-diff/src/main.rs`
- `crates/sniff-css-check/src/lib.rs` e `crates/sniff-css-check/src/main.rs`

Verificação: `cargo fmt --check` ✓, `cargo clippy --workspace --all-targets --
-D warnings` ✓, `cargo test --workspace` **415 testes** ✓, `cargo deny check` ✓.

### A1 — determinismo de coleções (concluído)

Decisão de projeto: **determinismo sempre**. **Zero `HashMap`/`HashSet` no
código** — todos os 46 usos viraram `BTreeMap`/`BTreeSet`, inclusive os lookups
quentes. As exceções `#[expect(clippy::disallowed_types)]` foram removidas.

Destaques:

- `sniff-css-check/src/uniformity.rs`: a moda era escolhida com
  `HashMap::iter().max_by_key(..)` — em empate o vencedor variava entre
  execuções. `BTreeMap` fixa a ordem e torna a escolha determinística (bug real,
  não só lint).
- `sniff-css-check/src/rules.rs`: `covered` (oclusão) virou `BTreeMap`; a saída
  passa a ser iterada diretamente, **eliminando** o `Vec` de chaves + `sort`.
- `sniff-engine/src/output.rs`: `GlobalVars` virou `BTreeMap`; `vars_to_json`
  deixou de coletar e ordenar as chaves — a ordem já vem ordenada.
- `sniff-engine/src/effects.rs`: `keyed` virou `BTreeMap` (a ordem alimentava
  `appeared`/`removed`/`changed` antes do `sort_by_key`).
- `sniff-core/src/properties.rs`: `HashSet` de dedup → `BTreeSet`.
- `sniff-css-diff::NodeIndex`, `sniff-cdp::jsonrpc::PendingMap`,
  `sniff-engine::ax::{AxFacets, AxTreeIndex}` e `sniff-flutter` → `BTreeMap`.

### A2 — estilo (concluído no que faz sentido)

- `#![forbid]`/`#[expect]`: os 21 `#[allow(clippy::…)]` justificados viraram
  `#[expect(clippy::…)]`; se o lint parar de disparar, o build quebra.
- `allow_attributes` e `let_underscore_untyped` promovidas a `deny`.
- Os 5 `let _ = …` viraram `drop(..)` ou ganharam tipo (`let _: bool = …`); o
  parâmetro morto `category` de `compact_group` foi removido.
- **Não adotadas** (empiricamente ruidosas e de baixo valor para o domínio):
  `min_ident_chars` (1729, nomes geométricos `x/y/w/h/i/j` e closures `|e|`/`|v|`),
  `excessive_nesting` (262, `match` de parser/UI), `absolute_paths` (183) e
  `missing_docs_in_private_items` (156 — docs públicas já são `warn`). Ficam em
  `allow` documentado, conforme a política "relaxar antes de espalhar exceções".

### Performance (avaliação)

Como o determinismo foi priorizado, todos os mapas são `BTreeMap`. Ainda assim,
**não houve regressão** — em vários casos a remoção de uma ordenação/alocação
explícita compensou o `O(log n)`.

Medições A/B (release, `lto=fat`, `codegen-units=1`):

**`sniff check --rules`** — snapshot sintético de 1201 nós com sobreposição
(oclusão para todos), 30 execuções:

| Implementação | mediana | min |
|---|---:|---:|
| `HashMap` + collect + `sort` | 22,1 ms | 20,1 ms |
| `BTreeMap` iterado direto | **21,0 ms** | **19,9 ms** |

**`sniff diff`** — snapshot flat de 1200 nós, 40 execuções (mede
`NodeIndex`):

| Implementação | mediana | min |
|---|---:|---:|
| `HashMap` | 35,7 ms | 33,6 ms |
| `BTreeMap` | **33,2 ms** | **31,7 ms** |

Em ambos, `BTreeMap` ficou igual ou ~5% mais rápido. O custo real está na
geometria O(n²) da oclusão e no parsing, não nas operações de mapa. Benchmarks
contínuos com `criterion` (D5) ficam para a fase de performance.

### B3 + B4 + D1 — concluídos

Débito inicial (com testes isentos de `indexing_slicing`): **158 indexações de
produção + 119 aritméticas**. Resultado: **zero** em todo o workspace; as duas
lints foram promovidas a `deny`.

- **D1 (iteradores):** a oclusão passou de `cands: Vec<usize>` + `flat[i]` para
  `cands: Vec<&Flat>` + `cands.iter().skip(..)`, com `related`/
  `same_svg_container`/`effective_z_index`/`paints_above` recebendo `&Flat`.
  O mesmo em `check_backdrop_over_modal`, `build_forest` (`sniff-css-diff`),
  `nest`/`walk` (`sniff-flutter`) e `summarize` (agora com `.filter().count()`).
- **B3 (indexação):** `split_once` para `://`, `strip_prefix` para o banner do
  DevTools, `split_at` para o seletor, `get`/`first` em vetores e
  `Value::get_mut` no lugar de `reference["__sniff"][..] = ..`.
- **B4 (aritmética):** contadores com `saturating_add`, `Duration` de timeout
  com `Instant::now()` + `start.elapsed()` + `saturating_sub` (elimina o
  `Instant + Duration`, que pode estourar com `timeout_ms` absurdo),
  `median` com `>> 1` + `is_multiple_of(2)`.
- **Isenção:** `allow-indexing-slicing-in-tests = true` — em teste,
  `json["k"]["j"]` é idiomático e o panic É a falha do teste.

**Performance (A/B, release):** o primeiro corte da oclusão usava
`BTreeSet<usize>` para `used_h`; isso custou ~19% no `sniff diff`
(33→40 ms). Como a ordem de `used_h` não define a saída, voltou para
`Vec<bool>` indexado via `.get` — o determinismo vem da iteração
`base`/`head`, não da estrutura. Resultado final:

| Caminho | Antes | Depois |
|---|---:|---:|
| `sniff check --rules` (1201 nós) | 22,1 ms | **21,3 ms** |
| `sniff diff` (1200 flat) | 35,7 ms | **33,8 ms** |

Sem regressão; a oclusão até ficou marginalmente mais rápida por evitar o
`Vec` de índices.

### B5 — fontes não-determinísticas (concluído)

Débito inicial: **34 usos** (`env::var`×18, `SystemTime::now`×7, `env::var_os`×5,
`mem::forget`×4). Resultado: **zero**; `disallowed_methods` promovida a `deny`.

- **Ambiente:** `sniff_core::env::{var,var_os}` é o único ponto que toca o
  ambiente do processo (um `#[expect]` por função), e todos os adaptadores
  passam a usá-lo. `sniff-cdp` fica de fora por design (só tem `sniff-core`
  como dev-dependency) e carrega uma exceção local justificada.
- **Relógio:** `snapshot::utc_now()` é a única leitura de `SystemTime::now`. A
  lógica pura usa `snapshot_file_name_at(.., stamp)`, então dá para testar com
  timestamp fixo sem portar um trait por toda a pilha de armazenamento.
- **Nomes únicos de temporários:** os `SystemTime::now()` de teste e do
  navegador viraram `process::id()` + contador atômico — sem relógio.
- **`mem::forget`:** as duas exceções (emulador) já tinham `#[expect]`; agora
  também cobrem `disallowed_methods`.
- **`process::exit`:** não havia nenhum (a CLI devolve `Result`).

### B1 + B2 — `unwrap`/`expect` (concluído com escopo revisado)

A estimativa de ~384 sites de produção revelou-se enganosa: com o
`allow-unwrap-in-tests` ligado, sobravam **51 sites fora de `#[test]`**, e
destes apenas **5 são produção real** (`sniff-flutter/src/{inspector,machine}.rs`,
trocados por `unwrap_or_default`). Os outros ~46 eram funções auxiliares de
teste.

**Decisão de escopo (B2):** converter 415 funções de teste para
`Result<(), E>` + `?` seria ~900 linhas de churn com ganho de segurança
marginal — em teste, `.unwrap()`/`.expect()` **é** o mecanismo de falha (o panic
aponta a linha). Então:

- `allow-unwrap-in-tests = true` / `allow-expect-in-tests = true`;
- os 8 arquivos de suporte a teste (funções auxiliares fora de `#[test]`)
  ganharam `#![allow(clippy::unwrap_used, clippy::expect_used, reason = "…")]`,
  expressando "este arquivo inteiro é código de teste";
- `unwrap_used` e `expect_used` promovidas a `deny`.

Panics intencionais continuam via `#[should_panic]`/`panic!` (fora do escopo de
B1/B2). Se o time preferir a conversão estrita, ela fica como trabalho
mecânico incremental — o gate não muda.

### A4 + B6 — `panic`/`unreachable`/`disallowed_macros`/`print_*`

**Ramos impossíveis (A4), resolvidos sem panic/cast:**

- `sniff-core::config::parse_action` — o `_ => unreachable!()` virou
  `trailing.last()` (o último token numérico é o timeout em toda aridade
  0/1/2); a checagem de `len() > 2` continua retornando `Err`.
- `sniff-css::args` — novo enum `EffectiveBackend` (só `Web`/`Flutter`);
  `effective_backend()` deixa de devolver `Auto`, então `main` deixa de ter o
  braço impossível. Testes comparam com `EffectiveBackend`.
- `sniff-flutter::action::perform` — em vez de `unsupported(action)?` seguido de
  `unreachable!()`, o ramo `other => unsupported(other)` concentra a rejeição
  dos kinds web-only.
- `disallowed-macros` perdeu `std::panic`/`std::unreachable` (redundantes com
  `clippy::panic`/`clippy::unreachable` e origem de falsos positivos dentro da
  expansão do `#[tool]` do rmcp).

**stdout/stderr (B6):**

- `print_stdout = deny`: stdout é **dado** (JSONL via `write_output`/`BufWriter`);
  não há `println!` em produção.
- `print_stderr = deny` + `allow-print-in-tests = true`: todo diagnóstico humano
  passa por `sniff_core::diagnostic::write` — o **único** ponto de escrita em
  stderr do workspace (1 `#[expect]` auditado). Binários usam-no; testes
  (`eprintln!("skipping: …")`) ficam isentos.
- `sniff-css-diff` ganhou dependência de `sniff-core` (crate-base, sem ciclo)
  para usar o mesmo helper.

**Isenção de teste:** `allow-panic-in-tests` cobre `panic!`, mas **não**
`unreachable!`; os dois `unreachable!()` de teste em `config.rs` viraram
`panic!("expected …, got {other:?}")` (mensagem melhor). Os helpers de teste de
integração (fora de `#[test]`) receberam `#![allow(clippy::panic, clippy::print_stderr, reason = "…")]`
no mesmo bloco já usado para `unwrap`/`expect`.

Lints promovidas a `deny`: `panic`, `unreachable`, `disallowed_macros`,
`print_stdout`, `print_stderr`.

### B7 — casts numéricos (`as_conversions` + `cast_*`)

Inventário: **121** diagnósticos, **25 expressões** em 9 arquivos. Todo `as`
numérico restante do workspace vive agora em `sniff_core/numeric.rs`
(dois helpers `f64 → u32`/`u64` com um único `#[expect]` justificado por
`round()` + `clamp`). O resto usa conversões da biblioteca padrão:

- **u8 → u32/f64** — `u32::from`/`f64::from` (lossless). Em `color.rs`, o
  extrator `valueProperties` passou a devolver `u8` (`u8::try_from`), então os
  canais ARGB são validados na entrada.
- **u64/u128 → usize/u64/i64** — `usize::try_from`/`u64::try_from`/
  `i64::try_from` com `unwrap_or` saturante. Em `sniff-cdp`,
  `Content-Length` inválido agora vira `CdpError` em vez de arriscar um
  `Vec::with_capacity` absurdo.
- **`civil_from_days`** (`snapshot.rs`) — dia/mês via `u32::try_from`.
- **Sort keys determinísticos** — em `rules.rs` e `effects.rs` os `as i64`/
  `as u8` de chave de ordenação viraram `f64::total_cmp` e `f64::to_bits`
  (`sort_by_cached_key`): precisão total, ordem total determinística (sem
  empates falsos por truncamento, sem `NaN`). O desempate por área/`path`
  mantém o resultado anterior.
- **`oov_dirs`** (`effects.rs`) — ordenação por `total_cmp` (era
  `partial_cmp().unwrap_or(Equal)`, não-determinístico com `NaN`).

Lints promovidas a `deny`: `as_conversions`, `cast_possible_truncation`,
`cast_sign_loss`, `cast_possible_wrap`, `cast_precision_loss`.

### A3 — grupos `pedantic`/`nursery`/`cargo`

Ligados em `warn` (prioridade `-1`, igual à referência knudge); como o gate roda
`-D warnings`, o resultado prático é **zero diagnósticos**. Inventário inicial:
**489** avisos em ~40 lints.

1. **`cargo clippy --fix`** aplicou tudo que era machine-applicable
   (`use_self`, `uninlined_format_args`, `redundant_closure_for_method_calls`,
   `needless_raw_string_hashes`, `map_unwrap_or`, `trivially_copy_pass_by_ref`…),
   derrubando 489 → 232.
2. **Correções reais** no restante:
   - `missing_fields_in_debug` → `.finish_non_exhaustive()` nos 7 `Debug` manuais.
   - `needless_pass_by_value` de produção → `&[T]`/`&T`; os dois wrappers de
     `.map_err` (`cdp_err`/`io_err`) e helpers de teste levam `#[expect]`
     justificado.
   - `ref_option` (`&Option<T>` → `Option<&T>`), `float_cmp` (tolerância sem
     `==`, equivalente inclusive com tolerância `0`), `assigning_clones`
     (`clone_from`), `default_trait_access`, `iter_on_single_items`,
     `needless_collect`, `manual_let_else`, `match_same_arms`.
   - `items_after_statements` → itens movidos para o topo do bloco.
   - `unused_async` em `FlutterMachine::{run,attach}` (só fazem `Command::spawn`)
     → removido `async`/`.await`.
   - `case_sensitive_file_extension_comparisons` → `Path::extension()` +
     `eq_ignore_ascii_case`.
   - `format_push_string` → `format!` + `push_str`.
3. **Allowlist comentada** (custo > valor, documentada no `Cargo.toml`):
   `too_many_lines`, `too_long_first_doc_paragraph`, `similar_names`,
   `unused_async_trait_impl` (`async` imposto pela trait), `redundant_pub_crate`
   (conflita com `unreachable_pub` em crate binário).

Estado final: `pedantic`/`nursery`/`cargo` em `warn` com **0** avisos, 415 testes
verdes, `cargo deny` ok.

### D2 + D3 + D5 — alocação / buffers / benchmarks

**D2 (alocação/clones):** já pago pelo A3 — os lints que definem o item
(`redundant_clone`, `needless_collect`, `str_to_string`, `unnecessary_to_owned`,
`needless_pass_by_value`) estão em **0**. Revisão adicional troca por
`Vec::with_capacity` onde a contagem é conhecida (`effects::parse_elements`);
`collect()` sobre `slice::iter` já reserva via `size_hint`, então não precisou
de `Cow`.

**D3 (saída em buffer):** a serialização já usa `BufWriter` +
`serde_json::to_writer` (nunca monta um JSON inteiro com `format!`), e o
`format_push_string` foi eliminado no A3. Nenhum `format!` intermediário em loop
quente.

**D5 (benchmarks):** `criterion 0.8` (dev-dependency) + 4 benches
`harness = false`:

| Crate | Bench | Alvo |
|---|---|---|
| `sniff-css-diff` | `diff` | `diff_trees` (100/500/1000 nós) |
| `sniff-css-check` | `check` | `check_uniformity` + `run_rules` (10/50/200) |
| `sniff-flutter` | `color` | `parse_flutter_color`/`parse_color_value` |
| `sniff-engine` | `ax` | `attach_ax` (50/500/2000) |

Baseline publicado em [`docs/benchmarks.md`](benchmarks.md). Os benches são
targets do workspace, então passam pelo mesmo `clippy --all-targets -D warnings`.
`cargo deny` segue limpo (criterion é MIT/Apache-2.0).

### C3 — concorrência

Auditoria dos primitivos de sincronização:

- `sniff-cdp::jsonrpc` usa **`tokio::sync::Mutex`** para o `sink` WebSocket — o
  único lock que atravessa `.await`, e é async-aware por design.
- O `pending` (`std::sync::Mutex`) é sempre acessado em escopo de expressão
  (`.lock().ok().and_then(...)`), sem `.await` com o guard vivo.
- Os locks de `stdout` (`StdoutLock`) vivem em código síncrono.

`await_holding_lock` / `await_holding_refcell_ref` seguem em `deny`.

### C4 — verificação dinâmica (Miri + fuzz)

- **Miri:** `crates/sniff-core/tests/miri_smoke.rs` (3 testes) cobre parsers de
  config, cor, numéricos e o nome de snapshot. Rodado com
  `cargo +nightly miri test -p sniff-core --test miri_smoke`
  (`MIRIFLAGS=-Zmiri-disable-isolation`): **3 passed, sem UB**.
- **Fuzz:** crate standalone `fuzz/` (excluído do workspace) com
  `libfuzzer-sys` e dois alvos: `jsonl_decode` (`sniff_css_diff::load_str`) e
  `jsonrpc_decode` (`sniff_cdp::decode_incoming`). O decoder JSON-RPC foi
  extraído do laço do WebSocket para `decode_incoming` — puro, total e
  testável (é o que o alvo exercita).
- Observação: o `cargo fuzz build` local exige um compilador C++ (runtime do
  libFuzzer); nesta máquina só há `gcc` (sem `cc1plus`) e sem `sudo`, então a
  construção é validada no CI (`ubuntu-latest` tem `build-essential`).

### C5 — supply chain + CI

Novo `.github/workflows/ci.yml` com quatro jobs:

| Job | Comandos |
|---|---|
| `check` | `fmt --check`, `clippy --all-targets -D warnings`, `test --workspace` |
| `supply-chain` | `cargo deny check`, `cargo audit`, `cargo machete`, `typos` |
| `miri` | `cargo miri setup` + smoke de `sniff-core` |
| `fuzz` | `cargo fuzz build` + 30 s por alvo |

`cargo machete` apontou deps realmente não usadas, removidas:
`futures-util`/`serde`/`thiserror`/`tracing` (`sniff-engine`), `serde`
(`sniff-flutter`) e `tracing` (`sniff-css`).
