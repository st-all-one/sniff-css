//! Argumentos de `sniff diff`.

use std::path::PathBuf;

use clap::Args;

/// Flags de `sniff diff`.
#[derive(Debug, Args)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "flags independentes da CLI (clap), não estados mutuamente exclusivos"
)]
pub(crate) struct DiffArgs {
    /// Snapshot base (JSONL gerado por `sniff capture`).
    pub base: PathBuf,

    /// Snapshot head (JSONL gerado por `sniff capture`).
    pub head: PathBuf,

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
}
