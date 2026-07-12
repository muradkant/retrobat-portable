//! Optional integration with a desktop controller-to-mouse inhibitor.
//!
//! Linux users may keep a controller mapped to a virtual mouse for desktop
//! accessibility. The established `controller-mouse-game-guard` protocol
//! suspends that mapper while a game owns the physical controller.

#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::Command;
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

#[cfg(target_os = "linux")]
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Error)]
pub enum ControllerGuardError {
    #[error("could not run controller mouse guard {program}: {source}")]
    Launch {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("controller mouse guard {program} {action} failed with {status}")]
    Exit {
        program: String,
        action: &'static str,
        status: std::process::ExitStatus,
    },
}

/// An acquired desktop-mapper inhibitor. Dropping it restores the mapper.
pub struct ControllerMouseGuard {
    #[cfg(target_os = "linux")]
    program: PathBuf,
    #[cfg(target_os = "linux")]
    token: String,
    #[cfg(target_os = "linux")]
    armed: bool,
}

impl ControllerMouseGuard {
    /// Acquire the locally installed guard when present. This is intentionally
    /// optional so a portable bundle remains usable on ordinary Linux and on
    /// Windows without machine-specific setup.
    pub fn acquire_if_available() -> Result<Option<Self>, ControllerGuardError> {
        #[cfg(target_os = "linux")]
        {
            let explicit = std::env::var_os("CONTROLLER_MOUSE_GAME_GUARD");
            let program = explicit.map(PathBuf::from).or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".local/bin/controller-mouse-game-guard"))
                    .filter(|path| path.is_file())
            });
            Self::acquire_from(program.as_deref())
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(None)
        }
    }

    #[cfg(target_os = "linux")]
    fn acquire_from(program: Option<&Path>) -> Result<Option<Self>, ControllerGuardError> {
        let Some(program) = program else {
            return Ok(None);
        };
        let token = format!(
            "{}-retroport-{}",
            std::process::id(),
            NEXT_TOKEN.fetch_add(1, Ordering::Relaxed)
        );
        run_guard(program, "--inhibit", &token)?;
        Ok(Some(Self {
            program: program.to_owned(),
            token,
            armed: true,
        }))
    }

    /// Release explicitly after the complete emulator process tree exits.
    pub fn release(mut self) -> Result<(), ControllerGuardError> {
        self.release_inner()
    }

    #[cfg(target_os = "linux")]
    fn release_inner(&mut self) -> Result<(), ControllerGuardError> {
        if !self.armed {
            return Ok(());
        }
        self.armed = false;
        run_guard(&self.program, "--release", &self.token)
    }

    #[cfg(not(target_os = "linux"))]
    fn release_inner(&mut self) -> Result<(), ControllerGuardError> {
        Ok(())
    }
}

impl Drop for ControllerMouseGuard {
    fn drop(&mut self) {
        let _ = self.release_inner();
    }
}

#[cfg(target_os = "linux")]
fn run_guard(
    program: &Path,
    action: &'static str,
    token: &str,
) -> Result<(), ControllerGuardError> {
    let status = Command::new(program)
        .args([action, token])
        .status()
        .map_err(|source| ControllerGuardError::Launch {
            program: program.display().to_string(),
            source,
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(ControllerGuardError::Exit {
            program: program.display().to_string(),
            action,
            status,
        })
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn inhibitor_is_acquired_and_released_around_a_game_lifetime() {
        let root = tempfile::tempdir().unwrap();
        let log = root.path().join("calls");
        let guard = root.path().join("guard");
        fs::write(
            &guard,
            format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n", log.display()),
        )
        .unwrap();
        fs::set_permissions(&guard, fs::Permissions::from_mode(0o755)).unwrap();

        let inhibitor = ControllerMouseGuard::acquire_from(Some(&guard))
            .unwrap()
            .unwrap();
        inhibitor.release().unwrap();

        let calls = fs::read_to_string(log).unwrap();
        let lines = calls.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("--inhibit "));
        assert!(lines[1].starts_with("--release "));
        assert_eq!(
            lines[0].strip_prefix("--inhibit "),
            lines[1].strip_prefix("--release ")
        );
    }

    #[test]
    fn dropping_inhibitor_releases_after_a_launch_failure() {
        let root = tempfile::tempdir().unwrap();
        let log = root.path().join("calls");
        let guard = root.path().join("guard");
        fs::write(
            &guard,
            format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n", log.display()),
        )
        .unwrap();
        fs::set_permissions(&guard, fs::Permissions::from_mode(0o755)).unwrap();

        let inhibitor = ControllerMouseGuard::acquire_from(Some(&guard))
            .unwrap()
            .unwrap();
        drop(inhibitor);

        let calls = fs::read_to_string(log).unwrap();
        assert_eq!(calls.lines().count(), 2);
        assert!(calls.contains("--inhibit "));
        assert!(calls.contains("--release "));
    }

    #[test]
    fn missing_optional_guard_is_a_no_op() {
        assert!(ControllerMouseGuard::acquire_from(None).unwrap().is_none());
    }
}
