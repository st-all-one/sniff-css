# Superfície da CLI (`sniff`) — contrato

> Inspirada no `kd` (knudge): **poucos verbos, muitos modos**, protocolo estático
> (`prime`), utilitários do binário (`self`) e help otimizado. Este documento é o
> **contrato congelado**: nomes, agrupamento e o contrato de máquina.

## 1. Princípios

1. **Poucos verbos, muitos modos.** O verbo agrupa; flags e subcomandos carregam o resto.
2. **`prime` é estático.** Protocolo byte-idêntico por versão (cacheável); `--long` detalha.
3. **stdout = dados, stderr = logs.** `--json` é o contrato de máquina (envelope).
4. **Sem verbo = help**, exit 0. `sniff help <verbo>` mostra o verbo.
5. **Conjuntos fechados listam as opções** e rejeitam valor inválido com a lista.
6. **Operando = posicional; opção = flag.** O objeto do verbo é sempre o
   argumento posicional (`capture <URL>`, `diff <BASE> <HEAD>`, `check <SNAP>`,
   `init [DIR]`); não existe flag equivalente ao operando. Ver §5.
7. **Um eixo de entrada e um de saída.** Entrada estruturada (`--params`,
   `--batch`, stdin) e saída explícita (`--output`, `--persist`) são canais de
   máquina paralelos à interface humana. Ver §5 e §6.

## 2. Verbos de topo

| Verbo | Absorve | Papel |
|---|---|---|
| `sniff` (sem args) | = `sniff help` | help completo, exit 0 |
| `sniff init` | — | grava/atualiza o bloco do `AGENTS.md` do projeto |
| `sniff capture` | flags de captura (web CDP / Flutter VM Service) | fonte da verdade dos estilos |
| `sniff diff` | `sniffCSS-diff` | diff determinístico entre dois JSONL |
| `sniff check` | `sniffCSS-check` | contraste/alvo/foco/alt + uniformidade |
| `sniff snapshots` | tool MCP `sniff_snapshots` | lista snapshots persistidos |
| `sniff prime` | — | protocolo de uso estático (`--long`) |
| `sniff self` | — | `setup` / `completions` / `upgrade` / `version` |
| `sniff help [verbo]` | — | ajuda |

`sniff-mcp` continua um **binário separado** (como `knudge-mcp`), porque clientes
MCP configuram um comando dedicado; ele expõe as tools `sniff_page`,
`sniff_diff`, `sniff_check`, `sniff_snapshots`, `sniff_categories` e os recursos
`sniff://prompts/eval`, `sniff://schemas/eval`, `sniff://guides/golden`.

## 3. Globais

```
sniff [--json] [--log-level error|warn|info|debug|trace|off] [--quiet] <VERBO>
```

- `--json`: envelope de máquina `{success, command, data?, error{code,message,retryable}, warnings?}`.
  Em `capture`, força a saída `--output json` (array único).
- `--log-level` / `--quiet`: filtro de log em stderr (fechado; default `warn`).

## 4. Códigos de saída

| Código | Classe |
|---|---|
| `0` | sucesso |
| `2` | entrada inválida (`invalid`) |
| `3` | não encontrado (`not_found`) |
| `5` | I/O (`io`) |
| `70` | interno (`internal`) |

## 5. Operandos, stdin/heredoc e execução em lote

### 5.1 Operandos (posicionais)

| Verbo | Operando | Cardinalidade |
|---|---|---|
| `capture` | `<URL>` (http(s):// ou flutter://<device>) | 1 |
| `diff` | `<BASE> <HEAD>` | 2 |
| `check` | `<SNAP>` | 1 |
| `init` | `[DIR]` | 0..1 (default: raiz do git) |
| `snapshots`, `prime`, `self …` | — | 0 (rejeitam posicional) |

Operando **nunca** tem flag equivalente. Os nomes antigos seguem como aliases
ocultos depreciados por um ciclo: `-u/--url`, `-i/--input`, `--dir`.

### 5.2 stdin / heredoc

Regra única: **`-` é stdin** em qualquer canal de entrada (operando de arquivo,
`--params`, `--batch`); **operando obrigatório ausente + stdin não-TTY → lê do
stdin**; ausente + TTY → exit 2.

```bash
sniff capture https://x -s body --no-summary | sniff check -   # pipe
sniff check - < snap.jsonl
sniff diff base.jsonl - < head.jsonl
sniff check - <<'EOF'          # heredoc (o shell entrega o EOF)
{"__meta":{"viewport":{"width":1366,"height":768}}}
EOF
cat params.json | sniff capture   # stdin vira o objeto de params
```

### 5.3 `--params` (uma operação) e `--batch` (N operações)

`--params '<json>'` recebe **um objeto** com todos os campos do verbo
(`snake_case`, iguais aos do MCP e do `--json`); `--params -` lê de stdin.
`--batch <FONTE|->` lê **JSONL**, uma operação por linha. **`--params` é o lote
de 1** — mesmo parser. As chaves de `capture` incluem `url`, `selector`,
`depth`, `categories`, `output`, `persist`, `wait`, `actions`, `header`, … as
mesmas das flags.

```bash
sniff capture --params '{"url":"https://x","selector":".btn","depth":2}'
sniff check   --params '{"input":"snap.jsonl","rules":true}'
sniff diff    --params '{"base":"a.jsonl","head":"b.jsonl"}'

sniff capture --batch jobs.jsonl     # uma URL por linha
sniff capture --batch - <<'EOF'
{"url":"https://x","selector":".btn"}
{"url":"https://x","selector":".card","depth":3}
EOF
```

- **Best-effort**: linha inválida vira `warnings[]` (stderr) e o item um status
  com erro; o lote **continua**.
- **Teto**: `sniff.batch_max=100` (excedeu → exit 2).
- **Saída do lote**: uma linha JSONL por item em stdout
  (`{"index","ok","path"?,"error"?}`); `--json` agrega em `data`.
- `--dry-run`: valida cada item sem tocar no browser.
- **Exclusividade**: posicional + `--params`/`--batch`, ou `--params` + `--batch`
  → exit 2 (fonte duplicada).

## 6. Saída e registro

- `--output <summary|jsonl|jsonl-flat|json>`: modo explícito do payload de
  `capture` (default `summary`). `--no-summary`/`--summary` são aliases ocultos
  de `--output jsonl`/`summary`.
- `--persist [PATH]`: registra o JSONL completo. Sem valor → store gerenciado
  (`sniff/[domínio]/[UTC]-[path]-[selector].jsonl`); com `PATH` → arquivo
  indicado. Não afeta o stdout. No `--batch`, cada item persiste antes da linha
  de status.
- `sniff snapshots [--root DIR]` lista o registro persistido.

## 7. `sniff capture`

```
sniff capture [OPTIONS] <URL> [--output MODO] [--persist [PATH]]
              [--params JSON|-] [--batch FONTE|-] [--dry-run]
```

Captura estilos computados de uma página (web) ou a árvore de widgets de um app
Flutter. O default é otimizado para IA (`compact`, `custom-props`, `stabilize`,
`contrast`, `ax` ligados). O selector vem de `-s/--selector`; `flutter://<device>`
ativa o backend Flutter (selector default `root`).

Web (CDP): `-s/--selector`, `--depth`, `-c/--categories`, `--props`,
`--pseudo`, `--wait`, `--action`/`--click`/`--hover`/`--type`/`--upload`,
`--header`, `--storage-state`/`--save-storage-state`, `--connect`/`--chrome`,
`--viewport`, `--ignore-certificate-errors`, `--output`, `--screenshot`,
`--fullpage-screenshot`, `--persist`, `--attrs`, `--stable-key`, `--full` e os
`--no-*`.

Flutter: `--backend flutter|auto`, `--device`, `--avd`, `--project`, `--target`,
`--attach`.

Referência completa das flags: [`usage.md`](usage.md).

## 8. `sniff diff` / `sniff check` / `sniff snapshots`

```
sniff diff <BASE> <HEAD> [--tolerance 0.5] [--ignore-props a,b] [--no-structural] [--stats-only] [--no-actions] [--params|--batch]
sniff check <SNAP> [--rules] [--uniform] [--tolerance 0.5] [--params|--batch]
sniff snapshots [--root DIR]      # default: SNIFF_SNAPSHOT_DIR ou ./sniff
```

`diff`/`check` são **offline e determinísticos** (nunca abrem browser) e aceitam
o snapshot por stdin. `--json` devolve o resumo estruturado; o texto segue no
stdout e o resumo em stderr.

## 9. `sniff init`

```
sniff init [DIR] [--force] [--params|--batch]
```

Garante o **bloco gerenciado** do sniff no `AGENTS.md` do projeto (raiz do git
a partir do CWD, ou `[DIR]`). O bloco fica entre `<!-- sniff:start -->` e
`<!-- sniff:end -->` e é escrito de forma **idempotente**, preservando tudo que
está fora dele — mesma segurança do `kd init`. `--force` **sobrescreve o arquivo
inteiro** com o bloco (reset destrutivo, explícito).

O bloco é um protocolo enxuto em inglês, focado no que importa: ciclo
`capture → check → diff`, flags essenciais e os casos de uso mais comuns
(TLS self-signed com `--ignore-certificate-errors`, auth por `--header`,
sessão persistente, ações, Flutter, persist).

## 10. `sniff prime` e `sniff self`

```
sniff prime [--long]
sniff self setup <claude|cursor|codex|pi>
sniff self completions <bash|zsh|fish>
sniff self upgrade [--dry-run] [--version TAG] [--script PATH]
sniff self version
```

`prime` é a "ajuda da IA": compacto por padrão, byte-idêntico por versão.
`self setup` grava a recipe em `.sniff/setup/<cliente>.json` e a imprime;
`self upgrade` roda o instalador oficial (`install.sh`), verificável com
`--dry-run`.
