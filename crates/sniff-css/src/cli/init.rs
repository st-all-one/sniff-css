//! Argumentos de `sniff init`.

use clap::Args;

/// Flags de `sniff init`.
#[derive(Debug, Args)]
pub(crate) struct InitArgs {
    /// Sobrescreve o `AGENTS.md` inteiro com o bloco do sniff (destrutivo).
    /// Sem esta flag, apenas a região entre `<!-- sniff:start/end -->` é tocada.
    #[arg(long)]
    pub force: bool,

    /// Diretório do projeto (default: raiz do git a partir do CWD, ou o CWD).
    #[arg(long)]
    pub dir: Option<String>,
}
