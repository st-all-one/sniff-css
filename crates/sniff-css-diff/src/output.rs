//! Serialization of delta output (JSONL) and stats.

use std::io::Write;

use crate::diff::DeltaLine;
use crate::error::{DiffError, DiffResult};

/// Write one JSON line per delta.
pub fn write_delta<W: Write>(writer: &mut W, deltas: &[DeltaLine]) -> DiffResult<()> {
    for d in deltas {
        serde_json::to_writer(&mut *writer, d)?;
        writeln!(writer)?;
    }
    writer.flush()?;
    Ok(())
}

/// Serialize the delta to a string (one JSON line per delta).
pub fn delta_to_string(deltas: &[DeltaLine]) -> DiffResult<String> {
    let mut buf = Vec::new();
    write_delta(&mut buf, deltas)?;
    String::from_utf8(buf).map_err(|e| DiffError::Io(std::io::Error::other(e)))
}
