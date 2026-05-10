// Field-level docs are intentionally omitted on data structs — see
// crates/core/src/format.rs for the rationale.
#![allow(missing_docs)]

//! Workspace operations: open / create / scan the on-disk tree.
//!
//! A `Workspace` is the in-memory representation of one of the directories
//! described in [`crate::format`]. Opening a workspace reads the root
//! manifest, walks the `collections/` and `environments/` subtrees, and
//! yields a typed tree the UI can render.
//!
//! Save operations live on the format types themselves (`RequestDraft::save`,
//! `Folder::save`, etc.) — this module only deals with discovery and
//! creation of the directory layout.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::format::{
    workspace::{default_gitignore, SubDir},
    Environment, Folder, FormatError, RequestDraft, WorkspaceManifest,
};

/// In-memory snapshot of an opened workspace.
///
/// Constructed via [`Workspace::open`]. Holds the root path, the parsed
/// manifest, the parsed environments and the lazily-walked collection
/// tree. The struct itself is cheap to clone — the heavy lifting is the
/// initial scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    /// Absolute path to the workspace root.
    pub root: PathBuf,
    pub manifest: WorkspaceManifest,
    pub tree: TreeNode,
    pub environments: Vec<Environment>,
}

/// A node in the collection tree. Folders contain other nodes; requests are
/// leaves.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TreeNode {
    /// Folder node. `meta` is `None` if the folder lacks a `_folder.argos.yaml`.
    Folder {
        path: PathBuf,
        name: String,
        meta: Option<Folder>,
        children: Vec<TreeNode>,
    },
    Request {
        path: PathBuf,
        draft: RequestDraft,
    },
}

impl Workspace {
    /// Open an existing workspace at `root`.
    ///
    /// # Errors
    ///
    /// [`FormatError::MissingWorkspaceMeta`] if `argos.yaml` is missing;
    /// [`FormatError::Io`] / [`FormatError::Yaml`] for read/parse failures
    /// of the manifest, environments, or any node in the tree.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, FormatError> {
        let root = root.as_ref().to_path_buf();
        let manifest = WorkspaceManifest::load(&root)?;

        let collections_root = manifest.config.resolve(&root, SubDir::Collections);
        let tree = if collections_root.is_dir() {
            scan_folder(&collections_root, "collections")?
        } else {
            TreeNode::Folder {
                path: collections_root,
                name: "collections".into(),
                meta: None,
                children: Vec::new(),
            }
        };

        let env_root = manifest.config.resolve(&root, SubDir::Environments);
        let environments = if env_root.is_dir() {
            load_environments(&env_root)?
        } else {
            Vec::new()
        };

        Ok(Self {
            root,
            manifest,
            tree,
            environments,
        })
    }

    /// Create a new workspace at `root`.
    ///
    /// Idempotent: if `argos.yaml` already exists at `root` we return an
    /// error rather than risk overwriting an existing workspace. The caller
    /// can decide whether to call [`Workspace::open`] instead.
    ///
    /// Side effects: creates `argos.yaml`, `collections/`, `environments/`,
    /// `runs/` and `.gitignore` under `root`.
    ///
    /// # Errors
    ///
    /// I/O failures during directory or file creation; an error if a
    /// workspace already exists at the path.
    pub fn create(root: impl AsRef<Path>, name: impl Into<String>) -> Result<Self, FormatError> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(&root).map_err(|e| FormatError::io(&root, e))?;

        let manifest_path = root.join("argos.yaml");
        if manifest_path.exists() {
            return Err(FormatError::io(
                &manifest_path,
                std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "workspace already initialised at this path",
                ),
            ));
        }

        let manifest = WorkspaceManifest::new(name);
        manifest.save(&root)?;

        for sub in [SubDir::Collections, SubDir::Environments, SubDir::Runs] {
            let path = manifest.config.resolve(&root, sub);
            std::fs::create_dir_all(&path).map_err(|e| FormatError::io(&path, e))?;
        }

        let gitignore_path = root.join(".gitignore");
        std::fs::write(&gitignore_path, default_gitignore())
            .map_err(|e| FormatError::io(&gitignore_path, e))?;

        Self::open(&root)
    }
}

fn scan_folder(dir: &Path, fallback_name: &str) -> Result<TreeNode, FormatError> {
    let meta = match Folder::load(dir) {
        Ok(m) => Some(m),
        Err(FormatError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            None
        }
        Err(e) => return Err(e),
    };

    let display_name = meta
        .as_ref()
        .map_or_else(|| fallback_name.to_string(), |m| m.name.clone());

    let mut children = Vec::new();

    let entries = std::fs::read_dir(dir).map_err(|e| FormatError::io(dir, e))?;
    let mut entries: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    // Stable order: folders first (alphabetical), then requests (alphabetical).
    entries.sort();

    for entry in entries {
        let file_name = entry.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if entry.is_dir() {
            let display = entry
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let sub = scan_folder(&entry, &display)?;
            children.push(sub);
        } else if file_name == crate::format::folder::FILENAME {
            // Already loaded as the folder meta — skip without continuing.
        } else if file_name.ends_with(".argos.yaml") {
            let draft = RequestDraft::load(&entry)?;
            children.push(TreeNode::Request { path: entry, draft });
        }
        // Anything else (READMEs, OpenAPI specs, etc.) is silently skipped.
    }

    Ok(TreeNode::Folder {
        path: dir.to_path_buf(),
        name: display_name,
        meta,
        children,
    })
}

fn load_environments(dir: &Path) -> Result<Vec<Environment>, FormatError> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| FormatError::io(dir, e))?;
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.to_string_lossy().ends_with(".env.argos.yaml"))
        .collect();
    paths.sort();
    for p in paths {
        out.push(Environment::load(&p)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::format::folder::HeaderEntry;
    use crate::http::HttpMethod;

    #[test]
    fn create_then_open_round_trips() {
        let dir = tempdir().unwrap();
        let ws = Workspace::create(dir.path(), "demo").unwrap();
        assert_eq!(ws.manifest.name, "demo");
        assert!(matches!(ws.tree, TreeNode::Folder { ref children, .. } if children.is_empty()));
        assert!(ws.environments.is_empty());
        assert!(dir.path().join(".gitignore").exists());
        assert!(dir.path().join("collections").is_dir());
        assert!(dir.path().join("environments").is_dir());
        assert!(dir.path().join("runs").is_dir());
    }

    #[test]
    fn create_refuses_to_overwrite() {
        let dir = tempdir().unwrap();
        Workspace::create(dir.path(), "first").unwrap();
        let err = Workspace::create(dir.path(), "second").unwrap_err();
        assert!(
            matches!(err, FormatError::Io { source, .. } if source.kind() == std::io::ErrorKind::AlreadyExists)
        );
    }

    #[test]
    fn scan_picks_up_folder_meta_and_requests() {
        let dir = tempdir().unwrap();
        let ws = Workspace::create(dir.path(), "demo").unwrap();

        let users_dir = dir.path().join("collections/users");
        std::fs::create_dir_all(&users_dir).unwrap();

        let folder = Folder {
            kind: crate::format::Kind::Folder,
            name: "Users".into(),
            description: None,
            headers: vec![HeaderEntry {
                name: "X-App".into(),
                value: "argos".into(),
                enabled: true,
            }],
            auth: None,
        };
        folder.save(&users_dir).unwrap();

        let req = RequestDraft::new_rest("List users", HttpMethod::Get, "{{baseUrl}}/users");
        req.save(&users_dir.join("list-users.argos.yaml")).unwrap();

        let ws = Workspace::open(&ws.root).unwrap();
        let children = match ws.tree {
            TreeNode::Folder { children, .. } => children,
            TreeNode::Request { .. } => panic!("root must be a folder"),
        };
        assert_eq!(children.len(), 1);
        match &children[0] {
            TreeNode::Folder {
                name,
                meta,
                children,
                ..
            } => {
                assert_eq!(name, "Users");
                assert_eq!(meta.as_ref().unwrap().headers.len(), 1);
                assert_eq!(children.len(), 1);
                assert!(matches!(
                    children[0],
                    TreeNode::Request { ref draft, .. } if draft.name == "List users"
                ));
            }
            TreeNode::Request { .. } => panic!("expected nested folder"),
        }
    }

    #[test]
    fn scan_loads_environments() {
        let dir = tempdir().unwrap();
        let ws = Workspace::create(dir.path(), "demo").unwrap();

        let env_dir = dir.path().join("environments");
        let mut env = Environment::new("local");
        env.variables.push(crate::format::environment::EnvVar {
            name: "baseUrl".into(),
            value: "http://localhost:3000".into(),
            enabled: true,
        });
        env.save(&env_dir.join("local.env.argos.yaml")).unwrap();

        let ws = Workspace::open(&ws.root).unwrap();
        assert_eq!(ws.environments.len(), 1);
        assert_eq!(ws.environments[0].name, "local");
    }

    #[test]
    fn missing_workspace_returns_typed_error() {
        let dir = tempdir().unwrap();
        let err = Workspace::open(dir.path()).unwrap_err();
        assert!(matches!(err, FormatError::MissingWorkspaceMeta(_)));
    }
}
