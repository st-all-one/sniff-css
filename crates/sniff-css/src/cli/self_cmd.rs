//! Argumentos de `sniff self`.

use clap::{Args, Subcommand};

/// Subcomandos de `sniff self`.
#[derive(Debug, Subcommand)]
pub(crate) enum SelfCommand {
    /// Grava a recipe de um cliente (`claude`/`cursor`/`codex`/`pi`).
    Setup {
        /// Cliente alvo.
        #[arg(value_name = "CLIENTE")]
        client: String,
    },
    /// Gera shell completions.
    Completions {
        /// Shell alvo.
        #[arg(value_name = "SHELL")]
        shell: String,
    },
    /// Atualiza o binário pelo instalador oficial (release + checksum).
    Upgrade(UpgradeArgs),
    /// Mostra a versão.
    Version,
}

/// Argumentos de `sniff self upgrade`.
#[derive(Debug, Args)]
pub(crate) struct UpgradeArgs {
    /// Só mostra o plano (não baixa nem executa).
    #[arg(long)]
    pub dry_run: bool,
    /// Versão/tag alvo (default: `latest`).
    #[arg(long, value_name = "TAG")]
    pub version: Option<String>,
    /// Usa um script local em vez do instalador remoto (offline/testes).
    #[arg(long, value_name = "PATH")]
    pub script: Option<String>,
}
