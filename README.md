<div align="center">

# sniff

Computed styles reais e árvore de widgets Flutter, determinísticos, para agentes de IA — captura → diff → checks.

[![License](https://img.shields.io/badge/license-CC0--1.0-2b3a42?style=for-the-badge)](./LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.99%2B-DEA584?style=for-the-badge&logo=rust&logoColor=000)](https://www.rust-lang.org/)
[![MCP](https://img.shields.io/badge/MCP-JSON--RPC%202.0-6E56CF?style=for-the-badge)](docs/ai-usage.md)
[![Version](https://img.shields.io/badge/version-0.5.1-009739?style=for-the-badge)](CHANGELOG.md)

</div>

O sniff é uma CLI em Rust que lê o **computed style real** dos elementos de uma
página via **Chrome DevTools Protocol (CDP)** — e a **árvore de widgets** de apps
**Flutter/Dart** nativos via **Dart VM Service** — emitindo JSONL compacto e
determinístico. Em vez de screenshots e julgamento visual, o pipeline mede:
**`sniff capture`** captura, **`sniff diff`** devolve só o que mudou,
**`sniff check`** roda regras de acessibilidade/consistência offline e
**`sniff-mcp`** expõe tudo a agentes de IA. `sniff init` grava o protocolo de uso
no `AGENTS.md` do projeto e `sniff prime` é o protocolo token-optimized para a
IA. A IA interpreta apenas o delta.

## Instalação

Release pré-compilado, verificado por SHA-256, que instala `sniff` e
`sniff-mcp` em `~/.local/bin` (e remove os binários legados `sniffCSS*`):

```bash
curl --proto '=https' \
     --tlsv1.2 \
     --show-error \
     --fail \
  https://raw.githubusercontent.com/st-all-one/sniff-css/main/install.sh \
  | sh
```

Versão fixa: `... | VERSION=v0.5.1 sh`. Outro destino: `... | INSTALL_DIR=/usr/local/bin sh`.
Imagem Docker (`stallonels/sniffcss`, multi-arch, Chromium embutido):
[`docs/docker.md`](docs/docker.md).

> Requisito: Chrome/Chromium no sistema (ou `SNIFF_CHROME_PATH` / `--chrome`).

## Uso

```bash
# capture — computed styles de um elemento (default otimizado para IA)
sniff capture https://example.net -s ".btn-primary"
sniff capture https://example.net -s ".card" --depth 4 --categories box-model,typography
# snapshot completo (entrada de diff/check/jq): --no-summary

# capture — revelar elementos que só existem após uma ação (modal, dropdown, menu)
sniff capture https://example.net -s ".modal" --click "#open-modal"
sniff capture https://example.net -s ".result" \
  --action "click:#open-modal:5000" --action "type:#q:shoes"

# capture — área restrita e HTTPS self-signed (header em todo request, sem token na URL)
sniff capture https://example.net/cms -s "main" --header "X-CMS-AI-Token: <token>"
sniff capture https://app.local:3443/home -s "body" --ignore-certificate-errors

# capture — evidência visual para review humana (PNG ao lado do snapshot)
sniff capture https://example.net -s ".btn-primary" --screenshot snap.png

# flutter — árvore de widgets de um app Android (build debug), mesmo pipeline JSONL
sniff capture flutter://emulator-5554 --project ~/projetos/app --depth 10
sniff capture flutter://emulator-5554 --attach --project ~/projetos/app --depth 10
sniff capture flutter://pixel --avd pixel --project ~/projetos/app --depth 10
sniff capture flutter://emulator-5554 --project ~/projetos/app \
  --action "click:FilledButton-[<'counter'>][0]" --screenshot snap.png

# diff — só o que mudou entre duas capturas (CHANGED/ADDED/REMOVED + resumo)
sniff diff base.jsonl head.jsonl --tolerance 0.5

# check — regras offline: contraste AA/AAA, alvo ≥24px, foco, alt, uniformidade, oclusão
sniff check snap.jsonl --uniform --rules

# mcp — servidor stdio para agentes: captura → diff → checks → IA interpreta o delta
sniff-mcp

# prime — protocolo token-optimized para IA (byte-idêntico por versão)
sniff prime
# self — utilitários do binário (setup de cliente, completions, upgrade, version)
sniff self setup pi
sniff self completions bash

# init — grava/atualiza o bloco do sniff no AGENTS.md do projeto (--force = sobrescreve)
sniff init

# entrada estruturada — --params (uma operação) / --batch (N) / stdin, como o knudge
sniff capture --params '{"url":"https://example.net","selector":".btn","depth":2}'
sniff check --params '{"input":"snap.jsonl","rules":true}'
sniff capture --batch pages.jsonl        # uma página por linha (JSONL)
sniff capture https://example.net -s body --no-summary | sniff check -
```

Auth por header, login persistente (`--storage-state`), uploads reais e o mapa
de efeito `__actions`: [`docs/usage.md`](docs/usage.md). Backend Flutter:
[`docs/flutter.md`](docs/flutter.md).

## Documentação

| Tema | Referência |
|---|---|
| Referência completa da CLI (captura) | [`docs/usage.md`](docs/usage.md) |
| Superfície da CLI (verbos, `init`, `prime`, `self`, help) | [`docs/cli-surface.md`](docs/cli-surface.md) |
| Backend Flutter/Dart (instalação no Linux, modo `auto`, flags) | [`docs/flutter.md`](docs/flutter.md) |
| Guia para IA (pipeline captura→diff→checks) | [`docs/ai-usage.md`](docs/ai-usage.md) |
| `sniff diff` e `sniff check` | [`docs/diff-checks.md`](docs/diff-checks.md) |
| Auditoria de acessibilidade | [`docs/accessibility.md`](docs/accessibility.md) |
| Docker / container (incl. MCP) | [`docs/docker.md`](docs/docker.md) |
| Contrato de determinismo (padrão ouro) | [`docs/golden-run.md`](docs/golden-run.md) |
| Prompt de avaliação semântica por IA | [`docs/eval-prompt.md`](docs/eval-prompt.md) |
| Arquitetura interna | [`docs/architecture.md`](docs/architecture.md) |
| Política de qualidade (lints, toolchain, CI) | [`docs/quality.md`](docs/quality.md) |
| Revisão de dependências | [`docs/dependencies.md`](docs/dependencies.md) |
| Baseline de desempenho (`criterion`) | [`docs/benchmarks.md`](docs/benchmarks.md) |
| Guia ativo para agentes | [`SKILL.md`](SKILL.md) |
| Índice para LLM | [`llms.txt`](llms.txt) |
| Histórico de mudanças | [`CHANGELOG.md`](CHANGELOG.md) |
| Contribuição | [`AGENTS.md`](AGENTS.md) |

## Licença

[CC0 1.0 Universal](LICENSE) (domínio público) — uso livre, sem atribuição.
