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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::DeltaLine;

    fn sample() -> DeltaLine {
        DeltaLine {
            status: "CHANGED",
            selector: "div.card".into(),
            tag: Some("DIV".into()),
            path: None,
            depth: None,
            changes: Some(serde_json::json!({"styles": {"box_model": {"width": {
                "before": "44px", "after": "40px"
            }}}})),
            snapshot: None,
        }
    }

    #[test]
    fn delta_to_string_serializes_one_json_line_per_delta() {
        let text = delta_to_string(&[sample(), sample()]).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["status"], "CHANGED");
        assert_eq!(first["selector"], "div.card");
        assert_eq!(
            first["changes"]["styles"]["box_model"]["width"]["after"],
            "40px"
        );
    }

    #[test]
    fn delta_to_string_empty_yields_empty_string() {
        assert_eq!(delta_to_string(&[]).unwrap(), "");
    }
}
