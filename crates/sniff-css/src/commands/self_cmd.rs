//! Handler de `sniff self` — instalação e utilitários do binário.

use std::process::ExitCode;

use serde_json::json;

use crate::cli::{Cli, SelfCommand, UpgradeArgs};
use crate::output::{self, AppError, Output};

/// Versão do binário.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Clientes suportados por `sniff self setup`.
const CLIENTS: [&str; 4] = ["claude", "cursor", "codex", "pi"];

/// Shells suportadas por `sniff self completions`.
const SHELLS: [&str; 3] = ["bash", "zsh", "fish"];

/// Verbos de topo (para completions).
const VERBS: [&str; 8] = [
    "capture",
    "init",
    "diff",
    "check",
    "snapshots",
    "prime",
    "self",
    "help",
];

/// Instalador oficial (mesmo do `curl | sh` do README).
const INSTALL_URL: &str = "https://raw.githubusercontent.com/st-all-one/sniff-css/main/install.sh";

/// `sniff self <sub>`.
pub(crate) fn run(cli: &Cli, command: &SelfCommand) -> ExitCode {
    let result = match command {
        SelfCommand::Version => Ok(version()),
        SelfCommand::Completions { shell } => completions(shell),
        SelfCommand::Setup { client } => setup(client),
        SelfCommand::Upgrade(args) => upgrade(args),
    };
    match result {
        Ok(output) => output::emit_success(cli.json, "self", &output),
        Err(error) => output::emit_error(cli.json, "self", &error),
    }
}

/// `sniff self version`.
fn version() -> Output {
    Output::new(
        format!(
            "sniff {VERSION}\najuda: sniff help (ou `sniff help <verbo>`); protocolo: sniff prime"
        ),
        json!({ "version": VERSION, "name": "sniff" }),
    )
}

/// `sniff self completions <shell>`.
fn completions(shell: &str) -> Result<Output, AppError> {
    let script = match shell {
        "bash" => bash(),
        "zsh" => zsh(),
        "fish" => fish(),
        other => {
            return Err(AppError::invalid(format!(
                "shell desconhecido: {other:?} (esperado: {})",
                SHELLS.join(", ")
            )));
        }
    };
    let data = json!({ "shell": shell, "script": &script });
    Ok(Output::new(script, data))
}

/// `sniff self setup <client>`: grava a recipe do cliente e a imprime.
fn setup(client: &str) -> Result<Output, AppError> {
    if !CLIENTS.contains(&client) {
        return Err(AppError::invalid(format!(
            "cliente desconhecido: {client:?} (esperado: {})",
            CLIENTS.join(", ")
        )));
    }
    let recipe = json!({
        "sniff": {
            "version": VERSION,
            "client": client,
            "binary": "sniff",
            "verbs": VERBS,
            "docs": "SKILL.md",
            "mcp": { "command": "sniff-mcp", "transport": "stdio" },
        }
    });
    let pretty = serde_json::to_string_pretty(&recipe)
        .map_err(|error| AppError::internal(format!("recipe inválida: {error}")))?;

    let path = std::path::PathBuf::from(".sniff")
        .join("setup")
        .join(format!("{client}.json"));
    let note = match write_recipe(&path, &pretty) {
        Ok(()) => format!("recipe {client} gravada em {}\n", path.display()),
        Err(error) => format!(
            "não foi possível gravar {}: {error}\n(copie a recipe abaixo)\n",
            path.display()
        ),
    };

    let text = format!("{note}\n{pretty}\n");
    let data = json!({ "client": client, "path": path, "recipe": recipe });
    Ok(Output::new(text, data))
}

/// Cria o diretório e grava a recipe de forma atômica (tmp + rename).
fn write_recipe(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let staging = path.with_extension("json.tmp");
    std::fs::write(&staging, contents)?;
    std::fs::rename(&staging, path)?;
    Ok(())
}

/// `sniff self upgrade`: roda o instalador oficial (ou um script local).
fn upgrade(args: &UpgradeArgs) -> Result<Output, AppError> {
    let version = args.version.as_deref().unwrap_or("latest");
    let command = match &args.script {
        Some(script) => format!("sh {script}"),
        None => format!("curl --proto '=https' --tlsv1.2 -sSf {INSTALL_URL} | sh"),
    };
    let plan = format!("VERSION={version} {command}");

    if args.dry_run {
        return Ok(Output::new(
            format!("plano (não executado):\n  {plan}\n"),
            json!({ "dry_run": true, "version": version, "script": args.script }),
        ));
    }

    let status = match &args.script {
        Some(script) => std::process::Command::new("sh")
            .arg(script)
            .env("VERSION", version)
            .status(),
        None => std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                "curl --proto '=https' --tlsv1.2 -sSf {INSTALL_URL} | sh"
            ))
            .env("VERSION", version)
            .status(),
    }
    .map_err(|error| AppError::io(format!("não foi possível rodar o instalador: {error}")))?;

    if !status.success() {
        return Err(AppError::io(format!(
            "instalador saiu com código {}",
            status.code().unwrap_or(70)
        )));
    }
    let text = format!("sniff atualizado ({version})\n");
    Ok(Output::new(
        text,
        json!({ "dry_run": false, "version": version }),
    ))
}

/// Script de completion do bash.
fn bash() -> String {
    format!(
        "# bash completion for sniff\n_sniff() {{\n    local cur=\"${{COMP_WORDS[COMP_CWORD]}}\"\n    COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") )\n}}\ncomplete -F _sniff sniff\n",
        VERBS.join(" ")
    )
}

/// Script de completion do zsh.
fn zsh() -> String {
    format!(
        "#compdef sniff\n_sniff() {{\n    compadd -- {}\n}}\ncompdef _sniff sniff\n",
        VERBS.join(" ")
    )
}

/// Script de completion do fish.
fn fish() -> String {
    use std::fmt::Write as _;
    let mut script = String::from("# fish completion for sniff\n");
    for verb in VERBS {
        let _write = writeln!(
            script,
            "complete -c sniff -n '__fish_use_subcommand' -a {verb}"
        );
    }
    script
}
