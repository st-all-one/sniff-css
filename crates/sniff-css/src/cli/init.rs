//! Argumentos de `sniff init`.

use clap::Args;

/// Flags de `sniff init`.
#[derive(Debug, Clone, Args, serde::Serialize, serde::Deserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "flags independentes da CLI (clap), não estados mutuamente exclusivos"
)]
pub(crate) struct InitArgs {
    /// Operando — diretório do projeto (default: raiz do git a partir do CWD).
    #[arg(value_name = "DIR", conflicts_with_all = ["dir_legacy", "params", "batch"])]
    pub dir: Option<String>,

    /// [deprecado] Use o operando posicional: `sniff init [DIR]`.
    #[arg(long = "dir", hide = true, value_name = "DIR")]
    pub dir_legacy: Option<String>,

    /// Sobrescreve o `AGENTS.md` inteiro com o bloco do sniff (destrutivo).
    /// Sem esta flag, apenas a região entre `<!-- sniff:start/end -->` é tocada.
    #[arg(long)]
    pub force: bool,

    /// Objeto JSON com todos os campos de UMA operação. `-` lê de stdin.
    #[arg(long, value_name = "JSON", conflicts_with = "batch")]
    pub params: Option<String>,

    /// Lote JSONL: uma operação por linha (`-` lê de stdin).
    #[arg(long, value_name = "FONTE")]
    pub batch: Option<String>,

    /// Valida os parâmetros sem aplicar (útil com `--batch`).
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}
