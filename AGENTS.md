# AGENTS.md — convenções de contribuição do sniff

> Guia para agentes/contribuidores que trabalham **no código deste repositório**
> (Rust, binário `sniff`). O bloco no fim (`<!-- sniff:start -->` ..
> `<!-- sniff:end -->`) é o protocolo de projeto-alvo que o `sniff init` grava em
> projetos que **usam** a ferramenta; ele é gerenciado/sobrescrito por
> `sniff init` — não edite à mão.

## Comandos

```bash
cargo fmt --all -- --check                               # rustfmt (config estrita)
cargo clippy --workspace --all-targets -- -D warnings     # lints (warnings = erro)
cargo test --workspace                                   # testes (doc-tests inclusos)
cargo deny check                                         # supply chain (advisories/licenças/bans/sources)
cargo audit                                              # CVEs das dependências
cargo machete                                            # dependências não usadas
cargo +nightly miri test -p sniff-core --test miri_smoke # UB (MIRIFLAGS=-Zmiri-disable-isolation)
cargo bench                                              # baseline criterion
```

Workspace: `crates/sniff-core` · `sniff-cdp` · `sniff-engine` · `sniff-flutter` ·
`sniff-css` (binário **`sniff`**) · `sniff-css-diff` e `sniff-css-check`
(**bibliotecas**; verbos `sniff diff`/`sniff check`) · `sniff-css-mcp`
(binário **`sniff-mcp`**).

- **MSRV/toolchain: Rust 1.99** (`rust-toolchain.toml`, `rust-version` e
  `clippy.toml::msrv` devem casar).
- Alvo de qualidade: fmt/clippy limpos + testes passando + `cargo deny check` ok.
- Política de lints, `clippy.toml`/`deny.toml` e o **débito da adoção em fases**
  estão em [`docs/quality.md`](docs/quality.md). Toda exceção exige
  `#[allow(.., reason = "…")]` (ou `#[expect]`).

## Superfície do binário (verbos)

Um binário **`sniff`** (modelo knudge: poucos verbos, muitos modos) +
**`sniff-mcp`** separado. Contrato em [`docs/cli-surface.md`](docs/cli-surface.md).

| Verbo | Código |
|---|---|
| `capture` | `sniff-css/src/cli/capture.rs` + `commands/capture.rs` |
| `init` | `cli/init.rs` + `commands/init.rs` (bloco em `sniff-core/src/agents.rs`) |
| `diff` / `check` | `cli/{diff,check}.rs` + `commands/{diff,check}.rs` (libs `sniff-css-diff`/`-check`) |
| `snapshots` | `cli/snapshots.rs` + `commands/snapshots.rs` |
| `prime` | `commands/prime.rs` (byte-idêntico por versão) |
| `self` | `cli/self_cmd.rs` + `commands/self_cmd.rs` |

Globais: `--json` (envelope `{success, command, data?, error{code,message,retryable}, warnings?}`),
`--log-level` (fechado), `--quiet`. Sem verbo = help (exit 0). Exit codes:
`2` inválido · `3` não encontrado · `5` I/O · `70` interno.

- **Operando = posicional** (`capture <URL>`, `diff <BASE> <HEAD>`, `check <SNAP>`,
  `init [DIR]`); nunca há flag equivalente.
- **Entrada estruturada**: `--params '<json>'` (uma operação), `--batch file.jsonl`
  (N; `-` = stdin), stdin/heredoc universal, `--dry-run`. `--params` ≡ lote de 1.
- **Saída/registro**: `--output summary|jsonl|...`, `--persist [PATH]`.
  Contrato completo em [`docs/cli-surface.md`](docs/cli-surface.md) §5–6.

- `capture` emite **stdout = dados** (JSONL), **stderr = logs**; `--json` força
  `--output json`. `prime` é inglês token-optimized; help é pt-BR.
- Fontes não-determinísticas só via pontos centralizados (`sniff-core::env`,
  `snapshot::utc_now`, `snapshot_file_name_at`, `sniff-cdp::detect_chrome`).

## Documentação — single source of truth

| Conteúdo | Vive em | Não duplicar em |
|---|---|---|
| Flags da CLI `sniff capture` | `docs/usage.md` | SKILL.md, ai-usage.md, llms.txt |
| Superfície de verbos (`prime`, `self`, help, envelope) | `docs/cli-surface.md` | README |
| `sniff diff` / `sniff check` | `docs/diff-checks.md` | ai-usage.md, SKILL.md |
| Auditoria de acessibilidade | `docs/accessibility.md` | ai-usage.md §5 |
| Docker / container | `docs/docker.md` (+ espelho do overview do Docker Hub em `docs/dockerhub.md`) | SKILL.md, README |
| Contrato de determinismo | `docs/golden-run.md` | — |
| Avaliação IA (prompt + schema) | `docs/eval-prompt.md` + `docs/sniff-eval.schema.json` | — |
| Arquitetura interna | `docs/architecture.md` | — |
| Configuração de qualidade (lints/toolchain/deny) | `docs/quality.md` | — |
| Plano da Fase 3 (rigor por eixo) | `docs/phase-3.md` | — |
| Benchmarks (baseline `criterion`) | `docs/benchmarks.md` | — |
| Dependências (necessidade + atualização) | `docs/dependencies.md` | — |
| Boas práticas de Rust (skill) | `.agents/skills/rust/SKILL.md` | — |
| Instalação / quickstart | `README.md` (manter enxuto: instalação + quickstart + índice) | — |
| Guia ativo para agentes de IA | `SKILL.md` (índice orientado a decisão; detalhes → docs/) | — |
| Índice para LLMs | `llms.txt` (magro, só fatos-chave + links) | — |

Regras:

- **Flag nova → edite só `docs/usage.md`** (captura) ou
  `docs/cli-surface.md` (verbo/global) e, se mudar o fluxo de IA, as seções
  correspondentes de `docs/ai-usage.md` (sem re-documentar a flag em tabelas
  paralelas). Depois mencione em `CHANGELOG.md`.
- `SKILL.md` é um **índice de decisão**, não uma referência: ~200-250 linhas,
  tudo que é detalhe aponta para `docs/`.
- Não use marcadores de versão tipo "Novo (0.3)" em docs de referência — isso
  é lugar do changelog.
- Manter `SKILL.md` sincronizado com `~/.config/opencode/skills/sniff-css/SKILL.md`
  (copie o arquivo após editar).

## Passo prévio à documentação/release

Antes de fechar um ciclo de documentação ou release, **revise as dependências**
(`docs/dependencies.md`): `cargo update`, `cargo machete`, `cargo deny check`,
`cargo audit`, `cargo tree -d`. Depois rode o gate completo
(`fmt`/`clippy`/`test`) e, quando mexer em parser/saída, Miri + benchmarks.

## Idioma

- Docs **humanos** (`README.md`, `docs/*.md`, `CHANGELOG.md`): pt-BR.
- Docs **consumidos por IA** (`SKILL.md`, `llms.txt`): inglês (mais fácil de
  alimentar agentes), com comandos/flags em código.

## Releases

- `git tag vX.Y.Z && git push origin vX.Y.Z` dispara
  `.github/workflows/release.yml`: compila `sniff` + `sniff-mcp` por arquitetura,
  publica o GitHub Release (tarball/zip + `sha256sums.txt`) e a imagem Docker.
- O `install.sh` instala `sniff` + `sniff-mcp` e **remove os binários legados**
  `sniffCSS*`.
- Antes de taggear: rode o gate completo + `cargo audit`/`machete`, Miri, e
  valide o empacotamento (`cargo build --release --workspace --bins`).

## Misc

- `ai-guides/` é um **submodule** (outro repo: `st-all-one/ai-guides`). Não
  duplicar a SKILL.md lá com conteúdo divergente — se precisar, sincronize via
  cópia e mantenha o stub apontando para este repo.
- `sniff/` (snapshots persistidos via `--persist`/MCP) é gerado e auto-
  ignorado pelo git — não commitar.
- Releases: `git tag vX.Y.Z && git push origin vX.Y.Z` (workflow
  `.github/workflows/release.yml`).

<!-- sniff:start -->
<!-- sniff:version: 1 -->
# sniff — computed styles for LLMs

`sniff capture` is the source of truth for layout/style — never guess CSS, capture it.
Cycle: `sniff capture` → `sniff check` → `sniff diff`; the AI reads only the delta.

- Capture: `sniff capture <URL> -s <CSS> [--depth N] [--output summary|jsonl|json]`
(default `summary`, token-lean). Diff/check need `--no-summary` JSONL snapshots.
- Diff: `sniff diff <BASE> <HEAD> [--tolerance 0.5] [--stats-only]` (deterministic, offline).
- Checks: `sniff check <SNAP> [--rules] [--uniform]` (contrast, target size, focus, alt).
- Batch/params: `--params '<json>'` (one op) or `--batch file.jsonl` (one op per line);
`-` reads stdin, so `sniff capture … --no-summary | sniff check -` works.
- Protocol: `sniff prime` (`--long` = full); `sniff self` = setup/completions/upgrade.

Common cases:
- Self-signed TLS: add `--ignore-certificate-errors` (or set `SNIFF_IGNORE_CERTIFICATE_ERRORS=1`).
- Auth: `--header "Name: Value"` on every request; persist a login with `--storage-state FILE`
(write it once with `--save-storage-state FILE`).
- Action-only UI (modal/dropdown): `--click SEL`, or ordered `--action "click:SEL:5000"`.
- Flutter/Dart app: `flutter://<device>` as `<URL>` + `--project DIR --depth N` (debug build only).
- Persist a snapshot: `--persist` writes `sniff/[domain]/[UTC]-[path]-[selector].jsonl`
(or `--persist PATH` for an explicit file).

Contract: stdout = data (JSONL), stderr = logs; `--json` = {success, command, data?, error{code,message,retryable}, warnings?}.
Exit: 2 invalid, 3 not_found, 5 io, 70 internal.
<!-- sniff:end -->
