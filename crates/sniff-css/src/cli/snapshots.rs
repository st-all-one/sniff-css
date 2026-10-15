//! Argumentos de `sniff snapshots`.

use clap::Args;

/// Flags de `sniff snapshots`.
#[derive(Debug, Args)]
pub(crate) struct SnapshotsArgs {
    /// Diretório raiz dos snapshots
    /// (default: `SNIFF_SNAPSHOT_DIR` ou `sniff` no CWD).
    #[arg(long)]
    pub root: Option<String>,
}
