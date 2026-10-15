//! Leitura de entrada por operando/stdin (convenção universal do posicional).
//!
//! Regra (ver `docs/cli-surface.md`):
//! - `-` é sempre **stdin** (pipe ou heredoc);
//! - um operando obrigatório ausente, com stdin **não-TTY**, é preenchido pelo stdin;
//! - ausente e com stdin **TTY** é erro (`2`), sem bloquear.

use std::io::{IsTerminal, Read};
use std::path::Path;

use crate::{SniffError, SniffResult};

/// `true` quando o operando é exatamente `-` (stdin explícito).
#[must_use]
pub fn is_stdin(path: &Path) -> bool {
    path.as_os_str() == "-"
}

/// `true` quando o stdin não é um terminal (pipe/heredoc).
#[must_use]
pub fn stdin_is_piped() -> bool {
    !std::io::stdin().is_terminal()
}

/// Lê todo o stdin como UTF-8.
///
/// # Errors
/// Propaga erros de I/O do stdin.
pub fn read_stdin() -> SniffResult<String> {
    let mut buffer = String::new();
    std::io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|error| SniffError::Io(format!("stdin: {error}")))?;
    Ok(buffer)
}

/// Lê o conteúdo de um operando tipo arquivo (`-` = stdin).
///
/// # Errors
/// Propaga I/O do stdin ou do arquivo.
pub fn read_source(path: &Path) -> SniffResult<String> {
    if is_stdin(path) {
        return read_stdin();
    }
    std::fs::read_to_string(path)
        .map_err(|error| SniffError::Io(format!("{}: {error}", path.display())))
}
