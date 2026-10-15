//! Handler de `sniff init`.

use std::path::PathBuf;

use serde_json::json;

use crate::cli::InitArgs;
use crate::output::{AppError, Output};

/// Garante o bloco do sniff no `AGENTS.md` do projeto.
pub(crate) fn run(args: &InitArgs) -> Result<Output, AppError> {
    let base = args.clone();
    let items = crate::input::requests(&base, args.params.as_deref(), args.batch.as_deref())?;

    if args.dry_run {
        return Ok(crate::input::dry_run_report(&items));
    }

    if args.batch.is_some() {
        let mut lines = String::new();
        let mut warnings: Vec<String> = Vec::new();
        let mut failures = 0_usize;
        for (index, item) in items.iter().enumerate() {
            let result = match item {
                Ok(item) => apply(item),
                Err(error) => Err(AppError::invalid(error.message.clone())),
            };
            match result {
                Ok(output) => {
                    lines.push_str(
                        &json!({"index": index, "ok": true, "data": output.data}).to_string(),
                    );
                    lines.push('\n');
                    warnings.extend(output.warnings);
                }
                Err(error) => {
                    failures = failures.saturating_add(1);
                    warnings.push(format!(
                        "item {}: {}",
                        index.saturating_add(1),
                        error.message
                    ));
                    lines.push_str(
                        &json!({"index": index, "ok": false, "error": error.message}).to_string(),
                    );
                    lines.push('\n');
                }
            }
        }
        let data = json!({"count": items.len(), "failed": failures});
        let mut output = Output::new(lines, data);
        output.warnings = warnings;
        return Ok(output);
    }

    let item = items
        .into_iter()
        .next()
        .ok_or_else(|| AppError::invalid("sem operação"))??;
    apply(&item)
}

/// Aplica o bloco de um item.
fn apply(args: &InitArgs) -> Result<Output, AppError> {
    let dir = args.dir.as_deref().or(args.dir_legacy.as_deref());
    let root = if let Some(dir) = dir {
        PathBuf::from(dir)
    } else {
        let cwd = std::env::current_dir().map_err(|error| {
            AppError::io(format!("não foi possível obter o diretório atual: {error}"))
        })?;
        sniff_core::agents::find_project_root(&cwd)
    };

    let applied = sniff_core::agents::apply(&root, args.force).map_err(AppError::from)?;
    let (state_pt, state_json) = match applied.state {
        sniff_core::agents::State::Created => ("criado", "created"),
        sniff_core::agents::State::Updated => ("atualizado", "updated"),
        sniff_core::agents::State::Unchanged => ("inalterado", "unchanged"),
    };
    let text = format!(
        "AGENTS.md {state_pt} em {}\nbloco gerenciado: {} .. {}\npróximos: sniff prime   # protocolo de uso\n",
        applied.path.display(),
        sniff_core::agents::MARKER_BEGIN,
        sniff_core::agents::MARKER_END,
    );
    let data = serde_json::json!({
        "path": applied.path,
        "state": state_json,
        "force": args.force,
        "version": sniff_core::agents::VERSION,
    });
    Ok(Output::new(text, data))
}
