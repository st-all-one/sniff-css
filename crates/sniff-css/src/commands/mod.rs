//! Despacho dos verbos do `sniff`.

pub(crate) mod capture;
pub(crate) mod check;
pub(crate) mod diff;
pub(crate) mod init;
pub(crate) mod prime;
pub(crate) mod self_cmd;
pub(crate) mod snapshots;

use std::process::ExitCode;

use clap::Parser;
use clap::error::ErrorKind as ClapErrorKind;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Command};
use crate::output::{self, AppError, ErrorCode, Output};

/// Ponto de entrada: parseia argv e roda o verbo pedido.
pub(crate) async fn run() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => return handle_parse_error(&err),
    };
    init_tracing(&cli);
    match &cli.command {
        None => no_command(&cli),
        Some(command) => dispatch(&cli, command).await,
    }
}

/// Despacha o verbo (assíncrono por causa do `capture`).
async fn dispatch(cli: &Cli, command: &Command) -> ExitCode {
    let name = command.name();
    match command {
        Command::Capture(args) => match capture::run(cli, args).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => output::emit_error(cli.json, name, &error),
        },
        Command::Init(args) => finish(cli, name, init::run(args)),
        Command::Diff(args) => finish(cli, name, diff::run(args)),
        Command::Check(args) => finish(cli, name, check::run(args)),
        Command::Snapshots(args) => finish(cli, name, snapshots::run(args)),
        Command::Prime(args) => finish(cli, name, Ok(prime::run(args))),
        Command::SelfCmd { command } => self_cmd::run(cli, command),
    }
}

/// Emite o resultado de um verbo que devolve [`Output`].
fn finish(cli: &Cli, name: &str, result: Result<Output, AppError>) -> ExitCode {
    match result {
        Ok(output) => output::emit_success(cli.json, name, &output),
        Err(error) => output::emit_error(cli.json, name, &error),
    }
}

/// `sniff` sozinho = `sniff --help` (exit 0); com `--json` não há envelope sem verbo.
fn no_command(cli: &Cli) -> ExitCode {
    if cli.json {
        let error = AppError::invalid("informe um verbo (veja `sniff help`)");
        return output::emit_error(true, "help", &error);
    }
    let mut help = crate::cli::render_help();
    if !help.ends_with('\n') {
        help.push('\n');
    }
    if output::write_stdout(help.as_bytes()).is_err() {
        return ExitCode::from(ErrorCode::Io.exit_code());
    }
    ExitCode::SUCCESS
}

/// Configura o tracing a partir de `--log-level`/`--quiet`.
fn init_tracing(cli: &Cli) {
    let level = if cli.quiet {
        "off"
    } else {
        cli.log_level.as_str()
    };
    if let Err(error) = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(level))
        .with_writer(std::io::stderr)
        .try_init()
    {
        output::warn(&format!("tracing subscriber not installed: {error}"));
    }
}

/// Trata erros de parsing do `clap` (help/version em stdout; erros em stderr).
fn handle_parse_error(err: &clap::Error) -> ExitCode {
    if err.kind() == ClapErrorKind::DisplayHelpOnMissingArgumentOrSubcommand {
        let rendered = err.render().to_string();
        return match output::write_stdout(rendered.as_bytes()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::from(ErrorCode::Io.exit_code()),
        };
    }
    let to_stderr = err.use_stderr();
    let rendered = err.render().to_string();
    if to_stderr {
        output::warn(&rendered);
        ExitCode::from(ErrorCode::Invalid.exit_code())
    } else {
        if output::write_stdout(rendered.as_bytes()).is_err() {
            return ExitCode::from(ErrorCode::Io.exit_code());
        }
        ExitCode::SUCCESS
    }
}
