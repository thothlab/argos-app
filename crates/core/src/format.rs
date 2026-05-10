// Field-level docs are intentionally omitted on data structs in this module —
// the field names + struct-level docs cover semantics. Will tighten after E5
// when the on-disk format stabilises across all protocols.
#![allow(missing_docs)]

//! On-disk file format for an Argos workspace.
//!
//! A workspace is a directory containing one root manifest plus a tree of
//! YAML files describing folders, requests, and environments. The format is
//! deliberately human-readable and git-friendly — the goal is that
//! `git diff` of an Argos workspace tells you exactly what changed in
//! the API surface.
//!
//! ## Layout
//!
//! ```text
//! my-workspace/
//! ├── argos.yaml                   # workspace meta (root)
//! ├── collections/
//! │   └── <folder>/
//! │       ├── _folder.argos.yaml   # folder meta (auth / headers inherited by children)
//! │       └── <name>.argos.yaml    # individual request
//! ├── environments/
//! │   └── <name>.env.argos.yaml
//! └── runs/                         # request history (gitignored by default)
//! ```
//!
//! ## File kinds
//!
//! Every YAML file (except `argos.yaml`) starts with a `kind:` discriminator
//! so the parser can dispatch correctly without relying on filename alone:
//! `kind: folder`, `kind: request`, `kind: environment`.
//!
//! ## Naming
//!
//! - Workspace meta: literal `argos.yaml`.
//! - Folder meta: literal `_folder.argos.yaml` (the leading `_` keeps it at
//!   the top of an alphabetical listing alongside its sibling requests).
//! - Request: `<slug>.argos.yaml` — the slug is derived from the human name
//!   on save (lowercase, kebab-case).
//! - Environment: `<slug>.env.argos.yaml` — the `.env.` middle segment makes
//!   it grep-able and visually distinct from request files.

pub mod environment;
pub mod folder;
pub mod request;
pub mod workspace;

pub use environment::Environment;
pub use folder::Folder;
pub use request::{AuthConfig, BodyDraft, FormField, RequestDraft, RestRequest, ScriptHooks};
pub use workspace::{WorkspaceConfig, WorkspaceManifest};

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors raised by the workspace format layer.
#[derive(Debug, Error)]
pub enum FormatError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("YAML parse error at {path}: {source}")]
    Yaml {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("YAML serialise error: {0}")]
    YamlSer(#[from] serde_yaml::Error),
    #[error("workspace meta `argos.yaml` not found at {0}")]
    MissingWorkspaceMeta(PathBuf),
    #[error("wrong file kind: expected {expected}, got {actual:?} (path: {path})")]
    WrongKind {
        expected: &'static str,
        actual: Option<String>,
        path: PathBuf,
    },
}

impl FormatError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub(crate) fn yaml(path: impl Into<PathBuf>, source: serde_yaml::Error) -> Self {
        Self::Yaml {
            path: path.into(),
            source,
        }
    }
}

/// File-kind discriminator used in YAML headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Workspace,
    Folder,
    Request,
    Environment,
}

/// Parse a YAML file into the requested type, asserting that its `kind:`
/// discriminator matches `expected`.
pub(crate) fn read_yaml<T: serde::de::DeserializeOwned>(
    path: &Path,
    expected: &'static str,
) -> Result<T, FormatError> {
    let bytes = std::fs::read(path).map_err(|e| FormatError::io(path, e))?;
    // First parse as a generic mapping so we can validate the `kind` field
    // before attempting the typed deserialise (clearer error messages).
    let value: serde_yaml::Value =
        serde_yaml::from_slice(&bytes).map_err(|e| FormatError::yaml(path, e))?;
    if let Some(kind) = value.get("kind").and_then(|v| v.as_str()) {
        if kind != expected {
            return Err(FormatError::WrongKind {
                expected,
                actual: Some(kind.to_string()),
                path: path.to_path_buf(),
            });
        }
    } else if expected != "workspace" {
        // Workspace manifest historically allowed an implicit kind. Other
        // files require it explicitly.
        return Err(FormatError::WrongKind {
            expected,
            actual: None,
            path: path.to_path_buf(),
        });
    }
    serde_yaml::from_value(value).map_err(|e| FormatError::yaml(path, e))
}

/// Serialise a typed value to YAML and write it atomically (temp file +
/// rename) so external watchers never see a half-written file.
pub(crate) fn write_yaml_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), FormatError> {
    let body = serde_yaml::to_string(value)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| FormatError::io(parent, e))?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|s| s.to_str()).unwrap_or("yaml")
    ));
    std::fs::write(&tmp, body.as_bytes()).map_err(|e| FormatError::io(&tmp, e))?;
    std::fs::rename(&tmp, path).map_err(|e| FormatError::io(path, e))?;
    Ok(())
}

/// Convert a human request name (e.g. "List users") to a filesystem-safe
/// slug ("list-users").
#[must_use]
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "untitled".into()
    } else {
        out
    }
}

#[cfg(test)]
mod slug_tests {
    use super::*;

    #[test]
    fn ascii_kebab_basics() {
        assert_eq!(slugify("List users"), "list-users");
        assert_eq!(slugify("Create User"), "create-user");
    }

    #[test]
    fn collapses_multiple_separators() {
        assert_eq!(slugify("Hello   World!!"), "hello-world");
        assert_eq!(slugify("a / b / c"), "a-b-c");
    }

    #[test]
    fn unicode_falls_through_to_separators() {
        // Non-ASCII chars are treated as separators in v0.1 — punycode
        // handling lands when we add file-name conflict resolution.
        assert_eq!(slugify("Заявка №42"), "42");
    }

    #[test]
    fn empty_yields_untitled() {
        assert_eq!(slugify("   "), "untitled");
        assert_eq!(slugify(""), "untitled");
    }
}
