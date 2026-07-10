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

    pub fn emulationstation_root(&self) -> PathBuf {
        self.retrobat_root().join("emulationstation")
    }

    pub fn emulator_launcher_executable(&self) -> PathBuf {
        self.emulationstation_root().join("emulatorLauncher.exe")
    }

    pub fn retroarch_root(&self) -> PathBuf {
        self.emulator_root("retroarch")
    }

    pub fn emulator_root(&self, emulator: &str) -> PathBuf {
        self.retrobat_root().join("emulators").join(emulator)
    }

    pub fn linux_runtime_root(&self) -> PathBuf {
        self.root.join("Runtime").join("Linux")
    }

    pub fn rpcs3_executable(&self) -> PathBuf {
        self.emulator_root("rpcs3").join("rpcs3.exe")
    }

    pub fn retroarch_executable(&self) -> PathBuf {
        self.retroarch_root().join("retroarch.exe")
    }

    pub fn retroarch_core(&self, core: &str) -> PathBuf {
        self.retroarch_root()
            .join("cores")
            .join(format!("{core}_libretro.dll"))
    }

    pub fn retroarch_core_info(&self, core: &str) -> PathBuf {
        self.retroarch_root()
            .join("info")
            .join(format!("{core}_libretro.info"))
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

    pub fn imported_root(&self) -> PathBuf {
        self.metadata_root().join("imported")
    }

    pub fn systems_config(&self) -> PathBuf {
        self.retrobat_root()
            .join("emulationstation")
            .join(".emulationstation")
            .join("es_systems.cfg")
    }

    pub fn bios_catalog(&self) -> PathBuf {
        self.retrobat_root()
            .join("system")
            .join("modules")
            .join("rb_gui")
            .join("bios_local.json")
    }
}
