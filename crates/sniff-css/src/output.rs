//! Output/error plumbing shared by the `sniff` verbs.

use std::io::{self, Write as _};
use std::process::ExitCode;

use serde_json::Value;

/// Result of a verb: human text, machine data and warnings.
#[derive(Debug, Clone)]
pub(crate) struct Output {
    /// Text emitted on stdout in human mode.
    pub text: String,
    /// Structured data emitted inside the `--json` envelope.
    pub data: Value,
    /// Warnings (stderr in human mode; `warnings` in JSON).
    pub warnings: Vec<String>,
}

impl Output {
    /// Builds an output with the given human text and machine data.
    #[must_use]
    pub(crate) fn new(text: impl Into<String>, data: Value) -> Self {
        Self {
            text: text.into(),
            data,
            warnings: Vec::new(),
        }
    }
}

/// Exit code contract (knudge-like).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorCode {
    /// Invalid input / bad arguments (2).
    Invalid,
    /// Requested resource not found (3).
    NotFound,
    /// I/O error (5).
    Io,
    /// Internal/unexpected error (70).
    Internal,
}

impl ErrorCode {
    /// Numeric exit code.
    #[must_use]
    pub(crate) fn exit_code(self) -> u8 {
        match self {
            Self::Invalid => 2,
            Self::NotFound => 3,
            Self::Io => 5,
            Self::Internal => 70,
        }
    }

    /// Stable machine token used in the JSON envelope.
    #[must_use]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Invalid => "invalid",
            Self::NotFound => "not_found",
            Self::Io => "io",
            Self::Internal => "internal",
        }
    }
}

/// A verb failure with an exit-code classification.
#[derive(Debug)]
pub(crate) struct AppError {
    /// Machine/human message.
    pub message: String,
    /// Exit-code class.
    pub code: ErrorCode,
}

impl AppError {
    /// Invalid input (`2`).
    #[must_use]
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: ErrorCode::Invalid,
        }
    }

    /// Not found (`3`).
    #[must_use]
    pub(crate) fn not_found(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: ErrorCode::NotFound,
        }
    }

    /// I/O error (`5`).
    #[must_use]
    pub(crate) fn io(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: ErrorCode::Io,
        }
    }

    /// Internal error (`70`).
    #[must_use]
    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: ErrorCode::Internal,
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

impl From<anyhow::Error> for AppError {
    fn from(error: anyhow::Error) -> Self {
        Self::internal(format!("{error:#}"))
    }
}

impl From<sniff_core::SniffError> for AppError {
    fn from(error: sniff_core::SniffError) -> Self {
        match error {
            sniff_core::SniffError::Io(message) => Self::io(message),
            sniff_core::SniffError::NoMatch { .. } => Self::not_found(error.to_string()),
            other => Self::invalid(other.to_string()),
        }
    }
}

impl From<io::Error> for AppError {
    fn from(error: io::Error) -> Self {
        Self::io(error.to_string())
    }
}

impl From<sniff_css_diff::DiffError> for AppError {
    fn from(error: sniff_css_diff::DiffError) -> Self {
        match error {
            sniff_css_diff::DiffError::Io(e) if e.kind() == io::ErrorKind::NotFound => {
                Self::not_found(e.to_string())
            }
            other => Self::io(other.to_string()),
        }
    }
}

/// Writes raw bytes to stdout (data channel).
pub(crate) fn write_stdout(bytes: &[u8]) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    out.write_all(bytes)?;
    out.flush()
}

/// Writes to stderr through the single audit point in `sniff-core`.
pub(crate) fn warn(message: &str) {
    sniff_core::diagnostic::write(format_args!("{message}"));
}

/// Emits a successful result (human text or JSON envelope).
pub(crate) fn emit_success(json: bool, command: &str, output: &Output) -> ExitCode {
    if json {
        let envelope = envelope(command, &output.data, &output.warnings);
        return write_json(&envelope);
    }
    for warning in &output.warnings {
        warn(warning);
    }
    let mut text = output.text.clone();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    if write_stdout(text.as_bytes()).is_err() {
        return ExitCode::from(ErrorCode::Io.exit_code());
    }
    ExitCode::SUCCESS
}

/// Emits an error (human stderr or JSON envelope) and returns its exit code.
pub(crate) fn emit_error(json: bool, command: &str, error: &AppError) -> ExitCode {
    if json {
        let value = serde_json::json!({
            "success": false,
            "command": command,
            "error": {
                "code": error.code.as_str(),
                "message": error.message,
                "retryable": false,
            }
        });
        let _code = write_json(&value);
    } else {
        warn(&format!("erro: {}", error.message));
    }
    ExitCode::from(error.code.exit_code())
}

/// Builds a success envelope.
#[must_use]
pub(crate) fn envelope(command: &str, data: &Value, warnings: &[String]) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("success".into(), Value::Bool(true));
    object.insert("command".into(), Value::String(command.into()));
    object.insert("data".into(), data.clone());
    if !warnings.is_empty() {
        object.insert(
            "warnings".into(),
            Value::Array(warnings.iter().cloned().map(Value::String).collect()),
        );
    }
    Value::Object(object)
}

/// Writes a JSON value as a single newline-terminated line to stdout.
fn write_json(value: &Value) -> ExitCode {
    let mut line = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    line.push('\n');
    match write_stdout(line.as_bytes()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(ErrorCode::Io.exit_code()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_match_contract() {
        assert_eq!(ErrorCode::Invalid.exit_code(), 2);
        assert_eq!(ErrorCode::NotFound.exit_code(), 3);
        assert_eq!(ErrorCode::Io.exit_code(), 5);
        assert_eq!(ErrorCode::Internal.exit_code(), 70);
    }

    #[test]
    fn envelope_contains_success_data_and_warnings() {
        let value = envelope("diff", &serde_json::json!({"changed": 1}), &["x".into()]);
        assert_eq!(value["success"], true);
        assert_eq!(value["command"], "diff");
        assert_eq!(value["data"]["changed"], 1);
        assert_eq!(value["warnings"][0], "x");
    }

    #[test]
    fn envelope_omits_empty_warnings() {
        let value = envelope("check", &serde_json::json!({}), &[]);
        assert!(value.get("warnings").is_none());
    }
}
