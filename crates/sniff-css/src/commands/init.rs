//! Handler de `sniff init`.

use std::path::PathBuf;

use crate::cli::InitArgs;
use crate::output::{AppError, Output};

/// Garante o bloco do sniff no `AGENTS.md` do projeto.
pub(crate) fn run(args: &InitArgs) -> Result<Output, AppError> {
    let root = if let Some(dir) = &args.dir {
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
