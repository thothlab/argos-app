//! Workspace manifest — the root `argos.yaml` file at the top of every
//! workspace.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{read_yaml, write_yaml_atomic, FormatError, Kind};

/// Defaults baked into the [`WorkspaceManifest`]'s schema. Bumped when we
/// make breaking changes to the on-disk layout.
pub const SCHEMA_VERSION: u32 = 1;

/// `argos.yaml` — workspace meta. Tells the engine *where* to look for the
/// other parts of the workspace (collections, environments) and carries the
/// human-facing name + workspace-scoped settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    /// `kind: workspace` discriminator. Always set to `Kind::Workspace`.
    #[serde(default = "default_kind")]
    pub kind: Kind,

    /// Schema version of the on-disk format. Used for forward-compat checks.
    #[serde(default = "default_schema_version")]
    pub version: u32,

    /// Human-readable workspace name. Defaults to the directory name on
    /// `create_workspace` if not specified.
    pub name: String,

    /// Optional long-form description shown on the welcome screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Workspace-scoped behaviour knobs. Optional — defaults applied if
    /// missing.
    #[serde(default)]
    pub config: WorkspaceConfig,
}

impl WorkspaceManifest {
    /// Convenience constructor used by `create_workspace`.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            kind: Kind::Workspace,
            version: SCHEMA_VERSION,
            name: name.into(),
            description: None,
            config: WorkspaceConfig::default(),
        }
    }

    /// Read `argos.yaml` from the given workspace root.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError::MissingWorkspaceMeta`] if the file does not
    /// exist; [`FormatError::Io`] / [`FormatError::Yaml`] for read or parse
    /// failures.
    pub fn load(workspace_root: &Path) -> Result<Self, FormatError> {
        let path = workspace_root.join("argos.yaml");
        if !path.exists() {
            return Err(FormatError::MissingWorkspaceMeta(
                workspace_root.to_path_buf(),
            ));
        }
        read_yaml(&path, "workspace")
    }

    /// Write `argos.yaml` to the given workspace root.
    ///
    /// # Errors
    ///
    /// I/O or YAML serialisation failures.
    pub fn save(&self, workspace_root: &Path) -> Result<(), FormatError> {
        let path = workspace_root.join("argos.yaml");
        write_yaml_atomic(&path, self)
    }
}

/// Workspace-scoped configuration. Everything has a sensible default so the
/// section can be omitted in the YAML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// Subdirectory (relative to the workspace root) where collections live.
    /// Defaults to `collections`.
    #[serde(default = "default_collections_dir")]
    pub collections_dir: String,

    /// Subdirectory for environments. Defaults to `environments`.
    #[serde(default = "default_environments_dir")]
    pub environments_dir: String,

    /// Subdirectory for run history. Defaults to `runs`. Gitignored by
    /// default — see [`crate::format::workspace::default_gitignore`].
    #[serde(default = "default_runs_dir")]
    pub runs_dir: String,

    /// Default environment name (e.g. `local`, `production`). Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_environment: Option<String>,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            collections_dir: default_collections_dir(),
            environments_dir: default_environments_dir(),
            runs_dir: default_runs_dir(),
            default_environment: None,
        }
    }
}

impl WorkspaceConfig {
    /// Resolve a [`WorkspaceConfig`] subdir into an absolute path, given
    /// the workspace root.
    #[must_use]
    pub fn resolve(&self, root: &Path, kind: SubDir) -> PathBuf {
        let segment = match kind {
            SubDir::Collections => &self.collections_dir,
            SubDir::Environments => &self.environments_dir,
            SubDir::Runs => &self.runs_dir,
        };
        root.join(segment)
    }
}

/// Names of the conventional subdirectories of a workspace.
#[derive(Debug, Clone, Copy)]
pub enum SubDir {
    Collections,
    Environments,
    Runs,
}

/// The `.gitignore` template seeded into new workspaces. Keeps run history
/// + transient caches out of git while still tracking everything else.
#[must_use]
pub fn default_gitignore() -> &'static str {
    "# Argos workspace gitignore\n\
     runs/\n\
     .argos/\n\
     *.argos.tmp\n"
}

fn default_kind() -> Kind {
    Kind::Workspace
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

fn default_collections_dir() -> String {
    "collections".into()
}

fn default_environments_dir() -> String {
    "environments".into()
}

fn default_runs_dir() -> String {
    "runs".into()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn manifest_round_trip() {
        let dir = tempdir().unwrap();
        let m = WorkspaceManifest::new("my-project");
        m.save(dir.path()).unwrap();

        let loaded = WorkspaceManifest::load(dir.path()).unwrap();
        assert_eq!(loaded, m);
    }

    #[test]
    fn missing_manifest_is_a_typed_error() {
        let dir = tempdir().unwrap();
        let err = WorkspaceManifest::load(dir.path()).unwrap_err();
        assert!(matches!(err, FormatError::MissingWorkspaceMeta(_)));
    }

    #[test]
    fn defaults_round_trip_via_yaml_string() {
        let m = WorkspaceManifest::new("demo");
        let s = serde_yaml::to_string(&m).unwrap();
        // Make sure we don't ship default-y noise (description: null etc).
        assert!(!s.contains("description"));
        let parsed: WorkspaceManifest = serde_yaml::from_str(&s).unwrap();
        assert_eq!(parsed, m);
    }

    #[test]
    fn config_paths_resolve_relative_to_root() {
        let cfg = WorkspaceConfig::default();
        let root = Path::new("/tmp/ws");
        assert_eq!(
            cfg.resolve(root, SubDir::Collections),
            root.join("collections")
        );
        assert_eq!(
            cfg.resolve(root, SubDir::Environments),
            root.join("environments")
        );
        assert_eq!(cfg.resolve(root, SubDir::Runs), root.join("runs"));
    }
}
