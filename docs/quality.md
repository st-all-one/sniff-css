# Qualidade, lints e supply chain

Este documento é a fonte de verdade da política de rigor do workspace sniff.
Ele descreve **as duas camadas** da configuração (valores × níveis), o estado
atual da adoção e o caminho até o padrão final.

## 1. Toolchain e MSRV

- **Rust 1.99** (Edition 2024), fixado em `rust-toolchain.toml`.
- `rust-version = "1.99"` no `[workspace.package]` e `msrv = "1.99"` no
  `clippy.toml` **devem casar** — senão a lint `incompatible_msrv` fica
  inconsistente.
- `resolver = "3"` (resolver MSRV-aware da Edition 2024).

## 2. Duas camadas

| Camada | Onde | O que define |
|---|---|---|
| Níveis | `Cargo.toml` → `[workspace.lints.*]` | `deny`/`warn`/`allow` |
| Valores | `clippy.toml` | limites e allowlists das lints |

Cada crate herda os níveis com `[lints] workspace = true`. Sem isso, a maioria
das opções de `clippy.toml` não tem efeito (as lints `restriction` são `allow`
por padrão).

## 3. Comandos

```bash
cargo fmt --all -- --check                                   # formatação
cargo clippy --workspace --all-targets -- -D warnings         # lints (warnings = erro)
cargo test --workspace                                       # testes (doc-tests inclusos)
cargo deny check                                             # supply chain
```

O CI (quando habilitado) roda os mesmos comandos.

## 4. Formatação — `rustfmt.toml`

Apenas opções **estáveis** (a toolchain é estável; opções `unstable` seriam
ignoradas com aviso). Destaques: `style_edition = "2024"`,
`newline_style = "Unix"` e `hex_literal_case = "Lower"`.

## 5. Adoção em fases

A configuração final é a mesma do projeto de referência (knudge). Como o
sniff é um codelab legado, os níveis foram aterrissados em fases para não
quebrar o portão de qualidade de uma vez.

### Fase 1 — aplicada agora

Ativos como `deny`:

- Rust: `unsafe_code`, `unused_lifetimes`, `elided_lifetimes_in_paths`,
  `single_use_lifetimes`, `trivial_casts`, `trivial_numeric_casts`,
  `unsafe_op_in_unsafe_fn`, `macro_use_extern_crate`, `meta_variable_misuse`,
  `non_ascii_idents`, `noop_method_call`, `unused_import_braces`.
- Rustdoc: `broken_intra_doc_links`, `unused_footnote_definition`.
- Clippy (grupos, `priority = -1`): `all`, `correctness`, `suspicious`,
  `style`, `complexity`, `perf`.
- Clippy (restriction curada): `multiple_unsafe_ops_per_block`,
  `wildcard_imports`, `disallowed_fields`, `await_holding_lock`,
  `await_holding_refcell_ref`, `allow_attributes_without_reason`,
  `large_enum_variant`, `large_stack_arrays`, `todo`, `unimplemented`,
  `dbg_macro`, `mem_forget`.

### Fase 2 — aplicada (documentação e qualificação)

- Rust: `unused_qualifications` (`deny`), `missing_docs` (`warn`) e
  `unreachable_pub` (`warn`) — todos os itens públicos documentados e os `pub`
  inalcançáveis rebaixados a `pub(crate)`.
- Clippy (restriction curada): `let_underscore_must_use`,
  `let_underscore_drop`, `disallowed_names`, `struct_excessive_bools`,
  `fn_params_excessive_bools` e `type_complexity`.
- Ajuste de valor: `max-fn-params-bools = 1` (uma flag explícita é tolerada;
  acima disso, enum).
- Correções **reais** aplicadas (não supressões):
  - `ax::capture` passou a receber o enum [`AxOutput`] em vez de dois bools;
  - removido o flag CLI redundante `actions` (ficou só `--no-actions`);
  - `let _ = expr` trocado por `drop(expr)` (intenção explícita de descartar);
  - qualificações encurtadas e `tmp` renomeado para `temp_path`;
  - tipos complexos extra
dos para `type` aliases (`FlatEntry`, `PendingMap`, `ProgressLog`).
- Exceções **analisadas** com `#[allow(.., reason = "…")]` (não são silêncio):
  `struct_excessive_bools` em structs de configuração/CLI/MCP e em fatos
  independentes (`Cookie`, `aria*`); `too_many_arguments`/`fn_params_excessive_bools`
  em helpers que só agregam argumentos numa chamada.

### Fase 3 — débito remanescente

O plano detalhado (workstreams por eixo, ordem de execução e aceite) vive em
**[`docs/phase-3.md`](phase-3.md)**, ancorado nas boas práticas de
`.agents/skills/rust/`. Resumo do que ainda está em `allow`:

Já pagos: `disallowed_types` (A1), `allow_attributes` + `let_underscore_untyped`
(A2) e todo o `unsafe` (C1/C2) — ver `phase-3.md` §7.

| Lint | Ocorrências aprox. | Ação para promover a `deny` |
|---|---|---|
| ~~`unwrap_used` / `expect_used`~~ | ~~~583~~ | ✅ 5 sites de produção; testes isentos (`allow-*-in-tests`) |
| ~~`indexing_slicing`~~ | ~~alto~~ | ✅ zero em produção; `.get()`/`first()`/`split_at` |
| ~~`as_conversions` / `cast_*`~~ | ✅ 121→0 | `From`/`TryFrom`; `f64`→int em `sniff_core::numeric` |
| ~~`arithmetic_side_effects`~~ | ~~vários~~ | ✅ `saturating_*`/`checked_*`; `elapsed()`+`saturating_sub` |
| ~~`print_stdout`~~ | 0 | ✅ stdout é dado; `println!` banido (`deny`) |
| ~~`print_stderr`~~ | ~18 prod | ✅ centralizado em `sniff_core::diagnostic` (1 `#[expect]`); testes isentos |
| ~~`disallowed_types` (`HashMap`/`HashSet`)~~ | ~~~46~~ | ✅ zero `HashMap`/`HashSet`; tudo `BTree*` (determinismo sempre) |
| ~~`disallowed_methods` (`env::var`, `SystemTime::now`)~~ | ~~vários~~ | ✅ `sniff_core::env` + `utc_now` centralizados; contadores p/ temporários |
| ~~`disallowed_macros` / `panic` / `unreachable`~~ | ✅ 0 prod | ramos impossíveis resolvidos; teste via `allow-panic-in-tests` |
| ~~`undocumented_unsafe_blocks`~~ | ~~6~~ | ✅ zero `unsafe` no workspace |
| ~~`allow_attributes`, `let_underscore_untyped`~~ | ~~26~~ | ✅ 21 `#[allow]`→`#[expect]`; `let _` tipado |
| `absolute_paths`, `min_ident_chars`, `excessive_nesting`, `missing_docs_in_private_items` | 183/1729/262/156 | **não adotadas** (ruído de domínio; ver `phase-3.md` §7) |
| ~~`pedantic` / `nursery` / `cargo`~~ | ✅ 489→0 | ligados em `warn`; gate `-D warnings` = zero; allowlist comentada |

> Regra de ouro: **nunca** desligar uma lint para "limpar" o CI. Ou o código se
> adequa, ou entra um `#[allow(.., reason = "…")]` pontual com justificativa.

## 6. `clippy.toml` — o que ele trava

- **Unsafe**: `// SAFETY:` colado ao bloco (`accept-comment-above-* = false`).
- **Erros**: `unwrap`/`expect`/`panic`/`dbg!`/`todo!`/`unimplemented!`.
- **Determinismo**: `HashMap`/`HashSet`/`LinkedList`, `env::var`,
  `SystemTime::now`, `Instant::now`, `process::exit`.
- **Memória**: `transmute`, `forget`, `zeroed`, `uninitialized`, `from_raw_parts`,
  `ptr::read/write`.
- **Recursos/tamanho**: `stack-size-threshold = 65536`,
  `array-size-threshold = 4096`, `future-size-threshold = 8192`,
  `pass-by-value-size-limit = 128`, `cognitive-complexity-threshold = 15`,
  `too-many-lines-threshold = 60`.
- **Concorrência**: guardas de lock/`Ref`/`RefMut` não atravessam `.await`.
- **Estilo**: `standard-macro-braces`, `warn-on-all-wildcard-imports`,
  `upper-case-acronyms-aggressive`.

## 7. Supply chain — `deny.toml`

`cargo deny check` valida quatro checks: advisories, licenses, bans e sources.
Licenças permitidas incluem `CC0-1.0` (licença do projeto) e `BSL-1.0`
(xxhash-rust). Versões duplicadas do mesmo crate geram **warning**; wildcard de
versão e registry/git desconhecidos são **erro**. O ban explícito de runtime
assíncrono do knudge **não** se aplica: o sniff é um cliente de rede
(CDP/VM Service) e depende de `tokio` por design.

A revisão periódica de necessidade/atualização de dependências (passo prévio à
documentação/release) está em [`docs/dependencies.md`](dependencies.md).

## 8. Política de exceções

- Todo `#[allow]` exige `reason` (`allow_attributes_without_reason = deny`).
- Prefira `#[expect]` (Edition 2024): se a lint parar de disparar, o `expect`
  vira erro e a supressão não fica zumbi.
- Onde possível, prefira `#[allow]` **local** (função/statement) a `allow` de
  workspace.

## 9. Diferenças deliberadas em relação ao knudge

| Item | knudge | sniff | Motivo |
|---|---|---|---|
| Runtime async | banido (`deny.toml`) | permitido (`tokio`) | cliente de rede CDP/VM Service |
| `doc-valid-idents` | termos de RAG/embeddings | termos de CSS/DOM/CDP/Flutter | domínio diferente |
| Faseamento | já no padrão final | fase 1 + débito mapeado | código legado |

## 10. Performance e benchmarks

O workstream D5 adiciona micro-benchmarks contínuos com `criterion` (dev-dep),
um por caminho quente: `diff` (`sniff-css-diff`), `check` (`sniff-css-check`),
`color` (`sniff-flutter`) e `ax` (`sniff-engine`). Todos usam
`harness = false` e passam pelo mesmo gate (`clippy --all-targets -D warnings`).

- Rode com `cargo bench -p <crate> --bench <name>`.
- O baseline e as instruções estão em [`docs/benchmarks.md`](benchmarks.md).
- Regressão acima de **5%** na mesma máquina exige justificativa no PR.

Os itens D2 (alocações/clones) e D3 (saída em buffer) foram absorvidos pelo A3:
os lints de alocação (`redundant_clone`, `needless_collect`, `str_to_string`,
`unnecessary_to_owned`, `needless_pass_by_value`) estão em `0`, e a serialização
já usa `BufWriter` + `serde_json::to_writer` sem `format!` intermediário.

## 11. Verificação dinâmica e CI

Além do gate estático (`fmt`/`clippy`/`test`/`deny`), o CI
(`.github/workflows/ci.yml`) roda:

- **`cargo audit`** — vulnerabilidades conhecidas (advisory database);
- **`cargo machete`** — dependências não usadas;
- **`cargo deny check`** — advisories/licenças/bans/sources;
- **Miri** (nightly) — smoke de UB em `sniff-core/tests/miri_smoke.rs`
  (`MIRIFLAGS=-Zmiri-disable-isolation`). O core é `#![forbid(unsafe_code)]`;
  aqui o Miri vale como sanidade de alocação/bounds/UTF-8;
- **fuzz** (nightly, `cargo-fuzz`) — alvos `jsonl_decode`
  (`sniff-css-diff`) e `jsonrpc_decode` (`sniff-cdp::decode_incoming`), com
  ~30 s de smoke por alvo.

O crate `fuzz/` é standalone (excluído do workspace em `Cargo.toml`), pois
depende de nightly + libfuzzer. O parser JSON-RPC virou uma função pura e total
(`decode_incoming`) justamente para ser fuzzável e testável.
