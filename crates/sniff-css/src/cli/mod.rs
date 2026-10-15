//! Argumentos e subcomandos do `sniff` (superfície v4, ver `docs/cli-surface.md`).

pub(crate) mod capture;
pub(crate) mod check;
pub(crate) mod diff;
pub(crate) mod help;
pub(crate) mod init;
pub(crate) mod prime;
pub(crate) mod self_cmd;
pub(crate) mod snapshots;

pub(crate) use capture::CaptureArgs;
pub(crate) use check::CheckArgs;
pub(crate) use diff::DiffArgs;
pub(crate) use help::{AFTER_HELP, render_help};
pub(crate) use init::InitArgs;
pub(crate) use prime::PrimeArgs;
pub(crate) use self_cmd::{SelfCommand, UpgradeArgs};
pub(crate) use snapshots::SnapshotsArgs;

use clap::{Parser, Subcommand};

/// Interface de linha de comando do `sniff`.
#[derive(Debug, Parser)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "flags globais independentes da CLI (clap), não estados mutuamente exclusivos"
)]
#[command(
    name = "sniff",
    version,
    about = "sniff — estilos computados para LLMs",
    after_help = AFTER_HELP
)]
pub(crate) struct Cli {
    /// Emite o resultado no envelope JSON (contrato de máquina).
    #[arg(long, global = true)]
    pub(crate) json: bool,

    /// Nível de log em stderr (`error|warn|info|debug|trace|off`).
    #[arg(
        long,
        global = true,
        value_name = "NÍVEL",
        default_value = "warn",
        value_parser = parse_log_level
    )]
    pub(crate) log_level: String,

    /// Silencia o stderr.
    #[arg(long, global = true)]
    pub(crate) quiet: bool,

    /// Subcomando; sem verbo o `sniff` mostra o help.
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

/// Níveis válidos de log (conjunto fechado).
const LOG_LEVELS: [&str; 6] = ["error", "warn", "info", "debug", "trace", "off"];

/// Valida `--log-level` contra o conjunto fechado.
fn parse_log_level(raw: &str) -> Result<String, String> {
    if LOG_LEVELS.contains(&raw.to_ascii_lowercase().as_str()) {
        Ok(raw.to_string())
    } else {
        Err(format!("nível de log inválido: {raw:?} ({LOG_LEVELS:?})"))
    }
}

/// Verbos de topo.
#[derive(Debug, Subcommand)]
#[expect(
    clippy::large_enum_variant,
    reason = "`Capture` carrega as ~55 flags da captura; boxar quebraria o derive do clap"
)]
pub(crate) enum Command {
    /// Captura estilos computados de uma página (web via CDP ou Flutter).
    Capture(CaptureArgs),

    /// Grava/atualiza o bloco do sniff no `AGENTS.md` do projeto.
    Init(InitArgs),

    /// Diff determinístico entre dois snapshots JSONL.
    Diff(DiffArgs),

    /// Checks determinísticos (contraste, alvo, foco, alt, uniformidade).
    Check(CheckArgs),

    /// Lista os snapshots persistidos.
    Snapshots(SnapshotsArgs),

    /// Protocolo de uso estático (byte-idêntico por versão).
    Prime(PrimeArgs),

    /// Instalação e utilitários do binário.
    #[command(name = "self")]
    SelfCmd {
        /// Subcomando de instalação.
        #[command(subcommand)]
        command: SelfCommand,
    },
}

impl Command {
    /// Nome canônico usado no envelope.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Capture(_) => "capture",
            Self::Init(_) => "init",
            Self::Diff(_) => "diff",
            Self::Check(_) => "check",
            Self::Snapshots(_) => "snapshots",
            Self::Prime(_) => "prime",
            Self::SelfCmd { .. } => "self",
        }
    }
}
