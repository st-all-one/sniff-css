# Release

A documentação de referência da
superfície de verbos fica em [`cli-surface.md`](cli-surface.md); aqui só vivem
versão, gates, artefatos e o procedimento de tag.

## 1. Versionamento

- Versão única do workspace em `Cargo.toml` (`[workspace.package].version`);
  os 8 crates herdam. `Cargo.lock` é regenerado por `cargo check`.
- `env!("CARGO_PKG_VERSION")` alimenta `sniff self version`, `sniff prime` e o
  bloco gerado por `sniff init` — não há constantes de versão espalhadas.
- A tag `vX.Y.Z` é o contrato de release: o instalador e o Dockerfile
  derivam o nome do artefato dela (`sniff-css-X.Y.Z-<target>.tar.gz`).
- `CHANGELOG.md` é a fonte de verdade do que mudou. `README.md`, `SKILL.md`,
  `llms.txt` e `docs/` não usam marcadores de versão em texto de referência.

## 2. Gates locais antes da tag

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check && cargo audit && cargo machete
typos --config _typos.toml .
MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test -p sniff-core --test miri_smoke
cargo build --release --workspace --bins
```

`_typos.toml` registra o vocabulário pt-BR legítimo; a ferramenta `typos`
(quando usada localmente) lê o mesmo arquivo. `typos` reprovava palavras como
`erro`, `fase`, `seletor` e `credencial` — corrigir o texto para inglês
quebraria a documentação; a solução é o dicionário de exceções.

Os gates acima são **locais**: o único workflow do repositório é o de release,
disparado por tag `v*`. Não há validação automática em push/PR contra a `main`.

## 3. Workflows

- **`.github/workflows/release.yml`** — em `push` de tag `v*` (ou disparo
  manual via `workflow_dispatch`): 6 targets
  (linux gnu/musl × x86_64/aarch64, macOS arm64, Windows x86_64), 2 binários
  por target, `sha256sums.txt` agregado, GitHub Release e imagem Docker
  multi-arch (`stallonels/sniffcss:<tag>` + `:latest`).

### Estado observado das tags 0.6.x

- `v0.6.0` — release anterior com o contrato v2 completo (macro de protocolo
  antigo, summary ainda pagava as fatias).
- `v0.6.1` (`ca27166`) — publicada em 2026-11-18 com 13 assets (6 tarballs/
  zip, 6 checksums e `sha256sums.txt`) e runner `macos-15`. O job `release`
  e `check` passaram; `supply-chain`, `miri` e `fuzz` falharam naquela
  execução (typos em pt-BR sem config, setup do toolchain nightly).
- **`v0.6.3` (publicada)** — tag assinada em 2026-11-23: correções de
  `--ax-tree`/`--minimal`, rejeição de `summary` em `check`/`diff`, curadoria
  de regras e contagem de 40 props. O job `check` do CI passou a executar
  `fmt`/`clippy`/`test` de fato.
- **`v0.6.4` (em andamento, sem tag)** — workspace, lockfile, README, SKILL e
  CHANGELOG já apontam para esta versão; upgrade da MSRV para Rust 1.100,
  lints nativos do Cargo 1.100, instrumentação por função (`SNIFF_PROFILE`)
  e bancada A/B global. Publicar após os gates da §2 com
  `git tag -s v0.6.4` + `git push origin v0.6.4`.

## 4. Publicar

```bash
# 1. conferir que HEAD contém o bump de versão e o CHANGELOG da release
grep -n '^version' Cargo.toml
head -20 CHANGELOG.md

# 2. gates da §2

# 3. tag assinada (o workflow dispara no push)
git tag -s vX.Y.Z -m "vX.Y.Z"
git push origin vX.Y.Z
```

Não publiques uma tag sobre um commit que já tenha outra tag de release: o
GitHub Release e o Docker Hub usam o nome da tag como chave de artefato.

## 5. Pós-tag

```bash
# artefatos e checksums da release
curl -fsSL https://api.github.com/repos/st-all-one/sniff-css/releases/tags/vX.Y.Z \
  | jq -r '.assets[].name'

# instalação pinned + verificação de versão
curl --proto '=https' --tlsv1.2 -sSf \
  https://raw.githubusercontent.com/st-all-one/sniff-css/main/install.sh \
  | VERSION=vX.Y.Z sh
sniff self version
```

O instalador escolhe o triple por libc no Linux e valida o SHA-256 antes de
instalar; `SNIFF_TARGET`/`sniff self upgrade --target TRIPLE` forçam outro
artefato. O Dockerfile baixa o tarball musl correspondente a `TARGETARCH`.
