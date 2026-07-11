use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use serde::Serialize;
use thiserror::Error;

use crate::paths::PortableLayout;
use crate::readiness::BackendRoute;

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
    pub generated_files: Vec<(PathBuf, String)>,
}

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("this host platform is not supported")]
    Unsupported,
    #[error("cannot determine a local data directory for the Wine prefix")]
    NoDataDirectory,
    #[error("game system name is invalid: {0}")]
    InvalidSystem(String),
    #[error("Wine cannot address a relative game path: {0}")]
    RelativeWinePath(PathBuf),
    #[error("Wine cannot address a non-Unicode game path: {0}")]
    NonUnicodeWinePath(PathBuf),
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
                generated_files: Vec::new(),
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
                    generated_files: Vec::new(),
                })
            }
            HostPlatform::Unsupported => Err(LaunchError::Unsupported),
        }
    }

    pub fn for_current_host(layout: &PortableLayout) -> Result<Self, LaunchError> {
        let data = dirs::data_local_dir();
        Self::for_host(layout, HostPlatform::current(), data.as_deref())
    }

    pub fn for_game_host(
        layout: &PortableLayout,
        host: HostPlatform,
        linux_data_dir: Option<&Path>,
        system: &str,
        rom: &Path,
    ) -> Result<Self, LaunchError> {
        Self::for_game_host_with_backend(layout, host, linux_data_dir, system, rom, None)
    }

    pub fn for_game_host_with_backend(
        layout: &PortableLayout,
        host: HostPlatform,
        linux_data_dir: Option<&Path>,
        system: &str,
        rom: &Path,
        backend: Option<&BackendRoute>,
    ) -> Result<Self, LaunchError> {
        if system.is_empty()
            || !system.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
            })
        {
            return Err(LaunchError::InvalidSystem(system.to_owned()));
        }
        if system == "chip8" {
            return Self::for_retroarch_core(layout, host, linux_data_dir, system, "jaxe", rom);
        }
        if host == HostPlatform::Linux
            && let Some(BackendRoute {
                emulator,
                core: Some(core),
                ..
            }) = backend
            && emulator.eq_ignore_ascii_case("libretro")
        {
            // EmulatorLauncher is a .NET Framework/WinForms program. Several
            // current Wine-Mono combinations abort before spawning RetroArch
            // (gmisc-win32 filename assertion). Libretro has a complete,
            // stable direct command line, so Linux card launches bypass that
            // unnecessary process without bypassing the selected core.
            return Self::for_retroarch_core(layout, host, linux_data_dir, system, core, rom);
        }
        if host == HostPlatform::Linux
            && backend.is_some_and(|route| route.emulator.eq_ignore_ascii_case("windows"))
            && rom
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        {
            let data_dir = linux_data_dir.ok_or(LaunchError::NoDataDirectory)?;
            return Ok(Self {
                program: PathBuf::from("wine"),
                args: vec![rom.to_owned()],
                current_dir: rom.parent().unwrap_or_else(|| Path::new(".")).to_owned(),
                env: BTreeMap::from([(
                    "WINEPREFIX".to_owned(),
                    data_dir.join("retrobat-portable").join("wine-prefix"),
                )]),
                generated_files: Vec::new(),
            });
        }
        if host == HostPlatform::Linux
            && let Some(backend) = backend
            && let Some(plan) = native_linux_game_plan(layout, backend, rom)
        {
            return Ok(plan);
        }

        let launcher = layout.emulator_launcher_executable();
        let mut args = vec![PathBuf::from("-system"), PathBuf::from(system)];
        if let Some(backend) = backend {
            args.extend([PathBuf::from("-emulator"), PathBuf::from(&backend.emulator)]);
            if let Some(core) = &backend.core {
                args.extend([PathBuf::from("-core"), PathBuf::from(core)]);
            }
        }
        args.push(PathBuf::from("-rom"));
        let (program, current_dir, env) = match host {
            HostPlatform::Windows => {
                args.push(rom.to_owned());
                (launcher, layout.emulationstation_root(), BTreeMap::new())
            }
            HostPlatform::Linux => {
                let data_dir = linux_data_dir.ok_or(LaunchError::NoDataDirectory)?;
                args.insert(0, launcher);
                args.push(wine_path(rom)?);
                (
                    PathBuf::from("wine"),
                    layout.emulationstation_root(),
                    BTreeMap::from([(
                        "WINEPREFIX".to_owned(),
                        data_dir.join("retrobat-portable").join("wine-prefix"),
                    )]),
                )
            }
            HostPlatform::Unsupported => return Err(LaunchError::Unsupported),
        };
        Ok(Self {
            program,
            args,
            current_dir,
            env,
            generated_files: Vec::new(),
        })
    }

    fn for_retroarch_core(
        layout: &PortableLayout,
        host: HostPlatform,
        linux_data_dir: Option<&Path>,
        system: &str,
        core: &str,
        rom: &Path,
    ) -> Result<Self, LaunchError> {
        let retroarch = layout.retroarch_executable();
        let core = layout.retroarch_core(core);
        let save = layout.retrobat_root().join("saves").join(system);
        let state = layout
            .retrobat_root()
            .join("saves")
            .join(system)
            .join("states");
        let append_config = layout
            .metadata_root()
            .join("runtime")
            .join("retroarch")
            .join(format!("{system}.cfg"));
        let mut args = Vec::new();
        let (program, env, config_argument, save_value, state_value) = match host {
            HostPlatform::Windows => {
                let save_value = retroarch_config_path(&save);
                let state_value = retroarch_config_path(&state);
                args.extend([
                    PathBuf::from("--appendconfig"),
                    append_config.clone(),
                    PathBuf::from("-L"),
                    core,
                    rom.to_owned(),
                ]);
                (
                    retroarch,
                    BTreeMap::new(),
                    append_config.clone(),
                    save_value,
                    state_value,
                )
            }
            HostPlatform::Linux => {
                let data_dir = linux_data_dir.ok_or(LaunchError::NoDataDirectory)?;
                let config_argument = wine_path(&append_config)?;
                let save_value = retroarch_config_path(&wine_path(&save)?);
                let state_value = retroarch_config_path(&wine_path(&state)?);
                args.extend([
                    retroarch,
                    PathBuf::from("--appendconfig"),
                    config_argument.clone(),
                    PathBuf::from("-L"),
                    wine_path(&core)?,
                    wine_path(rom)?,
                ]);
                (
                    PathBuf::from("wine"),
                    BTreeMap::from([(
                        "WINEPREFIX".to_owned(),
                        data_dir.join("retrobat-portable").join("wine-prefix"),
                    )]),
                    config_argument,
                    save_value,
                    state_value,
                )
            }
            HostPlatform::Unsupported => return Err(LaunchError::Unsupported),
        };
        debug_assert!(args.iter().any(|argument| argument == &config_argument));
        let mut config = format!(
            concat!(
                "savefile_directory = \"{}\"\n",
                "savestate_directory = \"{}\"\n",
                "config_save_on_exit = \"false\"\n",
                "audio_enable = \"true\"\n",
                "audio_driver = \"xaudio\"\n",
                "audio_mute_enable = \"false\"\n",
                "audio_mixer_mute_enable = \"false\"\n",
                "audio_volume = \"0.000000\"\n",
                "input_autodetect_enable = \"true\"\n",
                "input_joypad_driver = \"sdl2\"\n",
                "input_player1_joypad_index = \"0\"\n",
                "input_player1_analog_dpad_mode = \"1\"\n",
                "input_player1_b_btn = \"0\"\n",
                "input_player1_a_btn = \"1\"\n",
                "input_player1_y_btn = \"2\"\n",
                "input_player1_x_btn = \"3\"\n",
                "input_player1_select_btn = \"4\"\n",
                "input_player1_start_btn = \"6\"\n",
                "input_player1_up_btn = \"11\"\n",
                "input_player1_down_btn = \"12\"\n",
                "input_player1_left_btn = \"13\"\n",
                "input_player1_right_btn = \"14\"\n"
            ),
            save_value.replace('"', "\\\""),
            state_value.replace('"', "\\\"")
        );
        if system == "mame" {
            config.push_str(concat!(
                "input_player1_select = \"num5\"\n",
                "input_player1_start = \"num1\"\n",
                "input_player1_up = \"up\"\n",
                "input_player1_down = \"down\"\n",
                "input_player1_left = \"left\"\n",
                "input_player1_right = \"right\"\n"
            ));
        }
        Ok(Self {
            program,
            args,
            current_dir: layout.retroarch_root(),
            env,
            generated_files: vec![(append_config, config)],
        })
    }

    pub fn for_current_game(
        layout: &PortableLayout,
        system: &str,
        rom: &Path,
    ) -> Result<Self, LaunchError> {
        let data = dirs::data_local_dir();
        Self::for_game_host(
            layout,
            HostPlatform::current(),
            data.as_deref(),
            system,
            rom,
        )
    }

    pub fn for_current_game_with_backend(
        layout: &PortableLayout,
        system: &str,
        rom: &Path,
        backend: Option<&BackendRoute>,
    ) -> Result<Self, LaunchError> {
        let data = dirs::data_local_dir();
        Self::for_game_host_with_backend(
            layout,
            HostPlatform::current(),
            data.as_deref(),
            system,
            rom,
            backend,
        )
    }

    pub fn for_rpcs3_firmware_install_host(
        layout: &PortableLayout,
        host: HostPlatform,
        linux_data_dir: Option<&Path>,
        firmware: &Path,
    ) -> Result<Self, LaunchError> {
        let rpcs3 = layout.rpcs3_executable();
        let current_dir = layout.emulator_root("rpcs3");
        let native = layout.linux_runtime_root().join("RPCS3.AppImage");
        let (program, args, env) = match host {
            HostPlatform::Windows => (
                rpcs3,
                vec![PathBuf::from("--installfw"), firmware.to_owned()],
                BTreeMap::new(),
            ),
            HostPlatform::Linux => {
                if native.is_file() {
                    return Ok(Self {
                        program: native,
                        args: vec![PathBuf::from("--installfw"), firmware.to_owned()],
                        current_dir: layout.linux_runtime_root(),
                        env: native_linux_environment(layout, "rpcs3"),
                        generated_files: Vec::new(),
                    });
                }
                let data_dir = linux_data_dir.ok_or(LaunchError::NoDataDirectory)?;
                (
                    PathBuf::from("wine"),
                    vec![rpcs3, PathBuf::from("--installfw"), wine_path(firmware)?],
                    BTreeMap::from([(
                        "WINEPREFIX".to_owned(),
                        data_dir.join("retrobat-portable").join("wine-prefix"),
                    )]),
                )
            }
            HostPlatform::Unsupported => return Err(LaunchError::Unsupported),
        };
        Ok(Self {
            program,
            args,
            current_dir,
            env,
            generated_files: Vec::new(),
        })
    }

    pub fn for_current_rpcs3_firmware_install(
        layout: &PortableLayout,
        firmware: &Path,
    ) -> Result<Self, LaunchError> {
        let data = dirs::data_local_dir();
        Self::for_rpcs3_firmware_install_host(
            layout,
            HostPlatform::current(),
            data.as_deref(),
            firmware,
        )
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
        for key in ["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"] {
            if let Some(directory) = self.env.get(key) {
                fs::create_dir_all(directory)?;
            }
        }
        for (path, contents) in &self.generated_files {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, contents)?;
        }
        Ok(())
    }
}

fn retroarch_config_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn native_linux_game_plan(
    layout: &PortableLayout,
    backend: &BackendRoute,
    rom: &Path,
) -> Option<LaunchPlan> {
    let runtime = layout.linux_runtime_root();
    let emulator = backend.emulator.to_ascii_lowercase();
    let (program, args, key) = match emulator.as_str() {
        "eden" => (
            runtime.join("Eden.AppImage"),
            vec![PathBuf::from("-f"), PathBuf::from("-g"), rom.to_owned()],
            "eden",
        ),
        "cemu" => (
            runtime.join("Cemu.AppImage"),
            vec![PathBuf::from("-g"), rom.to_owned(), PathBuf::from("-f")],
            "cemu",
        ),
        "rpcs3" => (
            runtime.join("RPCS3.AppImage"),
            vec![PathBuf::from("--no-gui"), rom.to_owned()],
            "rpcs3",
        ),
        "shadps4" => (
            runtime.join("shadPS4/Shadps4-sdl.AppImage"),
            vec![rom.to_owned()],
            "shadps4",
        ),
        "xenia-canary" => (
            runtime.join("XeniaCanary.AppImage"),
            vec![rom.to_owned()],
            "xenia-canary",
        ),
        _ => return None,
    };
    program.is_file().then(|| LaunchPlan {
        program,
        args,
        current_dir: runtime,
        env: native_linux_environment(layout, key),
        generated_files: Vec::new(),
    })
}

fn native_linux_environment(layout: &PortableLayout, emulator: &str) -> BTreeMap<String, PathBuf> {
    let root = layout
        .metadata_root()
        .join("runtime")
        .join("linux")
        .join(emulator);
    BTreeMap::from([
        ("XDG_CONFIG_HOME".to_owned(), root.join("config")),
        ("XDG_DATA_HOME".to_owned(), root.join("data")),
        ("XDG_CACHE_HOME".to_owned(), root.join("cache")),
    ])
}

fn wine_path(path: &Path) -> Result<PathBuf, LaunchError> {
    if !path.is_absolute() {
        return Err(LaunchError::RelativeWinePath(path.to_owned()));
    }
    let Some(path_text) = path.to_str() else {
        return Err(LaunchError::NonUnicodeWinePath(path.to_owned()));
    };
    Ok(PathBuf::from(format!("Z:{}", path_text.replace('/', "\\"))))
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
    fn windows_rpcs3_firmware_install_uses_the_bundled_emulator() {
        let layout = PortableLayout::new("X:/Arcade");
        let firmware = Path::new("X:/Arcade/RetroBat/bios/PS3UPDAT.PUP");
        let plan = LaunchPlan::for_rpcs3_firmware_install_host(
            &layout,
            HostPlatform::Windows,
            None,
            firmware,
        )
        .unwrap();
        assert_eq!(
            plan.program,
            PathBuf::from("X:/Arcade/RetroBat/emulators/rpcs3/rpcs3.exe")
        );
        assert_eq!(
            plan.args,
            vec![PathBuf::from("--installfw"), firmware.to_owned()]
        );
    }

    #[test]
    fn linux_rpcs3_firmware_install_converts_only_the_firmware_path() {
        let layout = PortableLayout::new("/run/media/user/Arcade");
        let firmware = Path::new("/run/media/user/Arcade/RetroBat/bios/PS3UPDAT.PUP");
        let plan = LaunchPlan::for_rpcs3_firmware_install_host(
            &layout,
            HostPlatform::Linux,
            Some(Path::new("/home/user/.local/share")),
            firmware,
        )
        .unwrap();
        assert_eq!(plan.program, PathBuf::from("wine"));
        assert_eq!(
            plan.args,
            vec![
                PathBuf::from("/run/media/user/Arcade/RetroBat/emulators/rpcs3/rpcs3.exe"),
                PathBuf::from("--installfw"),
                PathBuf::from("Z:\\run\\media\\user\\Arcade\\RetroBat\\bios\\PS3UPDAT.PUP")
            ]
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

    #[test]
    fn windows_game_launch_uses_retrobat_emulator_launcher() {
        let layout = PortableLayout::new("X:/Arcade");
        let rom = Path::new("X:/Arcade/RetroBat/roms/gb/2048.gb");
        let plan =
            LaunchPlan::for_game_host(&layout, HostPlatform::Windows, None, "gb", rom).unwrap();
        assert_eq!(
            plan.program,
            PathBuf::from("X:/Arcade/RetroBat/emulationstation/emulatorLauncher.exe")
        );
        assert_eq!(
            plan.args,
            vec![
                PathBuf::from("-system"),
                PathBuf::from("gb"),
                PathBuf::from("-rom"),
                rom.to_owned(),
            ]
        );
    }

    #[test]
    fn launch_can_pin_an_installed_alternative_backend() {
        let layout = PortableLayout::new("X:/Arcade");
        let rom = Path::new("X:/Arcade/RetroBat/roms/gamecube/game.rvz");
        let backend = BackendRoute {
            emulator: "libretro".to_owned(),
            core: Some("dolphin".to_owned()),
            incompatible_extensions: vec![".zip".to_owned()],
        };
        let plan = LaunchPlan::for_game_host_with_backend(
            &layout,
            HostPlatform::Windows,
            None,
            "gamecube",
            rom,
            Some(&backend),
        )
        .unwrap();
        assert_eq!(
            plan.args,
            vec![
                PathBuf::from("-system"),
                PathBuf::from("gamecube"),
                PathBuf::from("-emulator"),
                PathBuf::from("libretro"),
                PathBuf::from("-core"),
                PathBuf::from("dolphin"),
                PathBuf::from("-rom"),
                rom.to_owned(),
            ]
        );
    }

    #[test]
    fn linux_modern_console_route_uses_the_bundled_native_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let layout = PortableLayout::new(temp.path().join("Arcade"));
        fs::create_dir_all(layout.linux_runtime_root()).unwrap();
        fs::write(layout.linux_runtime_root().join("Cemu.AppImage"), b"app").unwrap();
        let rom = layout.retrobat_root().join("roms/wiiu/game.rpx");
        let backend = BackendRoute {
            emulator: "cemu".to_owned(),
            core: None,
            incompatible_extensions: Vec::new(),
        };
        let plan = LaunchPlan::for_game_host_with_backend(
            &layout,
            HostPlatform::Linux,
            None,
            "wiiu",
            &rom,
            Some(&backend),
        )
        .unwrap();
        assert_eq!(
            plan.program,
            layout.linux_runtime_root().join("Cemu.AppImage")
        );
        assert_eq!(
            plan.args,
            vec![PathBuf::from("-g"), rom, PathBuf::from("-f")]
        );
        assert!(plan.env["XDG_CONFIG_HOME"].starts_with(layout.metadata_root()));
    }

    #[test]
    fn linux_game_launch_converts_the_rom_to_wines_z_drive() {
        let layout = PortableLayout::new("/run/media/user/Arcade");
        let rom = Path::new("/run/media/user/Arcade/RetroBat/roms/gb/2048.gb");
        let plan = LaunchPlan::for_game_host(
            &layout,
            HostPlatform::Linux,
            Some(Path::new("/home/user/.local/share")),
            "gb",
            rom,
        )
        .unwrap();
        assert_eq!(plan.program, PathBuf::from("wine"));
        assert_eq!(
            plan.args,
            vec![
                PathBuf::from(
                    "/run/media/user/Arcade/RetroBat/emulationstation/emulatorLauncher.exe"
                ),
                PathBuf::from("-system"),
                PathBuf::from("gb"),
                PathBuf::from("-rom"),
                PathBuf::from(r"Z:\run\media\user\Arcade\RetroBat\roms\gb\2048.gb"),
            ]
        );
    }

    #[test]
    fn linux_libretro_card_play_bypasses_the_fragile_dotnet_launcher() {
        let layout = PortableLayout::new("/run/media/user/Arcade");
        let rom = Path::new("/run/media/user/Arcade/RetroBat/roms/mame/mspacman.zip");
        let backend = BackendRoute {
            emulator: "libretro".to_owned(),
            core: Some("mame".to_owned()),
            incompatible_extensions: Vec::new(),
        };
        let plan = LaunchPlan::for_game_host_with_backend(
            &layout,
            HostPlatform::Linux,
            Some(Path::new("/home/user/.local/share")),
            "mame",
            rom,
            Some(&backend),
        )
        .unwrap();

        assert_eq!(plan.program, PathBuf::from("wine"));
        assert_eq!(
            plan.args,
            vec![
                layout.retroarch_executable(),
                PathBuf::from("--appendconfig"),
                PathBuf::from(
                    r"Z:\run\media\user\Arcade\.retrobat-portable\runtime\retroarch\mame.cfg"
                ),
                PathBuf::from("-L"),
                PathBuf::from(
                    r"Z:\run\media\user\Arcade\RetroBat\emulators\retroarch\cores\mame_libretro.dll"
                ),
                PathBuf::from(r"Z:\run\media\user\Arcade\RetroBat\roms\mame\mspacman.zip"),
            ]
        );
        let config = &plan.generated_files[0].1;
        for expected in [
            "savefile_directory = \"Z:/run/media/user/Arcade/RetroBat/saves/mame\"",
            "savestate_directory = \"Z:/run/media/user/Arcade/RetroBat/saves/mame/states\"",
            "audio_enable = \"true\"",
            "audio_driver = \"xaudio\"",
            "audio_mute_enable = \"false\"",
            "input_joypad_driver = \"sdl2\"",
            "input_player1_select_btn = \"4\"",
            "input_player1_start_btn = \"6\"",
            "input_player1_select = \"num5\"",
            "input_player1_start = \"num1\"",
        ] {
            assert!(config.contains(expected), "missing {expected}");
        }
    }

    #[test]
    fn direct_libretro_launch_materializes_its_audio_and_input_overrides() {
        let temp = tempfile::tempdir().unwrap();
        let layout = PortableLayout::new(temp.path().join("Arcade"));
        let rom = layout.retrobat_root().join("roms/mame/mspacman.zip");
        let backend = BackendRoute {
            emulator: "libretro".to_owned(),
            core: Some("mame".to_owned()),
            incompatible_extensions: Vec::new(),
        };
        let plan = LaunchPlan::for_game_host_with_backend(
            &layout,
            HostPlatform::Linux,
            Some(temp.path()),
            "mame",
            &rom,
            Some(&backend),
        )
        .unwrap();

        plan.prepare_runtime().unwrap();

        let config_path = layout.metadata_root().join("runtime/retroarch/mame.cfg");
        let config = fs::read_to_string(config_path).unwrap();
        assert!(config.contains("audio_enable = \"true\""));
        assert!(config.contains("input_player1_select_btn = \"4\""));
        assert!(config.contains("input_player1_start = \"num1\""));
    }

    #[test]
    fn linux_windows_game_import_launches_the_exe_with_its_companion_directory() {
        let layout = PortableLayout::new("/run/media/user/Arcade");
        let game = Path::new("/run/media/user/Arcade/RetroBat/roms/windows/Game/bin/Game.exe");
        let backend = BackendRoute {
            emulator: "windows".to_owned(),
            core: None,
            incompatible_extensions: Vec::new(),
        };
        let plan = LaunchPlan::for_game_host_with_backend(
            &layout,
            HostPlatform::Linux,
            Some(Path::new("/home/user/.local/share")),
            "windows",
            game,
            Some(&backend),
        )
        .unwrap();

        assert_eq!(plan.program, PathBuf::from("wine"));
        assert_eq!(plan.args, vec![game.to_owned()]);
        assert_eq!(plan.current_dir, game.parent().unwrap());
    }

    #[test]
    fn game_launch_rejects_a_system_that_could_be_parsed_as_arguments() {
        let layout = PortableLayout::new("/run/media/user/Arcade");
        let result = LaunchPlan::for_game_host(
            &layout,
            HostPlatform::Linux,
            Some(Path::new("/home/user/.local/share")),
            "../gb",
            Path::new("/tmp/game.gb"),
        );
        assert!(matches!(result, Err(LaunchError::InvalidSystem(_))));
    }

    #[test]
    fn chip8_launch_uses_the_jaxe_core_directly() {
        let layout = PortableLayout::new("/run/media/user/Arcade");
        let rom = Path::new("/run/media/user/Arcade/RetroBat/roms/chip8/game.ch8");
        let plan = LaunchPlan::for_game_host(
            &layout,
            HostPlatform::Linux,
            Some(Path::new("/home/user/.local/share")),
            "chip8",
            rom,
        )
        .unwrap();
        assert_eq!(plan.program, PathBuf::from("wine"));
        assert_eq!(
            plan.args,
            vec![
                PathBuf::from("/run/media/user/Arcade/RetroBat/emulators/retroarch/retroarch.exe"),
                PathBuf::from("--appendconfig"),
                PathBuf::from(
                    r"Z:\run\media\user\Arcade\.retrobat-portable\runtime\retroarch\chip8.cfg"
                ),
                PathBuf::from("-L"),
                PathBuf::from(
                    r"Z:\run\media\user\Arcade\RetroBat\emulators\retroarch\cores\jaxe_libretro.dll"
                ),
                PathBuf::from(r"Z:\run\media\user\Arcade\RetroBat\roms\chip8\game.ch8"),
            ]
        );
    }
}
