use std::fs::{self, File};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;

use crate::install::{
    DownloadClient, DownloadError, InstallError, digest_file, ensure_safe_parent,
};
use crate::paths::PortableLayout;
use crate::readiness::FirmwareFileStatus;

#[derive(Debug, Error)]
pub enum FirmwareImportError {
    #[error("the selected firmware path is not a regular file: {0}")]
    NotAFile(PathBuf),
    #[error("the firmware destination is unsafe: {0}")]
    UnsafeDestination(String),
    #[error("filesystem operation failed: {0}")]
    Io(#[from] io::Error),
    #[error("destination safety check failed: {0}")]
    Safety(#[from] InstallError),
    #[error("this firmware record has no publisher-authorized download")]
    NotDownloadable,
    #[error("publisher download failed: {0}")]
    Download(#[from] DownloadError),
    #[error("publisher download size mismatch: expected {expected}, got {actual}")]
    Size { expected: u64, actual: u64 },
    #[error("publisher download SHA-256 mismatch: expected {expected}, got {actual}")]
    Hash { expected: String, actual: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareImportReport {
    pub destination: PathBuf,
    pub bytes: u64,
    pub sha256: String,
    pub replaced_existing: bool,
}

pub fn install_official_firmware<D: DownloadClient>(
    layout: &PortableLayout,
    firmware: &FirmwareFileStatus,
    downloader: &D,
) -> Result<FirmwareImportReport, FirmwareImportError> {
    let download = firmware
        .download
        .as_ref()
        .ok_or(FirmwareImportError::NotDownloadable)?;
    let operation = format!(
        "firmware-download-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let stage = layout.staging_root().join(operation);
    fs::create_dir_all(&stage)?;
    let staged = stage.join("publisher-download");
    let result = (|| {
        let mut output = File::create(&staged)?;
        downloader.fetch(&download.url, &mut output)?;
        output.sync_all()?;
        drop(output);

        let (actual_size, actual_hash) = digest_file(&staged)?;
        if actual_size != download.size {
            return Err(FirmwareImportError::Size {
                expected: download.size,
                actual: actual_size,
            });
        }
        if actual_hash != download.sha256.to_ascii_lowercase() {
            return Err(FirmwareImportError::Hash {
                expected: download.sha256.clone(),
                actual: actual_hash,
            });
        }
        import_firmware(layout, firmware, &staged)
    })();
    let _ = fs::remove_dir_all(stage);
    result
}

pub fn import_firmware(
    layout: &PortableLayout,
    firmware: &FirmwareFileStatus,
    source: &Path,
) -> Result<FirmwareImportReport, FirmwareImportError> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(FirmwareImportError::NotAFile(source.to_owned()));
    }
    let firmware_path = Path::new(&firmware.relative_path);
    if firmware_path.as_os_str().is_empty()
        || firmware_path.is_absolute()
        || firmware_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(FirmwareImportError::UnsafeDestination(
            firmware.relative_path.clone(),
        ));
    }
    let relative_destination = if firmware.directory {
        let Some(filename) = source.file_name() else {
            return Err(FirmwareImportError::NotAFile(source.to_owned()));
        };
        if !matches!(
            Path::new(filename).components().next(),
            Some(Component::Normal(_))
        ) {
            return Err(FirmwareImportError::UnsafeDestination(
                source.display().to_string(),
            ));
        }
        Path::new("RetroBat")
            .join("bios")
            .join(firmware_path)
            .join(filename)
    } else {
        Path::new("RetroBat").join("bios").join(firmware_path)
    };
    ensure_safe_parent(&layout.root, &relative_destination)?;
    let destination = layout.root.join(&relative_destination);
    let replaced_existing = match fs::symlink_metadata(&destination) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(FirmwareImportError::UnsafeDestination(
                destination.display().to_string(),
            ));
        }
        Ok(_) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };

    let operation = format!(
        "firmware-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let stage = layout.staging_root().join(operation);
    fs::create_dir_all(&stage)?;
    let staged = stage.join("payload");
    let previous = stage.join("previous");
    let result = (|| {
        fs::copy(source, &staged)?;
        let (bytes, sha256) = digest_file(&staged)?;
        if bytes == 0 {
            return Err(FirmwareImportError::NotAFile(source.to_owned()));
        }
        if replaced_existing {
            fs::rename(&destination, &previous)?;
        }
        if let Err(error) = fs::rename(&staged, &destination) {
            if replaced_existing {
                let _ = fs::rename(&previous, &destination);
            }
            return Err(error.into());
        }
        mirror_emulator_firmware(layout, firmware, &destination)?;
        Ok(FirmwareImportReport {
            destination,
            bytes,
            sha256,
            replaced_existing,
        })
    })();
    let _ = fs::remove_dir_all(stage);
    result
}

fn mirror_emulator_firmware(
    layout: &PortableLayout,
    firmware: &FirmwareFileStatus,
    source: &Path,
) -> Result<(), io::Error> {
    if firmware.relative_path == "eden/keys/prod.keys" {
        let destinations = [
            layout
                .emulator_root("eden")
                .join("user")
                .join("keys")
                .join("prod.keys"),
            layout
                .metadata_root()
                .join("runtime/linux/eden/data/eden/keys/prod.keys"),
        ];
        for destination in destinations {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(source, destination)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readiness::{FirmwareDownload, FirmwareInstallAction};
    use sha2::{Digest, Sha256};

    struct BytesDownloader(Vec<u8>);

    impl DownloadClient for BytesDownloader {
        fn fetch(&self, _url: &str, output: &mut dyn io::Write) -> Result<(), DownloadError> {
            output
                .write_all(&self.0)
                .map_err(|error| DownloadError::new(error.to_string()))
        }
    }

    fn downloadable_firmware(bytes: &[u8]) -> FirmwareFileStatus {
        FirmwareFileStatus {
            relative_path: "PS3UPDAT.PUP".to_owned(),
            description: "Publisher firmware".to_owned(),
            directory: false,
            optional: false,
            present: false,
            guidance_url: "https://publisher.example/firmware".to_owned(),
            guidance: String::new(),
            download: Some(FirmwareDownload {
                publisher: "Publisher".to_owned(),
                source_url: "https://publisher.example/firmware".to_owned(),
                url: "https://publisher.example/firmware.bin".to_owned(),
                size: bytes.len() as u64,
                sha256: hex::encode(Sha256::digest(bytes)),
                install_action: FirmwareInstallAction::Rpcs3,
            }),
        }
    }

    #[test]
    fn official_firmware_download_is_verified_then_installed() {
        let root = tempfile::tempdir().unwrap();
        let layout = PortableLayout::new(root.path());
        fs::create_dir_all(layout.retrobat_root().join("bios")).unwrap();
        let bytes = b"publisher supplied firmware";
        let firmware = downloadable_firmware(bytes);

        let report =
            install_official_firmware(&layout, &firmware, &BytesDownloader(bytes.to_vec()))
                .unwrap();

        assert_eq!(report.bytes, bytes.len() as u64);
        assert_eq!(
            fs::read(layout.retrobat_root().join("bios/PS3UPDAT.PUP")).unwrap(),
            bytes
        );
    }

    #[test]
    fn official_firmware_download_rejects_unexpected_bytes() {
        let root = tempfile::tempdir().unwrap();
        let layout = PortableLayout::new(root.path());
        fs::create_dir_all(layout.retrobat_root().join("bios")).unwrap();
        let firmware = downloadable_firmware(b"expected");

        assert!(matches!(
            install_official_firmware(&layout, &firmware, &BytesDownloader(b"unexpected".to_vec())),
            Err(FirmwareImportError::Size { .. } | FirmwareImportError::Hash { .. })
        ));
        assert!(!layout.retrobat_root().join("bios/PS3UPDAT.PUP").exists());
    }

    #[test]
    fn imports_and_renames_owner_supplied_firmware_into_the_portable_bios_tree() {
        let root = tempfile::tempdir().unwrap();
        let layout = PortableLayout::new(root.path());
        fs::create_dir_all(layout.retrobat_root().join("bios")).unwrap();
        let source = root.path().join("my-console-dump.bin");
        fs::write(&source, b"firmware").unwrap();
        let firmware = FirmwareFileStatus {
            relative_path: "kronos/saturn_bios.bin".to_owned(),
            description: "Saturn BIOS".to_owned(),
            directory: false,
            optional: false,
            present: false,
            guidance_url: String::new(),
            guidance: String::new(),
            download: None,
        };
        let report = import_firmware(&layout, &firmware, &source).unwrap();
        assert_eq!(
            report.destination,
            layout.retrobat_root().join("bios/kronos/saturn_bios.bin")
        );
        assert_eq!(fs::read(report.destination).unwrap(), b"firmware");
        assert!(!report.replaced_existing);
    }

    #[test]
    fn replaces_an_existing_firmware_file_without_hash_or_filename_gating() {
        let root = tempfile::tempdir().unwrap();
        let layout = PortableLayout::new(root.path());
        fs::create_dir_all(layout.retrobat_root().join("bios")).unwrap();
        let destination = layout.retrobat_root().join("bios/console.bin");
        fs::write(&destination, b"old bytes").unwrap();
        let source = root.path().join("arbitrary-source-name.rom");
        fs::write(&source, b"different nonempty bytes").unwrap();
        let firmware = FirmwareFileStatus {
            relative_path: "console.bin".to_owned(),
            description: "Console firmware".to_owned(),
            directory: false,
            optional: true,
            present: true,
            guidance_url: String::new(),
            guidance: String::new(),
            download: None,
        };

        let report = import_firmware(&layout, &firmware, &source).unwrap();

        assert!(report.replaced_existing);
        assert_eq!(fs::read(destination).unwrap(), b"different nonempty bytes");
    }

    #[test]
    fn rejects_a_traversal_destination_from_untrusted_metadata() {
        let root = tempfile::tempdir().unwrap();
        let layout = PortableLayout::new(root.path());
        let source = root.path().join("firmware.bin");
        fs::write(&source, b"firmware").unwrap();
        let firmware = FirmwareFileStatus {
            relative_path: "../outside.bin".to_owned(),
            description: String::new(),
            directory: false,
            optional: false,
            present: false,
            guidance_url: String::new(),
            guidance: String::new(),
            download: None,
        };
        assert!(matches!(
            import_firmware(&layout, &firmware, &source),
            Err(FirmwareImportError::UnsafeDestination(_))
        ));
    }

    #[test]
    fn imports_any_nonempty_filename_into_a_firmware_directory() {
        let root = tempfile::tempdir().unwrap();
        let layout = PortableLayout::new(root.path());
        fs::create_dir_all(layout.retrobat_root().join("bios/pcsx2/bios")).unwrap();
        let source = root.path().join("my-unknown-ps2-dump.bin");
        fs::write(&source, b"owner supplied bytes").unwrap();
        let firmware = FirmwareFileStatus {
            relative_path: "pcsx2/bios".to_owned(),
            description: "'pcsx2/bios' folder".to_owned(),
            directory: true,
            optional: false,
            present: false,
            guidance_url: String::new(),
            guidance: String::new(),
            download: None,
        };

        let report = import_firmware(&layout, &firmware, &source).unwrap();

        assert_eq!(
            report.destination,
            layout
                .retrobat_root()
                .join("bios/pcsx2/bios/my-unknown-ps2-dump.bin")
        );
        assert_eq!(
            fs::read(report.destination).unwrap(),
            b"owner supplied bytes"
        );
    }

    #[test]
    fn switch_keys_are_mirrored_into_both_portable_eden_profiles() {
        let root = tempfile::tempdir().unwrap();
        let layout = PortableLayout::new(root.path());
        let source = root.path().join("my-prod.keys");
        fs::write(&source, b"owner dumped keys").unwrap();
        let firmware = FirmwareFileStatus {
            relative_path: "eden/keys/prod.keys".to_owned(),
            description: "Switch keys".to_owned(),
            directory: false,
            optional: false,
            present: false,
            guidance_url: String::new(),
            guidance: String::new(),
            download: None,
        };

        import_firmware(&layout, &firmware, &source).unwrap();

        assert_eq!(
            fs::read(layout.emulator_root("eden").join("user/keys/prod.keys")).unwrap(),
            b"owner dumped keys"
        );
        assert_eq!(
            fs::read(
                layout
                    .metadata_root()
                    .join("runtime/linux/eden/data/eden/keys/prod.keys")
            )
            .unwrap(),
            b"owner dumped keys"
        );
    }
}
