# A/B `sniff` (verbos offline)

- antes: `/tmp/sniff-ab-before-target/release/sniff`
- depois: `target/release/sniff`
- amostras por verbo: 15 (+1 warm-up)
- p95: maior entre as amostras quando N <= 20 (rank `ceil(0.95*N)`)

## Tempo por verbo

| verbo | antes min (ms) | antes mediana (ms) | antes p95 (ms) | depois min (ms) | depois mediana (ms) | depois p95 (ms) | delta mediana |
|---|---:|---:|---:|---:|---:|---:|---:|
| diff | 2.651 ms | 2.889 ms | 3.891 ms | 2.682 ms | 2.900 ms | 3.194 ms | +0.38% |
| check | 4.459 ms | 4.864 ms | 5.578 ms | 4.211 ms | 4.892 ms | 5.545 ms | +0.55% |

## Equivalência de saída

| verbo | stdout antes == depois | bytes | FNV-1a 64 |
|---|---|---:|---|
| diff | sim | 824 | 6b01f89955fdac0e |
| check | sim | 23201 | b086f597cbf17d45 |

## Perfis por função (mediana, ns)

| função | antes | depois | delta |
|---|---:|---:|---:|
| diff_actions | - | 1885 | - |
| diff_trees | - | 12711 | - |

Perfis produzidos pela instrumentação da biblioteca com `SNIFF_PROFILE=1`.
