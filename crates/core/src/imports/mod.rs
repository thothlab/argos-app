//! Import adapters that turn third-party collection formats into the
//! Argos in-memory representation. Materialising the import (writing
//! YAML files into a workspace) happens in the host shell — see
//! [`ImportItem`] for the IR the host walks.

#![allow(missing_docs)]

use serde::{Deserialize, Serialize};

use crate::format::request::RequestDraft;

pub mod bruno;
pub mod insomnia;
pub mod openapi;
pub mod postman;

/// Result of importing a single source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedCollection {
    /// Display name from the source ("info.name" in Postman).
    pub name: String,
    /// Optional description from the source.
    pub description: Option<String>,
    /// Top-level items — folders and requests, in source order.
    pub items: Vec<ImportItem>,
    /// Collection-level variables `(name, value)`. The host can fold
    /// these into a fresh environment file or merge them into the
    /// active env. Empty for sources without a variables section.
    pub variables: Vec<(String, String)>,
}

/// Tree node produced by an importer. Matches Argos's two on-disk
/// kinds: folders (with nested items) and requests.
//
// `RequestDraft` is large; we accept the size variance — folders have
// a vec of children and requests have the full draft, so wrapping
// either in a `Box` saves nothing in practice.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportItem {
    Folder {
        name: String,
        description: Option<String>,
        items: Vec<ImportItem>,
    },
    Request {
        draft: RequestDraft,
    },
}
