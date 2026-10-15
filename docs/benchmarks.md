# Benchmarks (fase 3 — workstream D5)

Micro-benchmarks contínuos com [`criterion`](https://docs.rs/criterion), para
travar o desempenho dos caminhos quentes e detectar regressões (> 5% exige
justificativa no PR).

## Como rodar

```bash
# todos os benches de um crate
cargo bench -p sniff-css-diff --bench diff
cargo bench -p sniff-css-check --bench check
cargo bench -p sniff-flutter --bench color
cargo bench -p sniff-engine --bench ax

# mais rápido (menos amostras/tempo)
cargo bench -p sniff-css-diff --bench diff -- --warm-up-time 1 --measurement-time 3 --sample-size 30
```

O harness é `harness = false` (criterion). Os dados de entrada são sintéticos e
gerados no próprio bench (sem I/O), para medir só o algoritmo.

## Cobertura

| Bench | Alvo | Por que importa |
|---|---|---|
| `sniff-css-diff/diff` | `diff_trees` (100/500/1000 nós) | caminho central do `sniff diff` |
| `sniff-css-check/check` | `check_uniformity` + `run_rules` (10/50/200 nós) | verificação offline determinística |
| `sniff-flutter/color` | `parse_flutter_color`, `parse_color_value`, `is_color_property` | normalização de cor do backend Flutter |
| `sniff-engine/ax` | `attach_ax` (50/500/2000 nós) | resolução das facetas da árvore de acessibilidade |

## Baseline

- **Máquina/ferramenta:** `rustc 1.99.0 (b940084d7)`, perfil `bench` (herda
  `release`: `lto = "fat"`, `codegen-units = 1`).
- **Coleta:** 2026-10-15, `--warm-up-time 1 --measurement-time 3 --sample-size 30`.
- **Valores (mediana):**

| Bench | Caso | Mediana |
|---|---|---:|
| `diff_trees` | 100 nós | 103.15 µs |
| `diff_trees` | 500 nós | 567.13 µs |
| `diff_trees` | 1000 nós | 1.4272 ms |
| `uniformity` | 10 nós | 15.795 µs |
| `rules` | 10 nós | 15.862 µs |
| `uniformity` | 50 nós | 70.577 µs |
| `rules` | 50 nós | 84.491 µs |
| `uniformity` | 200 nós | 275.35 µs |
| `rules` | 200 nós | 384.18 µs |
| `parse_flutter_color` | 4 formatos | 434.64 ns |
| `parse_color_value` | — | 99.261 ns |
| `is_color_property` | — | 1.3699 ns |
| `attach_ax` | 50 nós | 1.2024 µs |
| `attach_ax` | 500 nós | 14.415 µs |
| `attach_ax` | 2000 nós | 80.778 µs |

Os números absolutos variam por máquina; o contrato é a **comparação relativa**
antes/depois de uma mudança na mesma máquina.
