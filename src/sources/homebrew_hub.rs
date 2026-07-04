use serde::Deserialize;
use thiserror::Error;

use crate::catalog::CatalogEntry;

const API_ROOT: &str = "https://hh3.gbdev.io/api";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct HomebrewHubEntry {
    pub developer: String,
    pub files: Vec<HomebrewHubFile>,
    pub license: String,
    pub platform: String,
    pub repository: String,
    #[serde(default)]
    pub screenshots: Vec<String>,
    pub slug: String,
    pub title: String,
    pub typetag: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct HomebrewHubFile {
    #[serde(default)]
    pub default: bool,
    pub filename: String,
    #[serde(default)]
    pub playable: bool,
}

#[derive(Debug, Error)]
pub enum HomebrewHubError {
    #[error("Homebrew Hub request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Homebrew Hub returned invalid metadata: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Homebrew Hub metadata drift for {field}: expected {expected}, got {actual}")]
    Drift {
        field: &'static str,
        expected: String,
        actual: String,
    },
    #[error("Homebrew Hub has no default playable file for {0}")]
    NoPlayableFile(String),
}

impl HomebrewHubEntry {
    pub fn parse(json: &str) -> Result<Self, HomebrewHubError> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn fetch(slug: &str) -> Result<Self, HomebrewHubError> {
        let url = format!("{API_ROOT}/entry/{slug}.json");
        let response = reqwest::blocking::Client::builder()
            .user_agent(concat!("retrobat-portable/", env!("CARGO_PKG_VERSION")))
            .build()?
            .get(url)
            .send()?
            .error_for_status()?
            .text()?;
        Self::parse(&response)
    }

    pub fn audit_against(&self, trusted: &CatalogEntry) -> Result<(), HomebrewHubError> {
        compare("title", &trusted.title, &self.title)?;
        compare("developer", &trusted.developer, &self.developer)?;
        compare("license", &trusted.license.name, &self.license)?;
        compare("repository", &trusted.source.repository, &self.repository)?;
        compare(
            "system",
            &trusted.system,
            &self.platform.to_ascii_lowercase(),
        )?;

        let playable = self
            .files
            .iter()
            .find(|file| file.default && file.playable)
            .ok_or_else(|| HomebrewHubError::NoPlayableFile(self.slug.clone()))?;
        compare(
            "artifact.filename",
            &trusted.artifact.filename,
            &playable.filename,
        )?;
        if let Some(artwork) = trusted.artwork.first()
            && !self.screenshots.contains(&artwork.filename)
        {
            return Err(HomebrewHubError::Drift {
                field: "artwork.filename",
                expected: artwork.filename.clone(),
                actual: self.screenshots.join(", "),
            });
        }
        Ok(())
    }
}

fn compare(field: &'static str, expected: &str, actual: &str) -> Result<(), HomebrewHubError> {
    if expected == actual {
        Ok(())
    } else {
        Err(HomebrewHubError::Drift {
            field,
            expected: expected.to_owned(),
            actual: actual.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;

    const FIXTURE: &str = r#"{
      "developer": "Sanqui",
      "files": [{"default": true, "filename": "2048.gb", "playable": true}],
      "license": "Zlib",
      "platform": "GB",
      "repository": "https://github.com/Sanqui/2048-gb",
      "screenshots": ["1.png", "2.png"],
      "slug": "2048gb",
      "title": "2048gb",
      "typetag": "game"
    }"#;

    #[test]
    fn parses_and_audits_the_source_fixture() {
        let upstream = HomebrewHubEntry::parse(FIXTURE).unwrap();
        let trusted = Catalog::built_in().unwrap().entries.remove(0);
        upstream.audit_against(&trusted).unwrap();
    }

    #[test]
    fn detects_upstream_metadata_drift() {
        let mut upstream = HomebrewHubEntry::parse(FIXTURE).unwrap();
        upstream.license = "Unknown".into();
        let trusted = Catalog::built_in().unwrap().entries.remove(0);
        assert!(matches!(
            upstream.audit_against(&trusted),
            Err(HomebrewHubError::Drift {
                field: "license",
                ..
            })
        ));
    }
}
