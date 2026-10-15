//! Argumentos de `sniff check`.

use std::path::PathBuf;

use clap::Args;

/// Flags de `sniff check`.
#[derive(Debug, Args)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "flags independentes da CLI (clap), não estados mutuamente exclusivos"
)]
pub(crate) struct CheckArgs {
    /// Snapshot JSONL (saída de `sniff capture`).
    #[arg(long, short = 'i')]
    pub input: PathBuf,

    /// Roda a checagem de uniformidade (o "card ímpar" entre irmãos).
    /// Ligada por padrão quando `--rules` não é dado.
    #[arg(long, default_value_t = false)]
    pub uniform: bool,

    /// Roda as regras derivadas (contraste, alvo, foco, alt).
    /// Ligada por padrão quando `--uniform` não é dado.
    #[arg(long, default_value_t = false)]
    pub rules: bool,

    /// Tolerância para desvios numéricos (na mesma unidade).
    #[arg(long, default_value_t = 0.5)]
    pub tolerance: f64,
}
