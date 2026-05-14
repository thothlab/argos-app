//! Per-folder display order — `_order.argos.yaml`. Lists the basenames of
//! child entries (folders and request files) in the order the UI should
//! render them. Children not listed in `items` fall back to alphabetical
//! sort at the tail, so unknown / newly-added files appear naturally
//! without forcing an explicit reorder.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{read_yaml, write_yaml_atomic, FormatError, Kind};

pub const FILENAME: &str = "_order.argos.yaml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    #[serde(default = "default_kind")]
    pub kind: Kind,

    /// Basenames (file names for requests, directory names for folders) in
    /// the desired display order. Entries that no longer exist in the
    /// directory are silently ignored on read.
    #[serde(default)]
    pub items: Vec<String>,
}

impl Order {
    #[must_use]
    pub fn new(items: Vec<String>) -> Self {
        Self {
            kind: Kind::Order,
            items,
        }
    }

    /// Read `_order.argos.yaml` from the given folder directory. Returns
    /// `Ok(None)` if the file is absent — absence is the common case and
    /// not an error.
    ///
    /// # Errors
    ///
    /// I/O failures other than not-found, or YAML parse errors.
    pub fn load(dir: &Path) -> Result<Option<Self>, FormatError> {
        let path = dir.join(FILENAME);
        match read_yaml::<Self>(&path, "order") {
            Ok(o) => Ok(Some(o)),
            Err(FormatError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Write `_order.argos.yaml` into the given folder directory.
    ///
    /// # Errors
    ///
    /// I/O or YAML serialisation failures.
    pub fn save(&self, dir: &Path) -> Result<(), FormatError> {
        let path = dir.join(FILENAME);
        write_yaml_atomic(&path, self)
    }

    /// Remove `_order.argos.yaml` from the given folder if present.
    /// Idempotent: not-found is treated as success.
    ///
    /// # Errors
    ///
    /// I/O failures other than not-found.
    pub fn remove(dir: &Path) -> Result<(), FormatError> {
        let path = dir.join(FILENAME);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(FormatError::io(&path, e)),
        }
    }
}

fn default_kind() -> Kind {
    Kind::Order
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn round_trip() {
        let dir = tempdir().unwrap();
        let o = Order::new(vec!["b.argos.yaml".into(), "a.argos.yaml".into()]);
        o.save(dir.path()).unwrap();
        let loaded = Order::load(dir.path()).unwrap().unwrap();
        assert_eq!(loaded, o);
    }

    #[test]
    fn missing_returns_none() {
        let dir = tempdir().unwrap();
        assert!(Order::load(dir.path()).unwrap().is_none());
    }

    #[test]
    fn remove_is_idempotent() {
        let dir = tempdir().unwrap();
        Order::remove(dir.path()).unwrap();
        Order::new(vec!["x".into()]).save(dir.path()).unwrap();
        Order::remove(dir.path()).unwrap();
        assert!(Order::load(dir.path()).unwrap().is_none());
    }
}
