pub mod artwork;
pub mod browse;
pub mod catalog;
pub mod install;
pub mod launch;
pub mod paths;
pub mod sources;

use serde::Serialize;
use thiserror::Error;

use crate::{browse::BrowseCatalog, catalog::Catalog, launch::HostPlatform, paths::PortableLayout};

#[derive(Debug, Serialize)]
pub struct SelfCheck {
    pub catalog_entries: usize,
    pub browse_entries: usize,
    pub browse_artwork_entries: usize,
    pub catalog_valid: bool,
    pub bundle_root: String,
    pub retrobat_present: bool,
    pub target_platform: &'static str,
}

#[derive(Debug, Error)]
pub enum SelfCheckError {
    #[error(transparent)]
    Catalog(#[from] catalog::CatalogError),
    #[error(transparent)]
    Browse(#[from] browse::BrowseError),
}

pub fn self_check(layout: &PortableLayout) -> Result<SelfCheck, SelfCheckError> {
    let catalog = Catalog::built_in()?;
    let browse = BrowseCatalog::homebrew_hub()?;
    let host = HostPlatform::current();
    Ok(SelfCheck {
        catalog_entries: catalog.entries.len(),
        browse_entries: browse.entries.len(),
        browse_artwork_entries: browse
            .entries
            .iter()
            .filter(|entry| entry.artwork_url.is_some())
            .count(),
        catalog_valid: true,
        bundle_root: layout.root.display().to_string(),
        retrobat_present: layout.retrobat_executable().is_file(),
        target_platform: host.as_str(),
    })
}
