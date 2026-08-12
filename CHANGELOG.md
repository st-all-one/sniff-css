# Changelog

Todos os lançamentos seguem [Semantic Versioning](https://semver.org/) e cada
versão publicada recebe uma tag `vX.Y.Z` no GitHub. Os binários de cada
arquitetura, o instalador e a imagem Docker são publicados a partir da mesma tag.

## [Unreleased]

### Added

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
