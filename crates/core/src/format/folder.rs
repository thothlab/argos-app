//! Folder meta — `_folder.argos.yaml` placed inside each collection
//! directory. Carries the human display name + folder-level auth / headers
//! that get inherited by descendant requests.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{read_yaml, write_yaml_atomic, FormatError, Kind};

/// File name we always look for at the top of a folder.
pub const FILENAME: &str = "_folder.argos.yaml";

/// `_folder.argos.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    /// `kind: folder`.
    #[serde(default = "default_kind")]
    pub kind: Kind,

    /// Display name for the folder.
    pub name: String,

    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Headers inherited by every request inside this folder. Children
    /// can override individual values by re-declaring the same header.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<HeaderEntry>,

    /// Folder-level auth, applied to every child unless they override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<crate::format::request::AuthConfig>,
}

impl Folder {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            kind: Kind::Folder,
            name: name.into(),
            description: None,
            headers: Vec::new(),
            auth: None,
        }
    }

    /// Read `_folder.argos.yaml` from the given folder directory.
    ///
    /// # Errors
    ///
    /// I/O or YAML parse errors; missing-file maps to [`FormatError::Io`]
    /// with `NotFound` source.
    pub fn load(dir: &Path) -> Result<Self, FormatError> {
        let path = dir.join(FILENAME);
        read_yaml(&path, "folder")
    }

    /// Write `_folder.argos.yaml` into the given folder directory.
    ///
    /// # Errors
    ///
    /// I/O or YAML serialisation failures.
    pub fn save(&self, dir: &Path) -> Result<(), FormatError> {
        let path = dir.join(FILENAME);
        write_yaml_atomic(&path, self)
    }
}

/// Inheritable folder-level header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderEntry {
    pub name: String,
    pub value: String,
    /// `false` lets users keep the header around for future use without
    /// applying it. Defaults to `true`.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_kind() -> Kind {
    Kind::Folder
}

fn default_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn round_trip_minimal() {
        let dir = tempdir().unwrap();
        let f = Folder::new("Users");
        f.save(dir.path()).unwrap();
        let loaded = Folder::load(dir.path()).unwrap();
        assert_eq!(loaded, f);
    }

    #[test]
    fn round_trip_with_headers() {
        let dir = tempdir().unwrap();
        let f = Folder {
            kind: Kind::Folder,
            name: "Users".into(),
            description: Some("CRUD over /users".into()),
            headers: vec![
                HeaderEntry {
                    name: "X-Trace-Id".into(),
                    value: "{{trace_id}}".into(),
                    enabled: true,
                },
                HeaderEntry {
                    name: "X-Disabled".into(),
                    value: "off".into(),
                    enabled: false,
                },
            ],
            auth: None,
        };
        f.save(dir.path()).unwrap();
        let loaded = Folder::load(dir.path()).unwrap();
        assert_eq!(loaded, f);
    }
}
