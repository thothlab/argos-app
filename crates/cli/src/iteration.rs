//! Iteration-data loader for `argos run --iteration-data`.
//!
//! Supports two layouts (Postman / Newman parity):
//!   - CSV: first row is the header, each subsequent row is one
//!     iteration. Values bind as env overrides under their column
//!     name.
//!   - JSON: an array of objects, each object one iteration. Keys bind
//!     as env overrides; non-string values are stringified through
//!     [`Value::to_string`] (matching what env templating expects).
//!
//! Format is detected from the file extension; on `.csv` we parse as
//! CSV, otherwise as JSON. We don't sniff the contents — explicit is
//! cheaper than clever for CI.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use serde_json::Value;

/// Errors loading an iteration-data file.
#[derive(Debug)]
pub enum IterationDataError {
    /// `std::fs::read_to_string` failed.
    Read { path: String, error: String },
    /// The file extension isn't `.csv` or `.json` (or `.jsonc`).
    UnsupportedFormat { path: String },
    /// JSON parsed but isn't an array of objects.
    BadJsonShape,
    /// Underlying parser error with the line number where it bit.
    Parse(String),
}

impl fmt::Display for IterationDataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, error } => write!(f, "read {path}: {error}"),
            Self::UnsupportedFormat { path } => write!(
                f,
                "unsupported iteration-data format (expected .csv or .json): {path}"
            ),
            Self::BadJsonShape => write!(f, "iteration-data JSON must be an array of objects"),
            Self::Parse(msg) => write!(f, "parse error: {msg}"),
        }
    }
}

impl std::error::Error for IterationDataError {}

/// Load one iteration-data file into a list of rows. An empty `Vec`
/// means the file has a header but no data rows.
///
/// # Errors
///
/// Returns a variant of [`IterationDataError`] when the file is
/// missing, the extension is unrecognised, or the contents don't match
/// the declared format.
pub fn load(path: &Path) -> Result<Vec<HashMap<String, String>>, IterationDataError> {
    let text = std::fs::read_to_string(path).map_err(|e| IterationDataError::Read {
        path: path.display().to_string(),
        error: e.to_string(),
    })?;
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("csv") => parse_csv(&text),
        Some("json" | "jsonc") => parse_json(&text),
        _ => Err(IterationDataError::UnsupportedFormat {
            path: path.display().to_string(),
        }),
    }
}

fn parse_json(text: &str) -> Result<Vec<HashMap<String, String>>, IterationDataError> {
    let v: Value = serde_json::from_str(text).map_err(|e| IterationDataError::Parse(e.to_string()))?;
    let Value::Array(arr) = v else {
        return Err(IterationDataError::BadJsonShape);
    };
    let mut rows = Vec::with_capacity(arr.len());
    for item in arr {
        let Value::Object(map) = item else {
            return Err(IterationDataError::BadJsonShape);
        };
        let mut row = HashMap::with_capacity(map.len());
        for (k, v) in map {
            row.insert(k, value_to_string(&v));
        }
        rows.push(row);
    }
    Ok(rows)
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        // Objects / arrays serialise to compact JSON — round-trips back
        // for tests that need structured fixtures.
        _ => v.to_string(),
    }
}

/// Minimal CSV reader: comma separator, `"`-quoted fields with `""`
/// to escape a literal quote inside a quoted field. Embedded newlines
/// inside quoted fields *are* supported. Trims a single trailing `\r`
/// per record so CRLF files behave.
fn parse_csv(text: &str) -> Result<Vec<HashMap<String, String>>, IterationDataError> {
    let records = read_records(text)?;
    let mut iter = records.into_iter();
    let Some(header) = iter.next() else {
        return Ok(Vec::new());
    };
    if header.is_empty() || header.iter().all(String::is_empty) {
        return Ok(Vec::new());
    }
    let mut rows = Vec::new();
    for rec in iter {
        // Skip empty trailing line.
        if rec.len() == 1 && rec[0].is_empty() {
            continue;
        }
        let mut row = HashMap::with_capacity(header.len());
        for (i, name) in header.iter().enumerate() {
            if name.is_empty() {
                continue;
            }
            row.insert(name.clone(), rec.get(i).cloned().unwrap_or_default());
        }
        rows.push(row);
    }
    Ok(rows)
}

fn read_records(text: &str) -> Result<Vec<Vec<String>>, IterationDataError> {
    let mut records = Vec::new();
    let mut field = String::new();
    let mut record: Vec<String> = Vec::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    let mut line = 1_usize;

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                if c == '\n' {
                    line += 1;
                }
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => in_quotes = true,
            ',' => {
                record.push(std::mem::take(&mut field));
            }
            '\n' => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                line += 1;
            }
            '\r' => {
                // Eat CR; next iteration handles the LF.
            }
            _ => field.push(c),
        }
    }
    if in_quotes {
        return Err(IterationDataError::Parse(format!(
            "unterminated quote starting before line {line}",
        )));
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_array_of_objects() {
        let rows = parse_json(r#"[{"a":"1","b":2},{"a":"x"}]"#).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get("a").unwrap(), "1");
        assert_eq!(rows[0].get("b").unwrap(), "2");
        assert_eq!(rows[1].get("a").unwrap(), "x");
    }

    #[test]
    fn rejects_non_array_json() {
        assert!(matches!(
            parse_json(r#"{"a": 1}"#).unwrap_err(),
            IterationDataError::BadJsonShape
        ));
    }

    #[test]
    fn parses_basic_csv() {
        let csv = "user,id\nalice,1\nbob,2\n";
        let rows = parse_csv(csv).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get("user").unwrap(), "alice");
        assert_eq!(rows[1].get("id").unwrap(), "2");
    }

    #[test]
    fn parses_csv_with_quoted_commas_and_escapes() {
        let csv = "name,note\n\"Alice, Bob\",\"says \"\"hi\"\"\"\n";
        let rows = parse_csv(csv).unwrap();
        assert_eq!(rows[0].get("name").unwrap(), "Alice, Bob");
        assert_eq!(rows[0].get("note").unwrap(), "says \"hi\"");
    }

    #[test]
    fn parses_csv_crlf() {
        let csv = "a,b\r\n1,2\r\n3,4\r\n";
        let rows = parse_csv(csv).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].get("a").unwrap(), "3");
    }

    #[test]
    fn missing_trailing_columns_default_to_empty() {
        let csv = "a,b,c\n1,2\n";
        let rows = parse_csv(csv).unwrap();
        assert_eq!(rows[0].get("c").unwrap(), "");
    }

    #[test]
    fn unterminated_quote_errors() {
        let csv = "a\n\"oops";
        assert!(matches!(
            parse_csv(csv).unwrap_err(),
            IterationDataError::Parse(_)
        ));
    }

    #[test]
    fn load_uses_extension_to_pick_parser() {
        let tmp = tempfile::tempdir().unwrap();
        let csv_path = tmp.path().join("data.csv");
        std::fs::write(&csv_path, "k\nv\n").unwrap();
        assert_eq!(load(&csv_path).unwrap()[0].get("k").unwrap(), "v");
        let json_path = tmp.path().join("data.json");
        std::fs::write(&json_path, r#"[{"k":"v"}]"#).unwrap();
        assert_eq!(load(&json_path).unwrap()[0].get("k").unwrap(), "v");
        let other = tmp.path().join("data.txt");
        std::fs::write(&other, "k\nv\n").unwrap();
        assert!(matches!(
            load(&other).unwrap_err(),
            IterationDataError::UnsupportedFormat { .. }
        ));
    }
}
