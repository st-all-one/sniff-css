//! Argumentos de `sniff check`.

use std::path::PathBuf;

use clap::Args;

/// Flags de `sniff check`.
#[derive(Debug, Clone, Args, serde::Serialize, serde::Deserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "flags independentes da CLI (clap), não estados mutuamente exclusivos"
)]
pub(crate) struct CheckArgs {
    /// Operando — snapshot JSONL (saída de `sniff capture`; `-` = stdin).
    #[arg(
        value_name = "SNAP",
        conflicts_with_all = ["input_legacy", "params", "batch"]
    )]
    pub input: Option<PathBuf>,

    /// [deprecado] Use o operando posicional: `sniff check <SNAP>`.
    #[arg(long = "input", short = 'i', hide = true, value_name = "SNAP")]
    pub input_legacy: Option<PathBuf>,

    /// Roda a checagem de uniformidade (o "card ímpar" entre irmãos).
    /// Ligada por padrão quando `--rules` não é dado.
    #[arg(long, default_value_t = false)]
    pub uniform: bool,

    /// Roda as regras derivadas (contraste, alvo, foco, alt).
    /// Ligada por padrão quando `--uniform` não é dado.
    #[arg(long, default_value_t = false)]
    pub rules: bool,

    /// Run only modern opportunity checks (`class: opportunity`)
    #[arg(long, default_value_t = false)]
    #[serde(default)]
    pub modern: bool,

    /// Keep the v0.5 default: uniformity + bug rules, no opportunities.
    #[arg(long = "no-modern", default_value_t = false, conflicts_with = "modern")]
    #[serde(default)]
    pub no_modern: bool,

    /// Only emit opportunities whose check belongs to this guide id.
    #[arg(long, value_name = "ID")]
    #[serde(default)]
    pub guide: Option<String>,

    /// Suppress a check id (repeatable or comma-separated).
    #[arg(long, value_name = "CHECK", value_delimiter = ',', action = clap::ArgAction::Append)]
    #[serde(default)]
    pub ignore: Vec<String>,

    /// Modern-check profile: `default` or `strict`.
    #[arg(long, value_name = "PROFILE")]
    #[serde(default)]
    pub profile: Option<String>,

    /// Cross-state checks over `__meta.matrix`. ON by default.
    #[arg(long, default_value_t = true)]
    #[serde(default = "default_true")]
    pub responsive: bool,

    /// Disable cross-state checks.
    #[arg(long = "no-responsive", default_value_t = false)]
    #[serde(default)]
    pub no_responsive: bool,

    /// Tolerância para desvios numéricos (na mesma unidade).
    #[arg(long, default_value_t = 0.5)]
    pub tolerance: f64,

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

fn default_true() -> bool {
    true
}
