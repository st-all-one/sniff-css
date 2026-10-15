//! Argumentos de `sniff diff`.

use std::path::PathBuf;

use clap::Args;

/// Flags de `sniff diff`.
#[derive(Debug, Clone, Args, serde::Serialize, serde::Deserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "flags independentes da CLI (clap), não estados mutuamente exclusivos"
)]
pub(crate) struct DiffArgs {
    /// Operando — snapshot base (JSONL; `-` = stdin).
    #[arg(
        value_name = "BASE",
        required_unless_present_any = ["params", "batch"],
        conflicts_with_all = ["params", "batch"]
    )]
    pub base: Option<PathBuf>,

    /// Operando — snapshot head (JSONL; `-` = stdin).
    #[arg(
        value_name = "HEAD",
        required_unless_present_any = ["params", "batch"],
        conflicts_with_all = ["params", "batch"]
    )]
    pub head: Option<PathBuf>,

    /// Ignora mudanças menores que este valor na mesma unidade
    /// (ex.: 0.5 absorve 16px → 16.2px de jitter subpixel). 0 desliga.
    #[arg(long, default_value_t = 0.5)]
    pub tolerance: f64,

    /// Propriedades cuja mudança nunca marca um nó como alterado
    /// (voláteis/animadas), separadas por vírgula.
    #[arg(long, value_delimiter = ',')]
    pub ignore_props: Vec<String>,

    /// Suprime linhas ADDED/REMOVED (só CHANGED) — para listas cujo
    /// tamanho varia por design.
    #[arg(long, default_value_t = false)]
    pub no_structural: bool,

    /// Só imprime as estatísticas, sem as linhas de delta.
    #[arg(long, default_value_t = false)]
    pub stats_only: bool,

    /// Desliga a comparação de efeitos de UI (`__actions`) — ligada por padrão.
    #[arg(long = "no-actions", default_value_t = false)]
    pub no_actions: bool,

    /// Objeto JSON com todos os campos de UMA operação. `-` lê de stdin.
    #[arg(long, value_name = "JSON", conflicts_with = "batch")]
    pub params: Option<String>,

    /// Lote JSONL: uma operação por linha (`-` lê de stdin).
    #[arg(long, value_name = "FONTE")]
    pub batch: Option<String>,

    /// Valida os parâmetros sem executar (útil com `--batch`).
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}
