//! Environment file — `<slug>.env.argos.yaml`. Maps variable names to
//! values that requests reference via `{{var}}` substitution.
//!
//! Encrypted values use the `!secret` YAML tag and base64-encoded
//! ciphertext; decryption happens lazily in E12. For now we store them as
//! plain strings on a separate `secrets:` block so the structure is clear
//! even before the crypto layer lands.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{read_yaml, write_yaml_atomic, FormatError, Kind};

/// `kind: environment`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    #[serde(default = "default_kind")]
    pub kind: Kind,

    /// Display name (e.g. `local`, `staging`, `production`).
    pub name: String,

    /// Plain-text variables. Keys reference these as `{{key}}` from request
    /// URLs / headers / bodies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<EnvVar>,

    /// Secret variables. Stored as ciphertext (base64) in v1.0+; for v0.1
    /// the values are still plaintext but live on a separate block to make
    /// the eventual transition trivial.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<EnvVar>,
}

impl Environment {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            kind: Kind::Environment,
            name: name.into(),
            variables: Vec::new(),
            secrets: Vec::new(),
        }
    }

    /// Read an environment file.
    ///
    /// # Errors
    ///
    /// I/O or YAML parse errors.
    pub fn load(path: &Path) -> Result<Self, FormatError> {
        read_yaml(path, "environment")
    }

    /// Write an environment file.
    ///
    /// # Errors
    ///
    /// I/O or YAML serialisation failures.
    pub fn save(&self, path: &Path) -> Result<(), FormatError> {
        write_yaml_atomic(path, self)
    }

    /// Look up a variable by name. Plain `variables` take precedence over
    /// `secrets` if a key happens to exist in both — we surface that case
    /// as a lint warning in E3.
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<&str> {
        self.variables
            .iter()
            .chain(self.secrets.iter())
            .find(|v| v.enabled && v.name == name)
            .map(|v| v.value.as_str())
    }
}

/// One environment variable entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_kind() -> Kind {
    Kind::Environment
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
        let path = dir.path().join("local.env.argos.yaml");
        let env = Environment::new("local");
        env.save(&path).unwrap();
        assert_eq!(Environment::load(&path).unwrap(), env);
    }

    #[test]
    fn round_trip_with_vars_and_secrets() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("prod.env.argos.yaml");
        let env = Environment {
            kind: Kind::Environment,
            name: "production".into(),
            variables: vec![
                EnvVar {
                    name: "baseUrl".into(),
                    value: "https://api.example.com".into(),
                    enabled: true,
                },
                EnvVar {
                    name: "trace".into(),
                    value: "off".into(),
                    enabled: false,
                },
            ],
            secrets: vec![EnvVar {
                name: "token".into(),
                value: "shh".into(),
                enabled: true,
            }],
        };
        env.save(&path).unwrap();
        assert_eq!(Environment::load(&path).unwrap(), env);
    }

    #[test]
    fn lookup_skips_disabled_vars() {
        let env = Environment {
            kind: Kind::Environment,
            name: "x".into(),
            variables: vec![
                EnvVar {
                    name: "a".into(),
                    value: "1".into(),
                    enabled: true,
                },
                EnvVar {
                    name: "b".into(),
                    value: "2".into(),
                    enabled: false,
                },
            ],
            secrets: vec![EnvVar {
                name: "c".into(),
                value: "3".into(),
                enabled: true,
            }],
        };
        assert_eq!(env.lookup("a"), Some("1"));
        assert_eq!(env.lookup("b"), None);
        assert_eq!(env.lookup("c"), Some("3"));
        assert_eq!(env.lookup("missing"), None);
    }
}
