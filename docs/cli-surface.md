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

## 5. `sniff capture`

Captura estilos computados de uma página (web) ou a árvore de widgets de um app
Flutter. O default é otimizado para IA (`compact`, `custom-props`, `stabilize`,
`contrast`, `ax` ligados). O selector vem de `-s/--selector`; `flutter://<device>`
ativa o backend Flutter (selector default `root`).

Web (CDP): `-u/--url`, `-s/--selector`, `--depth`, `-c/--categories`, `--props`,
`--pseudo`, `--wait`, `--action`/`--click`/`--hover`/`--type`/`--upload`,
`--header`, `--storage-state`/`--save-storage-state`, `--connect`/`--chrome`,
`--viewport`, `--ignore-certificate-errors`, `--output`, `--screenshot`,
`--fullpage-screenshot`, `--persist`, `--attrs`, `--stable-key`, `--full` e os
`--no-*`.

Flutter: `--backend flutter|auto`, `--device`, `--avd`, `--project`, `--target`,
`--attach`.

Referência completa das flags: [`usage.md`](usage.md).

## 6. `sniff diff` / `sniff check` / `sniff snapshots`

```
sniff diff <BASE> <HEAD> [--tolerance 0.5] [--ignore-props a,b] [--no-structural] [--stats-only] [--no-actions]
sniff check -i <JSONL> [--rules] [--uniform] [--tolerance 0.5]
sniff snapshots [--root DIR]      # default: SNIFF_SNAPSHOT_DIR ou ./sniff
```

`diff`/`check` são **offline e determinísticos** (nunca abrem browser). `--json`
devolve o resumo estruturado; o texto segue no stdout e o resumo em stderr.

## 7. `sniff init`

```
sniff init [--force] [--dir DIR]
```

Garante o **bloco gerenciado** do sniff no `AGENTS.md` do projeto (raiz do git
a partir do CWD, ou `--dir`). O bloco fica entre `<!-- sniff:start -->` e
`<!-- sniff:end -->` e é escrito de forma **idempotente**, preservando tudo que
está fora dele — mesma segurança do `kd init`. `--force` **sobrescreve o arquivo
inteiro** com o bloco (reset destrutivo, explícito).

O bloco é um protocolo enxuto em inglês, focado no que importa: ciclo
`capture → check → diff`, flags essenciais e os casos de uso mais comuns
(TLS self-signed com `--ignore-certificate-errors`, auth por `--header`,
sessão persistente, ações, Flutter, persist).

## 8. `sniff prime` e `sniff self`

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
