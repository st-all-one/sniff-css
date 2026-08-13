# AGENTS.md

Convenções do repositório sniff-computed-style-rs (SniffCSS).

## Comandos

```bash
cargo test --workspace        # 262 testes (unit + integração; integração auto-skip sem Chrome)
cargo fmt --all -- --check    # rustfmt
cargo clippy --workspace --all-targets -- -D warnings
```

Workspace: `crates/sniff-core` · `sniff-cdp` · `sniff-engine` · `sniff-css`
(CLI `sniffCSS`) · `sniff-css-diff` · `sniff-css-check` · `sniff-css-mcp`.
MSRV 1.88. Alvo de qualidade: testes passando + fmt/clippy limpos.

## Documentação — single source of truth

| Conteúdo | Vive em | Não duplicar em |
|---|---|---|
| Flags da CLI `sniffCSS` | `docs/usage.md` | SKILL.md, ai-usage.md, llms.txt |
| `sniffCSS-diff` / `sniffCSS-check` | `docs/diff-checks.md` | ai-usage.md, SKILL.md |
| Auditoria de acessibilidade | `docs/accessibility.md` | ai-usage.md §5 |
| Docker / container | `docs/docker.md` (+ espelho do overview do Docker Hub em `docs/dockerhub.md`) | SKILL.md, README |
| Contrato de determinismo | `docs/golden-run.md` | — |
| Avaliação IA (prompt + schema) | `docs/eval-prompt.md` + `docs/sniffCSS-eval.schema.json` | — |
| Arquitetura interna | `docs/architecture.md` | — |
| Instalação / quickstart | `README.md` (manter enxuto: instalação + quickstart + índice) | — |
| Guia ativo para agentes de IA | `SKILL.md` (índice orientado a decisão; detalhes → docs/) | — |
| Índice para LLMs | `llms.txt` (magro, só fatos-chave + links) | — |

Regras:

- **Flag nova → edite só `docs/usage.md`** e, se mudar o fluxo de IA, as seções
  correspondentes de `docs/ai-usage.md` (sem re-documentar a flag em tabelas
  paralelas). Depois mencione em `CHANGELOG.md`.
- `SKILL.md` é um **índice de decisão**, não uma referência: ~200-250 linhas,
  tudo que é detalhe aponta para `docs/`.
- Não use marcadores de versão tipo "Novo (0.3)" em docs de referência — isso
  é lugar do changelog.
- Manter `SKILL.md` sincronizado com `~/.config/opencode/skills/sniff-css/SKILL.md`
  (copie o arquivo após editar).

## Idioma

- Docs **humanos** (`README.md`, `docs/*.md`, `CHANGELOG.md`): pt-BR.
- Docs **consumidos por IA** (`SKILL.md`, `llms.txt`): inglês (mais fácil de
  alimentar agentes), com comandos/flags em código.

## Misc

- `ai-guides/` é um **submodule** (outro repo: `st-all-one/ai-guides`). Não
  duplicar a SKILL.md lá com conteúdo divergente — se precisar, sincronize via
  cópia e mantenha o stub apontando para este repo.
- `sniffCSS/` (snapshots persistidos via `--persist`/MCP) é gerado e auto-
  ignorado pelo git — não commitar.
- Releases: `git tag vX.Y.Z && git push origin vX.Y.Z` (workflow
  `.github/workflows/release.yml`).
