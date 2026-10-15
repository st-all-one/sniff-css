//! Handler de `sniff snapshots`.

use std::path::{Path, PathBuf};

use crate::cli::SnapshotsArgs;
use crate::output::{AppError, Output};

/// Lista os snapshots persistidos sob o diretório raiz.
pub(crate) fn run(args: &SnapshotsArgs) -> Result<Output, AppError> {
    let root = args
        .root
        .as_deref()
        .map_or_else(sniff_core::snapshot::snapshot_root, PathBuf::from);

    let mut entries: Vec<String> = Vec::new();
    if root.is_dir() {
        collect(&root, &root, &mut entries)?;
    }
    entries.sort();

    let text = if entries.is_empty() {
        format!("nenhum snapshot em {}\n", root.display())
    } else {
        let mut text = String::new();
        for entry in &entries {
            text.push_str(entry);
            text.push('\n');
        }
        text
    };

    let data = serde_json::json!({
        "root": root.display().to_string(),
        "count": entries.len(),
        "snapshots": entries,
    });
    Ok(Output::new(text, data))
}

/// Coleta recursivamente os arquivos (relativos a `root`), ignorando `.gitignore`.
fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), AppError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, out)?;
            continue;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        if rel.file_name().and_then(|name| name.to_str()) == Some(".gitignore") {
            continue;
        }
        out.push(rel.to_string_lossy().replace('\\', "/"));
    }
    Ok(())
}
