use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct PortableLayout {
    pub root: PathBuf,
}

impl PortableLayout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn from_executable(executable: &Path) -> Self {
        Self::new(
            executable
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new(".")),
        )
    }

    pub fn retrobat_root(&self) -> PathBuf {
        self.root.join("RetroBat")
    }

    pub fn retrobat_executable(&self) -> PathBuf {
        self.retrobat_root().join("RetroBat.exe")
    }

    pub fn metadata_root(&self) -> PathBuf {
        self.root.join(".retrobat-portable")
    }

    pub fn installed_root(&self) -> PathBuf {
        self.metadata_root().join("installed")
    }

    pub fn staging_root(&self) -> PathBuf {
        self.metadata_root().join("staging")
    }
}
