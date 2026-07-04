use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use serde::Serialize;
use thiserror::Error;

use crate::paths::PortableLayout;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum HostPlatform {
    Windows,
    Linux,
    Unsupported,
}

impl HostPlatform {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Unsupported
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchPlan {
    pub program: PathBuf,
    pub args: Vec<PathBuf>,
    pub current_dir: PathBuf,
    pub env: BTreeMap<String, PathBuf>,
}

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("this host platform is not supported")]
    Unsupported,
    #[error("cannot determine a local data directory for the Wine prefix")]
    NoDataDirectory,
    #[error("failed to launch RetroBat: {0}")]
    Io(#[from] io::Error),
}

impl LaunchPlan {
    pub fn for_host(
        layout: &PortableLayout,
        host: HostPlatform,
        linux_data_dir: Option<&Path>,
    ) -> Result<Self, LaunchError> {
        match host {
            HostPlatform::Windows => Ok(Self {
                program: layout.retrobat_executable(),
                args: Vec::new(),
                current_dir: layout.retrobat_root(),
                env: BTreeMap::new(),
            }),
            HostPlatform::Linux => {
                let data_dir = linux_data_dir.ok_or(LaunchError::NoDataDirectory)?;
                Ok(Self {
                    program: PathBuf::from("wine"),
                    args: vec![layout.retrobat_executable()],
                    current_dir: layout.retrobat_root(),
                    env: BTreeMap::from([(
                        "WINEPREFIX".to_owned(),
                        data_dir.join("retrobat-portable").join("wine-prefix"),
                    )]),
                })
            }
            HostPlatform::Unsupported => Err(LaunchError::Unsupported),
        }
    }

    pub fn for_current_host(layout: &PortableLayout) -> Result<Self, LaunchError> {
        let data = dirs::data_local_dir();
        Self::for_host(layout, HostPlatform::current(), data.as_deref())
    }

    pub fn spawn(&self) -> Result<Child, LaunchError> {
        self.prepare_runtime()?;

        let mut command = Command::new(&self.program);
        command.args(&self.args).current_dir(&self.current_dir);
        for (key, value) in &self.env {
            command.env(key, value);
        }
        Ok(command.spawn()?)
    }

    fn prepare_runtime(&self) -> Result<(), LaunchError> {
        if let Some(prefix) = self.env.get("WINEPREFIX") {
            fs::create_dir_all(prefix)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_launches_the_bundled_executable_directly() {
        let layout = PortableLayout::new("X:/Arcade");
        let plan = LaunchPlan::for_host(&layout, HostPlatform::Windows, None).unwrap();
        assert_eq!(
            plan.program,
            PathBuf::from("X:/Arcade/RetroBat/RetroBat.exe")
        );
        assert!(plan.args.is_empty());
    }

    #[test]
    fn linux_keeps_the_wine_prefix_off_the_removable_drive() {
        let layout = PortableLayout::new("/run/media/user/Arcade");
        let plan = LaunchPlan::for_host(
            &layout,
            HostPlatform::Linux,
            Some(Path::new("/home/user/.local/share")),
        )
        .unwrap();
        assert_eq!(plan.program, PathBuf::from("wine"));
        assert_eq!(
            plan.env["WINEPREFIX"],
            PathBuf::from("/home/user/.local/share/retrobat-portable/wine-prefix")
        );
        assert_eq!(
            plan.args,
            vec![PathBuf::from(
                "/run/media/user/Arcade/RetroBat/RetroBat.exe"
            )]
        );
    }

    #[test]
    fn linux_creates_the_wine_prefix_before_first_launch() {
        let data_dir = tempfile::tempdir().unwrap();
        let layout = PortableLayout::new("/run/media/user/Arcade");
        let plan =
            LaunchPlan::for_host(&layout, HostPlatform::Linux, Some(data_dir.path())).unwrap();
        let prefix = &plan.env["WINEPREFIX"];

        assert!(!prefix.exists());
        plan.prepare_runtime().unwrap();
        assert!(prefix.is_dir());
    }
}
