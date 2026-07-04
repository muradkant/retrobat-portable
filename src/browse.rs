use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

const HOMEBREW_HUB: &str = include_str!("../catalog/homebrew-hub-browse-v1.json");

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BrowseCatalog {
    pub schema_version: u32,
    pub generated_at: String,
    pub source: BrowseSource,
    pub entries: Vec<BrowseEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BrowseSource {
    pub id: String,
    pub name: String,
    pub homepage: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BrowseEntry {
    pub id: String,
    pub title: String,
    pub developer: String,
    pub system: String,
    pub kind: String,
    pub tags: Vec<String>,
    pub license: Option<String>,
    pub artwork_url: Option<String>,
    pub repository: Option<String>,
    pub install_state: InstallState,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InstallState {
    Verified,
    AuditRequired,
    BrowseOnly,
}

#[derive(Debug, Error)]
pub enum BrowseError {
    #[error("browse snapshot is invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported browse snapshot schema {0}")]
    Schema(u32),
    #[error("duplicate or empty browse id: {0}")]
    Id(String),
    #[error("browse entry {id} has an invalid artwork URL: {url}")]
    ArtworkUrl { id: String, url: String },
}

impl BrowseCatalog {
    pub fn homebrew_hub() -> Result<Self, BrowseError> {
        let catalog: Self = serde_json::from_str(HOMEBREW_HUB)?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn validate(&self) -> Result<(), BrowseError> {
        if self.schema_version != 1 {
            return Err(BrowseError::Schema(self.schema_version));
        }
        let mut ids = HashSet::new();
        for entry in &self.entries {
            if entry.id.is_empty() || !ids.insert(entry.id.as_str()) {
                return Err(BrowseError::Id(entry.id.clone()));
            }
            if let Some(url) = &entry.artwork_url {
                let parsed = Url::parse(url).map_err(|_| BrowseError::ArtworkUrl {
                    id: entry.id.clone(),
                    url: url.clone(),
                })?;
                if parsed.scheme() != "https" || parsed.host_str().is_none() {
                    return Err(BrowseError::ArtworkUrl {
                        id: entry.id.clone(),
                        url: url.clone(),
                    });
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_homebrew_snapshot_is_complete_and_valid() {
        let catalog = BrowseCatalog::homebrew_hub().unwrap();
        assert_eq!(catalog.entries.len(), 1_569);
        assert!(
            catalog
                .entries
                .iter()
                .filter(|entry| entry.artwork_url.is_some())
                .count()
                >= 1_548
        );
        assert_eq!(
            catalog
                .entries
                .iter()
                .filter(|entry| entry.install_state == InstallState::Verified)
                .count(),
            1
        );
    }
}
