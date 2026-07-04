use std::collections::HashSet;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

const BUILT_IN: &str = include_str!("../catalog/trusted-v1.json");

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Catalog {
    pub schema_version: u32,
    pub generated_at: String,
    pub entries: Vec<CatalogEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CatalogEntry {
    pub id: String,
    pub title: String,
    pub developer: String,
    pub system: String,
    pub description: String,
    pub license: License,
    pub source: Source,
    pub artifact: Artifact,
    #[serde(default)]
    pub artwork: Vec<Artwork>,
    pub trust: Trust,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct License {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Source {
    pub name: String,
    pub homepage: String,
    pub repository: String,
    pub audit_note: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Artifact {
    pub url: String,
    pub filename: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Artwork {
    pub kind: ArtworkKind,
    pub url: String,
    pub filename: String,
    pub size: u64,
    pub sha256: String,
    pub source_provided: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ArtworkKind {
    BoxFront,
    Screenshot,
    TitleScreen,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Trust {
    Verified,
    SourceAuthorized,
    Unverified,
}

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("catalog JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported catalog schema version {0}")]
    Schema(u32),
    #[error("duplicate or empty entry id: {0}")]
    EntryId(String),
    #[error("entry {id} has an unsafe {field}: {value}")]
    UnsafeField {
        id: String,
        field: &'static str,
        value: String,
    },
    #[error("entry {id} has an invalid URL in {field}: {value}")]
    Url {
        id: String,
        field: &'static str,
        value: String,
    },
    #[error("entry {id} has an invalid SHA-256: {value}")]
    Hash { id: String, value: String },
    #[error("entry {0} has no explicit license")]
    License(String),
    #[error("entry {0} is not approved for automatic installation")]
    Trust(String),
}

impl Catalog {
    pub fn built_in() -> Result<Self, CatalogError> {
        Self::parse(BUILT_IN)
    }

    pub fn parse(json: &str) -> Result<Self, CatalogError> {
        let catalog: Self = serde_json::from_str(json)?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn validate(&self) -> Result<(), CatalogError> {
        if self.schema_version != 1 {
            return Err(CatalogError::Schema(self.schema_version));
        }

        let mut ids = HashSet::new();
        for entry in &self.entries {
            entry.validate()?;
            if entry.id.is_empty() || !ids.insert(entry.id.as_str()) {
                return Err(CatalogError::EntryId(entry.id.clone()));
            }
        }
        Ok(())
    }
}

impl CatalogEntry {
    pub fn validate(&self) -> Result<(), CatalogError> {
        validate_slug(&self.id, "id", &self.id, true)?;
        validate_slug(&self.id, "system", &self.system, false)?;
        validate_filename(&self.id, &self.artifact.filename)?;

        if self.license.name.trim().is_empty() || self.license.name.eq_ignore_ascii_case("unknown")
        {
            return Err(CatalogError::License(self.id.clone()));
        }
        validate_url(&self.id, "license.url", &self.license.url)?;
        validate_url(&self.id, "source.homepage", &self.source.homepage)?;
        validate_url(&self.id, "source.repository", &self.source.repository)?;
        validate_url(&self.id, "artifact.url", &self.artifact.url)?;

        validate_integrity(&self.id, &self.artifact.sha256, self.artifact.size)?;
        for artwork in &self.artwork {
            validate_url(&self.id, "artwork.url", &artwork.url)?;
            validate_filename(&self.id, &artwork.filename)?;
            validate_integrity(&self.id, &artwork.sha256, artwork.size)?;
        }
        if self.trust == Trust::Unverified {
            return Err(CatalogError::Trust(self.id.clone()));
        }
        Ok(())
    }

    pub fn install_relative_path(&self) -> std::path::PathBuf {
        Path::new("RetroBat")
            .join("roms")
            .join(&self.system)
            .join(&self.artifact.filename)
    }
}

fn validate_integrity(id: &str, sha256: &str, size: u64) -> Result<(), CatalogError> {
    let hash = hex::decode(sha256).map_err(|_| CatalogError::Hash {
        id: id.to_owned(),
        value: sha256.to_owned(),
    })?;
    if hash.len() != 32 || size == 0 {
        return Err(CatalogError::Hash {
            id: id.to_owned(),
            value: sha256.to_owned(),
        });
    }
    Ok(())
}

fn validate_url(id: &str, field: &'static str, value: &str) -> Result<(), CatalogError> {
    let parsed = Url::parse(value).map_err(|_| CatalogError::Url {
        id: id.to_owned(),
        field,
        value: value.to_owned(),
    })?;
    if !matches!(parsed.scheme(), "https" | "http") || parsed.host_str().is_none() {
        return Err(CatalogError::Url {
            id: id.to_owned(),
            field,
            value: value.to_owned(),
        });
    }
    Ok(())
}

fn validate_slug(
    id: &str,
    field: &'static str,
    value: &str,
    allow_slash: bool,
) -> Result<(), CatalogError> {
    let safe = !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || byte == b'-'
                || byte == b'_'
                || (allow_slash && byte == b'/')
        })
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.contains("//");
    if safe {
        Ok(())
    } else {
        Err(CatalogError::UnsafeField {
            id: id.to_owned(),
            field,
            value: value.to_owned(),
        })
    }
}

fn validate_filename(id: &str, filename: &str) -> Result<(), CatalogError> {
    let path = Path::new(filename);
    let safe = !filename.is_empty()
        && path.components().count() == 1
        && matches!(path.components().next(), Some(Component::Normal(_)));
    if safe {
        Ok(())
    } else {
        Err(CatalogError::UnsafeField {
            id: id.to_owned(),
            field: "artifact.filename",
            value: filename.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_catalog_is_valid() {
        let catalog = Catalog::built_in().unwrap();
        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(catalog.entries[0].artifact.size, 32_768);
    }

    #[test]
    fn rejects_traversal_filename() {
        let mut catalog = Catalog::built_in().unwrap();
        catalog.entries[0].artifact.filename = "../../escape.gb".into();
        assert!(matches!(
            catalog.validate(),
            Err(CatalogError::UnsafeField { .. })
        ));
    }

    #[test]
    fn rejects_unverified_automatic_install() {
        let mut catalog = Catalog::built_in().unwrap();
        catalog.entries[0].trust = Trust::Unverified;
        assert!(matches!(catalog.validate(), Err(CatalogError::Trust(_))));
    }
}
