use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::catalog::{CatalogEntry, CatalogError};
use crate::paths::PortableLayout;

pub trait DownloadClient {
    fn fetch(&self, url: &str, output: &mut dyn Write) -> Result<(), DownloadError>;
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct DownloadError {
    message: String,
}

impl DownloadError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub struct ReqwestDownloader {
    client: reqwest::blocking::Client,
}

impl ReqwestDownloader {
    pub fn new() -> Result<Self, InstallError> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!(
                "retrobat-portable/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/gbdev/homebrewhub)"
            ))
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|error| InstallError::Download(DownloadError::new(error.to_string())))?;
        Ok(Self { client })
    }
}

impl DownloadClient for ReqwestDownloader {
    fn fetch(&self, url: &str, output: &mut dyn Write) -> Result<(), DownloadError> {
        let mut response = self
            .client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|error| DownloadError::new(error.to_string()))?;
        io::copy(&mut response, output).map_err(|error| DownloadError::new(error.to_string()))?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum InstallError {
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error("download failed: {0}")]
    Download(#[from] DownloadError),
    #[error("filesystem operation failed: {0}")]
    Io(#[from] io::Error),
    #[error("downloaded size mismatch: expected {expected}, got {actual}")]
    Size { expected: u64, actual: u64 },
    #[error("downloaded SHA-256 mismatch: expected {expected}, got {actual}")]
    Hash { expected: String, actual: String },
    #[error("unsafe destination path: {0}")]
    UnsafePath(PathBuf),
    #[error("destination already exists and is not owned by this installer: {0}")]
    DestinationExists(PathBuf),
    #[error("installed manifest is invalid: {0}")]
    Manifest(#[from] serde_json::Error),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InstalledManifest {
    pub schema_version: u32,
    pub catalog_id: String,
    pub relative_path: PathBuf,
    pub sha256: String,
    pub size: u64,
    pub installed_at_unix: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallReport {
    pub destination: PathBuf,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UninstallReport {
    pub removed: Vec<PathBuf>,
    pub preserved_modified: Vec<PathBuf>,
    pub already_missing: Vec<PathBuf>,
}

pub struct Installer<'a, D: DownloadClient> {
    layout: &'a PortableLayout,
    downloader: &'a D,
}

impl<'a, D: DownloadClient> Installer<'a, D> {
    pub fn new(layout: &'a PortableLayout, downloader: &'a D) -> Self {
        Self { layout, downloader }
    }

    pub fn install(&self, entry: &CatalogEntry) -> Result<InstallReport, InstallError> {
        entry.validate()?;
        let relative = entry.install_relative_path();
        validate_relative_path(&relative)?;

        let destination = self.layout.root.join(&relative);
        ensure_safe_parent(&self.layout.root, &relative)?;
        if fs::symlink_metadata(&destination).is_ok() {
            return Err(InstallError::DestinationExists(destination));
        }

        let operation_id = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let stage_dir = self.layout.staging_root().join(operation_id);
        fs::create_dir_all(&stage_dir)?;
        let staged = stage_dir.join("artifact.download");

        let result = (|| {
            let mut output = File::create(&staged)?;
            self.downloader.fetch(&entry.artifact.url, &mut output)?;
            output.sync_all()?;
            drop(output);

            let (actual_size, actual_hash) = digest_file(&staged)?;
            if actual_size != entry.artifact.size {
                return Err(InstallError::Size {
                    expected: entry.artifact.size,
                    actual: actual_size,
                });
            }
            if actual_hash != entry.artifact.sha256.to_ascii_lowercase() {
                return Err(InstallError::Hash {
                    expected: entry.artifact.sha256.clone(),
                    actual: actual_hash,
                });
            }

            fs::rename(&staged, &destination)?;

            let manifest = InstalledManifest {
                schema_version: 1,
                catalog_id: entry.id.clone(),
                relative_path: relative,
                sha256: actual_hash.clone(),
                size: actual_size,
                installed_at_unix: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            };
            if let Err(error) = write_manifest(self.layout, &manifest) {
                let _ = fs::remove_file(&destination);
                return Err(error);
            }

            Ok(InstallReport {
                destination,
                bytes: actual_size,
                sha256: actual_hash,
            })
        })();

        let _ = fs::remove_dir_all(&stage_dir);
        result
    }

    pub fn uninstall(&self, entry: &CatalogEntry) -> Result<UninstallReport, InstallError> {
        let manifest_path = manifest_path(self.layout, &entry.id);
        let manifest: InstalledManifest = serde_json::from_reader(File::open(&manifest_path)?)?;
        validate_relative_path(&manifest.relative_path)?;
        let destination = self.layout.root.join(&manifest.relative_path);
        let mut report = UninstallReport::default();

        match fs::symlink_metadata(&destination) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                report.already_missing.push(destination);
                fs::remove_file(manifest_path)?;
            }
            Err(error) => return Err(error.into()),
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                report.preserved_modified.push(destination);
            }
            Ok(_) => {
                let (_, actual_hash) = digest_file(&destination)?;
                if actual_hash == manifest.sha256 {
                    fs::remove_file(&destination)?;
                    report.removed.push(destination);
                    fs::remove_file(manifest_path)?;
                } else {
                    report.preserved_modified.push(destination);
                }
            }
        }
        Ok(report)
    }

    pub fn is_installed(&self, entry: &CatalogEntry) -> bool {
        is_installed(self.layout, entry)
    }
}

pub fn is_installed(layout: &PortableLayout, entry: &CatalogEntry) -> bool {
    manifest_path(layout, &entry.id).is_file()
}

pub(crate) fn digest_file(path: &Path) -> Result<(u64, String), io::Error> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size += read as u64;
        hasher.update(&buffer[..read]);
    }
    Ok((size, hex::encode(hasher.finalize())))
}

fn manifest_path(layout: &PortableLayout, id: &str) -> PathBuf {
    layout
        .installed_root()
        .join(format!("{}.json", id.replace('/', "--")))
}

fn write_manifest(
    layout: &PortableLayout,
    manifest: &InstalledManifest,
) -> Result<(), InstallError> {
    fs::create_dir_all(layout.installed_root())?;
    let final_path = manifest_path(layout, &manifest.catalog_id);
    let temporary = final_path.with_extension("json.tmp");
    let mut output = File::create(&temporary)?;
    serde_json::to_writer_pretty(&mut output, manifest)?;
    output.write_all(b"\n")?;
    output.sync_all()?;
    drop(output);
    fs::rename(temporary, final_path)?;
    Ok(())
}

fn validate_relative_path(path: &Path) -> Result<(), InstallError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(InstallError::UnsafePath(path.to_owned()));
    }
    Ok(())
}

pub(crate) fn ensure_safe_parent(root: &Path, relative: &Path) -> Result<(), InstallError> {
    validate_relative_path(relative)?;
    fs::create_dir_all(root)?;
    let mut current = root.to_owned();
    let parent = relative
        .parent()
        .ok_or_else(|| InstallError::UnsafePath(relative.to_owned()))?;
    for component in parent.components() {
        let Component::Normal(name) = component else {
            return Err(InstallError::UnsafePath(relative.to_owned()));
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(InstallError::UnsafePath(current));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;
    use tempfile::tempdir;

    struct BytesDownloader(Vec<u8>);

    impl DownloadClient for BytesDownloader {
        fn fetch(&self, _url: &str, output: &mut dyn Write) -> Result<(), DownloadError> {
            output
                .write_all(&self.0)
                .map_err(|error| DownloadError::new(error.to_string()))
        }
    }

    fn fixture_entry(bytes: &[u8]) -> CatalogEntry {
        let mut entry = Catalog::built_in().unwrap().entries.remove(0);
        entry.artifact.size = bytes.len() as u64;
        entry.artifact.sha256 = hex::encode(Sha256::digest(bytes));
        entry
    }

    #[test]
    fn installs_verifies_records_and_uninstalls() {
        let temp = tempdir().unwrap();
        let layout = PortableLayout::new(temp.path());
        let bytes = b"valid ROM fixture".to_vec();
        let downloader = BytesDownloader(bytes.clone());
        let installer = Installer::new(&layout, &downloader);
        let entry = fixture_entry(&bytes);

        let installed = installer.install(&entry).unwrap();
        assert_eq!(fs::read(&installed.destination).unwrap(), bytes);
        assert!(installer.is_installed(&entry));

        let removed = installer.uninstall(&entry).unwrap();
        assert_eq!(removed.removed, vec![installed.destination.clone()]);
        assert!(!installed.destination.exists());
        assert!(!installer.is_installed(&entry));
    }

    #[test]
    fn hash_mismatch_rolls_back_without_an_owned_file() {
        let temp = tempdir().unwrap();
        let layout = PortableLayout::new(temp.path());
        let entry = fixture_entry(b"expected");
        let downloader = BytesDownloader(b"tampered".to_vec());
        let installer = Installer::new(&layout, &downloader);

        assert!(matches!(
            installer.install(&entry),
            Err(InstallError::Hash { .. })
        ));
        assert!(!layout.root.join(entry.install_relative_path()).exists());
        assert!(!installer.is_installed(&entry));
    }

    #[test]
    fn uninstall_preserves_a_user_modified_file() {
        let temp = tempdir().unwrap();
        let layout = PortableLayout::new(temp.path());
        let bytes = b"original".to_vec();
        let downloader = BytesDownloader(bytes.clone());
        let installer = Installer::new(&layout, &downloader);
        let entry = fixture_entry(&bytes);
        let installed = installer.install(&entry).unwrap();
        fs::write(&installed.destination, b"user save or modification").unwrap();

        let report = installer.uninstall(&entry).unwrap();
        assert_eq!(
            report.preserved_modified,
            vec![installed.destination.clone()]
        );
        assert!(installed.destination.exists());
        assert!(installer.is_installed(&entry));
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_symlinked_destination_directory() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let layout = PortableLayout::new(temp.path());
        fs::create_dir_all(layout.root.join("RetroBat/roms")).unwrap();
        symlink(outside.path(), layout.root.join("RetroBat/roms/gb")).unwrap();
        let bytes = b"valid".to_vec();
        let downloader = BytesDownloader(bytes.clone());
        let installer = Installer::new(&layout, &downloader);

        assert!(matches!(
            installer.install(&fixture_entry(&bytes)),
            Err(InstallError::UnsafePath(_))
        ));
        assert!(!outside.path().join("2048.gb").exists());
    }
}
