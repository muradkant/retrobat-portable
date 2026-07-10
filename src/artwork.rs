use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sha2::Digest;
use thiserror::Error;

use crate::browse::{BrowseEntry, BundledArtwork};
use crate::catalog::Artwork;
use crate::install::{
    DownloadClient, DownloadError, InstallError, digest_file, ensure_safe_parent,
};
use crate::paths::PortableLayout;

#[derive(Debug, Error)]
pub enum ArtworkError {
    #[error("artwork download failed: {0}")]
    Download(#[from] DownloadError),
    #[error("artwork cache operation failed: {0}")]
    Io(#[from] io::Error),
    #[error("artwork cache path is unsafe: {0}")]
    UnsafePath(#[from] InstallError),
    #[error("artwork size mismatch: expected {expected}, got {actual}")]
    Size { expected: u64, actual: u64 },
    #[error("artwork SHA-256 mismatch: expected {expected}, got {actual}")]
    Hash { expected: String, actual: String },
    #[error("artwork exceeds the {0} byte download limit")]
    TooLarge(usize),
    #[error("bundled artwork is not a regular file: {0}")]
    NotAFile(PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BundledArtworkAudit {
    pub declared_assets: usize,
    pub verified_assets: usize,
    pub failed_assets: usize,
    pub failure_examples: Vec<String>,
}

impl BundledArtworkAudit {
    pub fn is_complete(&self) -> bool {
        self.declared_assets == self.verified_assets && self.failed_assets == 0
    }
}

pub fn audit_bundled_artwork(
    layout: &PortableLayout,
    entries: &[BrowseEntry],
) -> BundledArtworkAudit {
    let mut report = BundledArtworkAudit {
        declared_assets: 0,
        verified_assets: 0,
        failed_assets: 0,
        failure_examples: Vec::new(),
    };
    for entry in entries {
        let Some(asset) = &entry.artwork_asset else {
            continue;
        };
        report.declared_assets += 1;
        match load_bundled_artwork(layout, asset) {
            Ok(_) => report.verified_assets += 1,
            Err(error) => {
                report.failed_assets += 1;
                if report.failure_examples.len() < 20 {
                    report
                        .failure_examples
                        .push(format!("{}: {error}", entry.id));
                }
            }
        }
    }
    report
}

pub fn load_bundled_artwork(
    layout: &PortableLayout,
    artwork: &BundledArtwork,
) -> Result<Vec<u8>, ArtworkError> {
    const MAX_BYTES: u64 = 8 * 1024 * 1024;
    let relative = PathBuf::from(&artwork.path);
    ensure_safe_parent(&layout.root, &relative)?;
    let path = layout.root.join(&relative);
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ArtworkError::NotAFile(path));
    }
    if artwork.size > MAX_BYTES || metadata.len() > MAX_BYTES {
        return Err(ArtworkError::TooLarge(MAX_BYTES as usize));
    }
    let (size, sha256) = digest_file(&path)?;
    if size != artwork.size {
        return Err(ArtworkError::Size {
            expected: artwork.size,
            actual: size,
        });
    }
    if sha256 != artwork.sha256 {
        return Err(ArtworkError::Hash {
            expected: artwork.sha256.clone(),
            actual: sha256,
        });
    }
    Ok(fs::read(path)?)
}

pub fn load_snapshot_artwork<D: DownloadClient>(
    layout: &PortableLayout,
    url: &str,
    downloader: &D,
) -> Result<Vec<u8>, ArtworkError> {
    const MAX_BYTES: usize = 8 * 1024 * 1024;
    let url_hash = hex::encode(sha2::Sha256::digest(url.as_bytes()));
    let relative = PathBuf::from(".retrobat-portable")
        .join("cache")
        .join("browse-artwork")
        .join(format!("{url_hash}.image"));
    ensure_safe_parent(&layout.root, &relative)?;
    let cache_path = layout.root.join(&relative);
    if cache_path.is_file() {
        let metadata = fs::metadata(&cache_path)?;
        if metadata.len() <= MAX_BYTES as u64 {
            return Ok(fs::read(cache_path)?);
        }
        fs::remove_file(&cache_path)?;
    }

    let mut bytes = LimitedBytes::new(MAX_BYTES);
    downloader.fetch(url, &mut bytes)?;
    let bytes = bytes.into_inner()?;
    let temporary = cache_path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| {
        let mut output = File::create(&temporary)?;
        output.write_all(&bytes)?;
        output.sync_all()?;
        drop(output);
        fs::rename(&temporary, &cache_path)?;
        Ok(bytes)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

struct LimitedBytes {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}

impl LimitedBytes {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            exceeded: false,
        }
    }

    fn into_inner(self) -> Result<Vec<u8>, ArtworkError> {
        if self.exceeded {
            Err(ArtworkError::TooLarge(self.limit))
        } else {
            Ok(self.bytes)
        }
    }
}

impl Write for LimitedBytes {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let remaining = self.limit.saturating_sub(self.bytes.len());
        if buffer.len() > remaining {
            self.exceeded = true;
            return Err(io::Error::other("artwork download limit exceeded"));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn load_or_fetch<D: DownloadClient>(
    layout: &PortableLayout,
    artwork: &Artwork,
    downloader: &D,
) -> Result<Vec<u8>, ArtworkError> {
    let relative = PathBuf::from(".retrobat-portable")
        .join("cache")
        .join("artwork")
        .join(format!("{}.image", artwork.sha256));
    ensure_safe_parent(&layout.root, &relative)?;
    let cache_path = layout.root.join(&relative);

    if cache_path.is_file() {
        match verify(&cache_path, artwork) {
            Ok(()) => return Ok(fs::read(cache_path)?),
            Err(ArtworkError::Size { .. } | ArtworkError::Hash { .. }) => {
                fs::remove_file(&cache_path)?;
            }
            Err(error) => return Err(error),
        }
    }

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = cache_path.with_extension(format!("{}.tmp", unique));
    let result = (|| {
        let mut output = File::create(&temporary)?;
        downloader.fetch(&artwork.url, &mut output)?;
        output.sync_all()?;
        drop(output);
        verify(&temporary, artwork)?;
        fs::rename(&temporary, &cache_path)?;
        Ok(fs::read(&cache_path)?)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn verify(path: &std::path::Path, artwork: &Artwork) -> Result<(), ArtworkError> {
    let (actual_size, actual_hash) = digest_file(path)?;
    if actual_size != artwork.size {
        return Err(ArtworkError::Size {
            expected: artwork.size,
            actual: actual_size,
        });
    }
    if actual_hash != artwork.sha256 {
        return Err(ArtworkError::Hash {
            expected: artwork.sha256.clone(),
            actual: actual_hash,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;
    use crate::install::DownloadClient;
    use sha2::{Digest, Sha256};
    use tempfile::tempdir;

    struct BytesDownloader(Vec<u8>);

    impl DownloadClient for BytesDownloader {
        fn fetch(&self, _url: &str, output: &mut dyn Write) -> Result<(), DownloadError> {
            output
                .write_all(&self.0)
                .map_err(|error| DownloadError::new(error.to_string()))
        }
    }

    #[test]
    fn caches_only_verified_artwork() {
        let temp = tempdir().unwrap();
        let layout = PortableLayout::new(temp.path());
        let bytes = b"image fixture".to_vec();
        let mut artwork = Catalog::built_in().unwrap().entries[0].artwork[0].clone();
        artwork.size = bytes.len() as u64;
        artwork.sha256 = hex::encode(Sha256::digest(&bytes));
        let downloader = BytesDownloader(bytes.clone());

        assert_eq!(
            load_or_fetch(&layout, &artwork, &downloader).unwrap(),
            bytes
        );
        assert_eq!(
            load_or_fetch(
                &layout,
                &artwork,
                &BytesDownloader(b"network must not be used".to_vec())
            )
            .unwrap(),
            bytes
        );
    }

    #[test]
    fn rejects_and_removes_corrupt_artwork() {
        let temp = tempdir().unwrap();
        let layout = PortableLayout::new(temp.path());
        let artwork = Catalog::built_in().unwrap().entries[0].artwork[0].clone();

        assert!(matches!(
            load_or_fetch(
                &layout,
                &artwork,
                &BytesDownloader(b"not an image".to_vec())
            ),
            Err(ArtworkError::Size { .. })
        ));
        let cache = layout
            .metadata_root()
            .join("cache/artwork")
            .join(format!("{}.image", artwork.sha256));
        assert!(!cache.exists());
    }

    #[test]
    fn loads_only_the_declared_bundled_artwork_asset() {
        let temp = tempdir().unwrap();
        let layout = PortableLayout::new(temp.path());
        let path = layout.root.join("Artwork/mame/example.png");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"local artwork").unwrap();
        let asset = BundledArtwork {
            path: "Artwork/mame/example.png".to_owned(),
            size: 13,
            sha256: hex::encode(Sha256::digest(b"local artwork")),
        };

        assert_eq!(
            load_bundled_artwork(&layout, &asset).unwrap(),
            b"local artwork"
        );
    }

    #[test]
    fn bundled_artwork_audit_reports_missing_assets_instead_of_claiming_coverage() {
        let temp = tempdir().unwrap();
        let layout = PortableLayout::new(temp.path());
        let mut entry = crate::browse::BrowseCatalog::built_in()
            .unwrap()
            .entries
            .into_iter()
            .next()
            .unwrap();
        entry.artwork_asset = Some(BundledArtwork {
            path: "Artwork/mame/missing.png".to_owned(),
            size: 1,
            sha256: "0".repeat(64),
        });

        let report = audit_bundled_artwork(&layout, &[entry]);

        assert_eq!(report.declared_assets, 1);
        assert_eq!(report.failed_assets, 1);
        assert!(!report.is_complete());
    }
}
