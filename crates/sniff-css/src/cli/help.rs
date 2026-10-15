//! Help embutido do `sniff`: resumo global e renderização por verbo.

use clap::CommandFactory;

use super::Cli;

/// Resumo global exibido no fim do help.
pub(crate) const AFTER_HELP: &str = "\
CICLO: sniff capture → sniff check → sniff diff
INIT: sniff init grava/atualiza o bloco do AGENTS.md do projeto (--force = sobrescreve)
ALVO: -s/--selector aceita CSS; `flutter://<device>` ativa o backend Flutter
SAÍDA: stdout = dados (JSONL), stderr = logs; --json = envelope de máquina
veja: sniff help <verbo>   # exemplo, escopo e quando NÃO usar";

/// Help completo do `sniff` (idêntico a `sniff --help`).
#[must_use]
pub(crate) fn render_help() -> String {
    Cli::command().render_help().to_string()
}
