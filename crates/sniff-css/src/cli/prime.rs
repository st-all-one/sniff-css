//! Argumentos de `sniff prime`.

use clap::Args;

/// Flags de `sniff prime`.
#[derive(Debug, Args)]
pub(crate) struct PrimeArgs {
    /// Inclui o detalhamento completo (todas as flags e o contrato JSON).
    #[arg(long)]
    pub long: bool,
}
