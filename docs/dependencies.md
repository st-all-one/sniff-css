# Dependências — revisão de necessidade e atualização

Política e inventário das dependências do projeto. Antes de fechar qualquer
ciclo de documentação/release, esta revisão é o **passo prévio**: cada
dependência precisa continuar necessária, atualizada (dentro do possível) e
licenciada.

## 1. Princípios

1. **Necessidade:** nenhuma dependência sem uso real. `cargo machete` roda no
   CI e falha se houver dep não usada; toda dep nova exige justificativa no PR.
2. **Minimalismo de features:** `default-features = false` + features explícitas
   onde reduz árvore (ex.: `tokio-tungstenite` só com `connect`/`handshake`).
3. **MSRV:** Rust **1.99** (`rust-version` + `clippy.toml::msrv`). Toda versão
   escolhida precisa rodar em 1.99.
4. **Determinismo:** dependências não podem introduzir iteração não
   determinística na saída (ver `docs/golden-run.md`).
5. **Licenças:** só o conjunto permitido em `deny.toml` (MIT/Apache-2.0/BSD/
   ISC/Unicode/Zlib/BSL-1.0/CC0-1.0/MPL-2.0).
6. **Sem atalhos de supply chain:** `unknown-registry`/`unknown-git` em `deny`;
   registry único (crates.io).

## 2. Dependências diretas (runtime)

| Crate | Versão | Para quê | Onde |
|---|---:|---|---|
| `tokio` | 1.53.3 | runtime async (rede CDP/VM Service, processos) | cdp, engine, flutter, css, mcp |
| `tokio-tungstenite` | 0.30.0 | WebSocket (CDP e Dart VM Service); `default-features=false` + `connect`/`handshake` | cdp |
| `futures-util` | 0.3.34 | `SinkExt`/`StreamExt` no split do WS | cdp, flutter(tests) |
| `serde` | 1.0.229 | derives de (de)serialização | core |
| `serde_json` | 1.0.154 | JSON / JSONL (`preserve_order`) | core, cdp, engine, diff, check, flutter, css, mcp |
| `thiserror` | 2.0.21 | erros tipados das libs | core, diff, flutter, mcp |
| `anyhow` | 1.0.104 | erro nas fronteiras binárias (`main`) | css, check, diff |
| `clap` | 4.6.7 | parsing de CLI (derive) | css, check, diff, mcp |
| `tracing` | 0.1.44 | eventos estruturados | flutter, mcp |
| `tracing-subscriber` | 0.3.23 | instalação do subscriber (stderr) | css, mcp |
| `base64` | 0.23.1 | decode de screenshot CDP | cdp |
| `rmcp` | 3.5.1 | SDK do Model Context Protocol; `transport-io` + `client` (test) | mcp |
| `schemars` | 1.2.3 | JSON Schema das tools MCP | mcp |
| `xxhash-rust` | 0.8.19 | hash xxh3 da impressão digital de estilos; `xxh3` | engine |

## 3. Dependências de desenvolvimento/ferramentas

| Crate | Versão | Para quê |
|---|---:|---|
| `criterion` | 0.8.2 | micro-benchmarks (`benches/`, `harness = false`) |
| `libfuzzer-sys` | 0.4 | alvos de fuzz (`fuzz/`, crate standalone, nightly) |

Ferramentas de CI (não são deps do crate): `cargo-deny`, `cargo-audit`,
`cargo-machete`, `typos`, `cargo-fuzz`, Miri.

## 4. Como revisar

```bash
cargo machete            # necessidade: deps não usadas
cargo tree -d            # versões duplicadas (majors distintos)
cargo deny check         # advisories + licenças + bans + sources
cargo audit              # vulnerabilidades conhecidas
cargo update --dry-run   # atualizações compatíveis disponíveis
cargo update             # aplica as compatíveis (Cargo.lock)
```

Para além do semver, `cargo search <crate> --limit 1` mostra a última versão.
Bump de major exige o gate completo (fmt/clippy/test/deny + Miri) e, quando
mexe no parser/saída, comparação com `docs/benchmarks.md` e o golden
(`docs/golden-run.md`).

## 5. Estado da última revisão

- **Data:** 2026-10-15.
- **Atualizações compatíveis aplicadas** (`cargo update`, 50 pacotes): `tokio`
  1.53.1→1.53.3, `rmcp`/`rmcp-macros` 3.1.2→3.5.1, `schemars` 1.2.2→1.2.3,
  `serde_json` 1.0.151→1.0.154, `clap` 4.6.6→4.6.7, `thiserror` 2.0.20→2.0.21,
  `futures-*` 0.3.33→0.3.34, `xxhash-rust` 0.8.18→0.8.19, entre outros.
- **Bump semver-incompatível avaliado:** `tokio-tungstenite` 0.26 → **0.30**
  (tungstenite 0.26.2 → 0.30.0). Compila **sem mudança de código** (a API usada
  é `connect_async`/`accept_async`/`Message`), gate verde → adotado.
- **Necessidade:** `cargo machete` limpo. Na revisão foram **removidas** deps
  não usadas: `futures-util`/`serde`/`thiserror`/`tracing` (`sniff-engine`),
  `serde` (`sniff-flutter`) e `tracing` (`sniff-css`).
- **Duplicatas:** só `syn` 2.x/3.x (proc-macros) e `getrandom` 0.3/0.4
  (transitivas) — `multiple-versions = "warn"`, sem ação.
- **Licenças/advisories:** `cargo deny check` e `cargo audit` limpos.

Próxima revisão: a cada ciclo de release (ou quando o CI apontar advisory).
