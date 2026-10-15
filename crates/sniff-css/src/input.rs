//! Entrada estruturada: `--params`, `--batch` e stdin (D147 estendido).
//!
//! - `--params <JSON|->` é **um** item (objeto com os campos do verbo).
//! - `--batch <FONTE|->` é **N** itens (JSONL: uma operação por linha).
//! - `--params` ≡ lote de 1; ambos usam o **mesmo parser** (`merge`).
//! - `-` lê de stdin em qualquer canal.

use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::output::{AppError, Output};

/// Teto de itens por lote (`sniff.batch_max`).
pub(crate) const BATCH_MAX: usize = 100;

/// Relatório padronizado de `--dry-run`: uma linha JSONL de status por item,
/// sem executar nada. Falhas de parse ficam em `warnings`/`data.failed`.
pub(crate) fn dry_run_report<T>(items: &[Result<T, AppError>]) -> Output {
    let mut lines = String::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut failed = 0_usize;
    for (index, item) in items.iter().enumerate() {
        match item {
            Ok(_) => lines.push_str(
                &serde_json::json!({"index": index, "ok": true, "dry_run": true}).to_string(),
            ),
            Err(error) => {
                failed = failed.saturating_add(1);
                warnings.push(format!(
                    "item {}: {}",
                    index.saturating_add(1),
                    error.message
                ));
                lines.push_str(
                    &serde_json::json!({"index": index, "ok": false, "error": error.message})
                        .to_string(),
                );
            }
        }
        lines.push('\n');
    }
    let data = serde_json::json!({
        "dry_run": true,
        "count": items.len(),
        "failed": failed,
    });
    let mut output = Output::new(lines, data);
    output.warnings = warnings;
    output
}

/// Lê `--params` (JSON inline; `-` = stdin).
fn read_params(raw: &str) -> Result<String, AppError> {
    if raw == "-" {
        return sniff_core::input::read_stdin().map_err(AppError::from);
    }
    Ok(raw.to_string())
}

/// Lê `--batch` (arquivo JSONL; `-` = stdin).
fn read_batch(raw: &str) -> Result<String, AppError> {
    if raw == "-" {
        return sniff_core::input::read_stdin().map_err(AppError::from);
    }
    sniff_core::input::read_source(Path::new(raw)).map_err(AppError::from)
}

/// Faz o merge de um objeto JSON sobre a base (flags/posicionais já parseados)
/// e desserializa o resultado — a **fonte única** de `--params`, cada linha de
/// `--batch`, e (por construção) do próprio CLI.
///
/// # Errors
/// JSON inválido, valor que não é objeto, ou campo com tipo errado (exit 2).
pub(crate) fn merge<T>(base: &T, raw: &str) -> Result<T, AppError>
where
    T: Serialize + DeserializeOwned,
{
    let mut value =
        serde_json::to_value(base).map_err(|error| AppError::internal(error.to_string()))?;
    let overrides: serde_json::Value = serde_json::from_str(raw)
        .map_err(|error| AppError::invalid(format!("JSON inválido: {error}")))?;
    let patch = overrides
        .as_object()
        .ok_or_else(|| AppError::invalid("--params/--batch espera um objeto JSON por item"))?;
    let base_object = value
        .as_object_mut()
        .ok_or_else(|| AppError::internal("base de parâmetros não é um objeto"))?;
    for (key, item) in patch {
        base_object.insert(key.clone(), item.clone());
    }
    serde_json::from_value(value)
        .map_err(|error| AppError::invalid(format!("campo inválido nos parâmetros: {error}")))
}

/// Resultado de [`requests`]: a leitura do lote é fatal, mas cada item pode ter
/// falhado no parse (best-effort).
pub(crate) type Requested<T> = Result<Vec<Result<T, AppError>>, AppError>;

/// Resolve a lista de requisições de um verbo a partir de `--params`/`--batch`
/// (mutuamente exclusivos, garantido pelo `clap`) e do próprio operando/flag.
///
/// # Errors
/// Fonte ilegível, lote vazio ou acima de [`BATCH_MAX`]. Erros de parse de uma
/// linha viram `Err` por item (best-effort), não abortam o lote.
pub(crate) fn requests<T>(base: &T, params: Option<&str>, batch: Option<&str>) -> Requested<T>
where
    T: Serialize + DeserializeOwned,
{
    if let Some(source) = batch {
        let text = read_batch(source)?;
        let lines: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        if lines.is_empty() {
            return Err(AppError::invalid("lote vazio (nenhuma operação JSONL)"));
        }
        if lines.len() > BATCH_MAX {
            return Err(AppError::invalid(format!(
                "lote com {} itens excede sniff.batch_max={BATCH_MAX}",
                lines.len()
            )));
        }
        return Ok(lines.into_iter().map(|line| merge(base, line)).collect());
    }
    if let Some(source) = params {
        let text = read_params(source)?;
        return Ok(vec![merge(base, &text)]);
    }
    Ok(vec![merge(base, "{}")])
}
