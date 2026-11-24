# Bancada A/B do `sniff`

Comparação global (antes vs depois) dos verbos **offline** `diff` e `check`,
sem dependências externas, sem rede e sem Chrome. O crate é standalone
(`[workspace]` vazio no `Cargo.toml`) e não participa do workspace principal.

## Comandos

```sh
cargo run --release --manifest-path bench/Cargo.toml -- help
cargo run --release --manifest-path bench/Cargo.toml -- all \
  --before target/release/sniff-antigo \
  --after  target/release/sniff \
  --samples 9 --out bench/AB.md --json bench/AB.json
```

Modos: `all` (padrão), `diff`, `check`, `help`. `--samples N` controla o
número de amostras cronometradas (padrão 9); sempre há 1 warm-up por binário.
`--out` e `--json` definem os arquivos de relatório (padrões `bench/AB.md` e
`bench/AB.json`). O relatório também é impresso no stdout.

## Carga de trabalho

- `diff`: usa os fixtures reais
  `crates/sniff-css-diff/tests/fixtures/{base,head}.jsonl` (árvores com nós
  alterados, adicionados e jitter subpixel).
- `check`: nenhum snapshot existente serve de carga útil: os golden files de
  `sniff-engine` falham com exit 5 (misturam layouts tree/flat) e
  `examples/example.jsonl` carrega sem disparar regras. Por isso o harness
  gera um snapshot sintético determinístico (LCG com semente fixa
  `0xC0FFEE1234567890`, sem tempo, env, rede ou Chrome) com 24 cards e 6
  botões sem nome acessível, enviado por stdin. O conteúdo é o mesmo nos dois
  lados da comparação.

Cada amostra executa o binário real com `std::process::Command`, exige exit 0,
captura stdout+stderr e compara stdout e stderr byte a byte entre antes e
depois; se houver divergência, a bancada falha (não mediria o mesmo trabalho).
O resumo de cada verbo traz min, mediana e p95 (rank `ceil(0.95*N)`, limitado
ao máximo amostrado) em ns no JSON e em ms no Markdown.

## Perfis por função

```sh
cargo run --release --manifest-path bench/Cargo.toml -- all \
  --before target/release/sniff-antigo --after target/release/sniff \
  --before-profile /tmp/antes.jsonl --after-profile /tmp/depois.jsonl
```

Cada linha dos arquivos é `{"function":"nome","elapsed_ns":123}`. O produtor
desses perfis é a instrumentação da biblioteca, ligada por `SNIFF_PROFILE=1`
(implementada no código da biblioteca; a bancada apenas consome os JSONL).
Funções ausentes de um lado aparecem como `-`/`null`. A tabela de perfis é
ordenada por nome (BTreeMap/BTreeSet), com mediana agrupada por função.

## Contrato de saída

- stdout: tabela Markdown, divisor `--- JSON ---` e o relatório JSON.
- `bench/AB.md`: tabela Markdown com tempos, equivalência de saída
  (bytes + FNV-1a 64) e, quando presentes, perfis por função.
- `bench/AB.json`: `{"schema":1,"before","after","samples","warmup",
  "stdout_identical","verbs":[{"verb","before","after","stdout_bytes",
  "stdout_fnv1a64"}],"profiles":{"before","after","functions":[...]}}`.

Falha de exit status, stdout divergente ou perfil inválido termina com código
de saída 1 e mensagem em stderr; argumentos inválidos terminam com código 2.
