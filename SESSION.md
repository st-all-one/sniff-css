# SESSION.md

## Estado: FECHADO — v0.5.1 encerrada (2026-10-15)

A **v0.5.1** (workspace em `0.5.1`) é a continuação da superfície de API iniciada
na 0.5.0: a CLI ganha a **gramática de operandos** e os **canais de máquina**
(entrada/saída estruturada) do knudge, sem alterar o contrato de máquina
(MCP/`--json`/exit codes).

### O que entrou na v0.5.1

- **Operandos posicionais** — `sniff capture <URL>`, `sniff check <SNAP>`,
  `sniff init [DIR]` (`sniff diff <BASE> <HEAD>` já seguia); aliases legados
  `-u/--url`, `-i/--input`, `--dir` (avisam em stderr).
- **stdin/heredoc** — `-` em qualquer entrada; operando ausente + stdin não-TTY
  é preenchido pelo stdin; `sniff capture … --no-summary | sniff check -`.
- **`--params`** (uma operação; `-` = stdin) e **`--batch`** (JSONL, N
  operações; best-effort, teto `sniff.batch_max=100`), com schema `snake_case`
  compartilhado com o MCP/`--json`; **`--dry-run`**.
- **`--persist [PATH]`** + `--output` explícito; o lote registra cada payload
  antes da linha de status.
- **Licença** — de `CC0-1.0` para **`MIT OR Apache-2.0`** (padrão do knudge).
- Docs: `docs/cli-surface.md` §5–6, `docs/usage.md` (entrada estruturada),
  README, SKILL, AGENTS, `prime` e o bloco do `sniff init`.
- Gate: fmt ✓, clippy ✓, **439 testes** (0 falhas), deny/audit/machete ✓, Miri ✓.

## Estado: FECHADO — v0.5.0 encerrada (2026-10-15)

Todas as frentes desta etapa foram concluídas, validadas e documentadas. A
**v0.5.0** (workspace em `0.5.0`) é o fechamento da **fase de qualidade e
estabilidade**: base **Rust 1.99 / Edition 2024**, toolchain de rigor e
determinismo como padrão. Nenhuma mudança de comportamento de captura foi
introduzida de propósito — o foco foi rigor, previsibilidade e supply chain.

### O que entrou na v0.5.0

- **Superfície da CLI (modelo knudge)** — binário `sniff` único com verbos
  (`capture`, `init`, `diff`, `check`, `snapshots`, `prime`, `self`, `help`);
  `sniff-mcp` separado; `--json`/`--log-level`/`--quiet` globais; envelope de
  máquina e códigos de saída; `sniff prime` estático; `sniff self`. O
  instalador remove os binários legados `sniffCSS*`. Contrato:
  `docs/cli-surface.md`.
- **Toolchain** — `rust-toolchain.toml` (canal `1.99`), `rust-version` e
  `clippy.toml::msrv` alinhados; crates em Edition 2024 (resolver v3);
  `rustfmt.toml` e `clippy.toml` no rigor máximo.
- **Lints (fase 3, eixos A–D)** — grupos `pedantic`/`nursery`/`cargo` ligados e
  restriction curada; em `deny`: `unwrap`/`expect`/`panic`/`unreachable`,
  `print_*`, `indexing_slicing`, `arithmetic_side_effects`,
  `disallowed_methods`/`disallowed_types`/`disallowed_macros`,
  `as_conversions` + `cast_*`, `undocumented_unsafe_blocks`,
  `allow_attributes(_without_reason)`, `let_underscore_*`.
- **Determinismo** — `HashMap`/`HashSet` → `BTreeMap`/`BTreeSet`; ordenações
  por `f64::total_cmp`/`to_bits`; fontes não-determinísticas centralizadas
  (`sniff_core::env`, `utc_now`, timestamps injetáveis).
- **Segurança** — zero `unsafe` (`#![forbid(unsafe_code)]` no núcleo); auditoria
  de concorrência; `cargo deny`/`audit`/`machete`.
- **Desempenho** — iteradores sem índice, casts auditados em
  `sniff_core::numeric`, saída em `BufWriter`, benchmarks `criterion`.
- **CI** — `fmt`/`clippy`/`test`/`deny`/`audit`/`machete`/`typos` + Miri (smoke)
  + fuzz (JSONL/JSON-RPC).
- **Dependências** — `cargo update` (50 crates), `tokio-tungstenite` 0.26→0.30,
  deps não usadas removidas.

### Validação

- Gate completo verde: `fmt`, `clippy --all-targets -D warnings` (incl.
  pedantic/nursery/cargo), toda a suíte de testes, `cargo deny`, `cargo audit`,
  `cargo machete`.
- **Miri** (`sniff-core/tests/miri_smoke.rs`): 3 testes, sem UB.
- **Benchmarks**: baseline em `docs/benchmarks.md`.
- **Fuzz**: `cargo fuzz build` validado no CI (local exige compilador C++).

### Docs do fechamento

- `CHANGELOG.md` — `[Unreleased]` consolidado em `[0.5.0] — 2026-10-15`
  (Changed/Added/Fixed), sem `Unreleased`.
- `docs/quality.md` — política de lints/toolchain/exceções, performance e
  verificação dinâmica.
- `docs/phase-3.md` — plano do rigor por eixo (A/B/C/D), concluído.
- `docs/benchmarks.md` — baseline `criterion`.
- `docs/dependencies.md` — revisão de necessidade/atualização (passo prévio a
  cada release).
- `docs/cli-surface.md` — contrato da superfície de verbos (`prime`/`self`/help/envelope).
- `install.sh`, `SKILL.md`, `docs/usage.md`, `docs/docker.md` e
  `docker/Dockerfile` — exemplos de versão para `v0.5.0`.

### Notas / armadilhas (relevantes para a próxima feature)

- **NÃO commitar**: `conversations/` (untracked legado), `sniff/`.
- AVD é `sniff`; para actions use emulador **com janela** (`-gpu host`, sem
  `-no-window`); `adb root` + `setenforce 0`.
- Pegadinha de shell: selector com `'` (ex. `FilledButton-[<'counter'>][0]`)
  deve ir entre **aspas duplas** — aspas simples quebram e vira `ByText`.
- `adb kill-server`/`start-server` pode dessincronizar e "perder" o emulador.
- Rects de filhos de `Flex` têm offset 0 (limitação do `getLayoutExplorerNode`)
  — actions usam o finder do driver, nunca `rect`.
- Release: `git tag v0.5.0 && git push origin v0.5.0` (workflow
  `.github/workflows/release.yml` + `docs/docker.md`).
