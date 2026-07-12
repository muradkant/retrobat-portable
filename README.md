# RetroPort

RetroPort is one artwork-first library for emulated games on Windows and Linux.
Browse by cover, source, or system; search the entire catalogue; then act from
the game card:

- **DOWNLOAD** fetches and verifies a game hosted by its publisher or project.
- **IMPORT GAME** copies a local ROM, disc set, executable, or extracted game.
- **PLAY** opens that exact copy through the installed backend.
- **CONTROLS** shows the keyboard, controller, and special hardware mapping—and
  identifies its evidence instead of inventing game-specific directions.

Cards fill the available window in a multi-row grid. Every record has either
established catalogue artwork or a deterministic title-and-system cover.

## From clone to playable installation

Git contains the reviewable source, catalogues, evidence, tools, metadata, and
redistributable artwork. It does **not** contain the roughly 5 GB runtime or the
generated `RetroPort.exe` and `RetroPort-Linux` launchers.

On Debian or Ubuntu Linux, this builds both launchers, downloads every pinned
runtime from its official source, verifies it, assembles the installation, and
tests the result:

```sh
sudo apt-get update
sudo apt-get install -y build-essential curl git p7zip-full python3 rsync wine64
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"
git clone https://github.com/muradkant/retrobat-portable.git
cd retrobat-portable
./tools/bootstrap_bundle.sh
```

The bootstrap is idempotent. It needs about 7 GB beyond Cargo's build cache and
ends by printing the paths of both launchers. A raw clone without those files
has not been assembled. See [Dependencies](DEPENDENCIES.md) for the inclusion
boundary, cache control, pinned artifacts, and independent build stages.

## Play

On Windows, open the assembled folder and double-click `RetroPort.exe`.

On Linux, double-click `RetroPort-Linux.desktop` or run:

```sh
./RetroPort-Linux
```

Then:

1. Browse **FEATURED**, a source, or a system—or search globally.
2. Choose **DOWNLOAD** or **IMPORT GAME**.
3. Choose **PLAY**.

Import extracted PS3, PS4, Wii U, and PC games with **IMPORT THIS FOLDER** so
their executables, libraries, and data stay together. Import an arcade set such
as `mspacman.zip` as the original ZIP, not as its extracted component files.

**PLAY** becomes **LOADING**, then **TERMINATE**. This prevents duplicate
launches; **TERMINATE** stops the emulator's complete process tree. Readiness
audits and process monitoring run off the GUI thread, so importing or closing a
game does not freeze the library.

### Controllers

Connect the controller before **PLAY**. **CONTROLS** reads the installed
RetroArch/RetroBat mapping and the matching SDL device profile. Arcade entries
add MAME-declared players, coins, controls, buttons, and special devices;
RetroBat-tagged entries add exact gun, wheel, spinner, and trackball needs. If a
core reveals action labels only at runtime, RetroPort directs the reader to
RetroArch's Quick Menu → Controls rather than guessing.

On Linux, RetroPort also understands the optional
[`linux-zhixu-controller-fix` game guard](https://github.com/muradkant/linux-zhixu-controller-fix#tested-machine-circumstance).
It suspends that project's controller-as-mouse mapping for the life of the
emulator process tree, then restores desktop navigation. Without the guard,
this integration is a no-op.

### Firmware

RetroPort uses established firmware-free routes where available: Play! for
PS2, YabaSanshiro for Saturn, and Cxbx-Reloaded for original Xbox. A route that
does need machine data exposes one of two actions:

- **INSTALL FIRMWARE** downloads publisher-hosted bytes, verifies their size
  and SHA-256, and opens the emulator installer. PS3 system software uses this
  route.
- **IMPORT FIRMWARE** accepts any nonempty user-selected file. RetroPort records
  its hash for diagnosis but does not reject an unfamiliar dump. Switch
  `prod.keys` are copied into both Eden profiles.

Optional firmware follows the same rules. A documented built-in fallback does
not produce a false firmware warning.

## Catalogue coverage

The checked-in snapshot contains:

- **80,734** records from **10** established catalogue sources;
- **4,153** complete direct-download or local-import routes;
- **410** evidence-backed celebrated titles spanning **935** platform editions;
- **80,734 / 80,734** covers, import routes, and controls views;
- MAME machine associations for all **15,605** MAME records, with current input
  declarations for **14,845** and explicit labels for historical names.

**FEATURED** combines titles found on at least six independent editorial
best-of lists with World Video Game Hall of Fame inductees. **ALL SOURCES**
exposes the full snapshot. Generators and pins live in [`tools/`](tools/);
generated catalogue and evidence snapshots live in [`catalog/`](catalog/).

## Installation boundary

An assembled installation is one independent directory:

```text
RetroPort/
├── RetroPort.exe                 # Windows GUI
├── RetroPort-Linux               # Linux GUI
├── RetroPort-Linux.desktop
├── RetroBat/                     # adapters, emulators, games, saves
├── Runtime/Linux/                # native modern-console AppImages
├── Artwork/                      # verified local MAME artwork
├── .retrobat-portable/           # imports, installs, cache, native state
├── Source/RetroPort-source.zip   # corresponding source
├── SHA256SUMS
├── VERIFY-LINUX.sh
├── VERIFY-WINDOWS.cmd
└── README-FIRST.txt
```

Each launcher resolves only this directory. It never borrows games,
configuration, or runtimes from a sibling installation. Keep the launchers
inside it. On Linux, only Wine's symlink-heavy prefix lives in user data;
emulator configuration, native XDG state, artwork, games, saves, and manifests
remain here.

RPCS3, Cemu, shadPS4, Eden, and Xenia use native Linux runtimes. The legacy
RetroBat stack uses 64-bit Wine.

## Verify or develop

Verify an assembled Linux installation with `./VERIFY-LINUX.sh`; on Windows,
double-click `VERIFY-WINDOWS.cmd`. Both compare launchers, source,
documentation, runtimes, backends, and artwork with `SHA256SUMS`. The GUI
self-check separately validates catalogue schemas, identifiers, counts, and
coverage.

The repository pins Rust 1.92.0. Verify a source checkout with:

```sh
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo run -- --self-check --bundle-root "$PWD"
```

The live upstream test performs network I/O and is therefore explicit:

```sh
cargo test --test live_source -- --ignored
```

Build both release launchers on Linux with:

```sh
cargo build --release --target x86_64-unknown-linux-gnu
cargo xwin build --release --target x86_64-pc-windows-msvc
```

`cargo-xwin` uses Microsoft's public CRT/SDK packages; the resulting GUI binary
links the static C runtime and opens no console. On a Windows MSVC host, use
ordinary `cargo build --release`.

For source-machine menu integration, `tools/run_latest_linux.sh` rebuilds the
current checkout before launch and rejects an incomplete local runtime. To
exercise a real imported game beyond compositor timeout thresholds:

```sh
cargo run --release -- \
  --bundle-root "$PWD" \
  --gameplay-probe CATALOG_ID \
  --gameplay-probe-output /tmp/retroport-gameplay.jsonl \
  --gameplay-probe-seconds 20
```

This records launch, sustained execution, whole-tree termination, and exit.
Maintainers with one complete installation can synchronize it elsewhere with
`./tools/deploy_bundle.sh /destination`; normal clean-clone setup uses
`bootstrap_bundle.sh`.

## Provenance

RetroPort was built against these exact RetroBat revisions:

| Project | Revision | Role |
| --- | --- | --- |
| [RetroBat](https://github.com/RetroBat-Official/retrobat) | `c90884f56f278dc943e898d8f47376e9ea27fb52` | Configuration, templates, updater, emulator matrix |
| [EmulatorLauncher](https://github.com/RetroBat-Official/emulatorlauncher) | `1a9571af3411333cefd196b6c2ce3dc460bf8d88` | Per-system commands and input integration |
| [EmulationStation](https://github.com/RetroBat-Official/emulationstation) | `d77fbf1fb198a10bb44221e40e463e2e2c30f1a7` | Established library frontend retained in the runtime |

Execution comes from [RetroArch](https://github.com/libretro/RetroArch),
[JAXE](https://github.com/kurtjd/jaxe),
[Xenia Canary](https://github.com/xenia-canary/xenia-canary),
[RPCS3](https://github.com/RPCS3/rpcs3),
[Cemu](https://github.com/cemu-project/Cemu),
[shadPS4](https://github.com/shadps4-emu/shadPS4),
[Eden](https://git.eden-emu.dev/eden-emu/eden),
[Cxbx-Reloaded](https://github.com/Cxbx-Reloaded/Cxbx-Reloaded), and
[Play!](https://github.com/jpd002/Play-). Their notices remain with their
runtimes.

Controls evidence comes from [MAME `-listxml`](https://docs.mamedev.org/commandline/commandline-all.html),
the [Libretro MAME DAT](https://github.com/libretro/libretro-database/tree/master/metadat/mame),
RetroBat's pinned [`gamesdb.xml`](https://github.com/RetroBat-Official/emulationstation/blob/d77fbf1fb198a10bb44221e40e463e2e2c30f1a7/resources/gamesdb.xml),
and Libretro's [RetroPad model](https://docs.libretro.com/guides/input-and-controls/).

Catalogue and artwork sources are [Libretro Database](https://github.com/libretro/libretro-database),
[Libretro Thumbnails](https://thumbnails.libretro.com/),
[LaunchBox Games Database](https://gamesdb.launchbox-app.com/),
[Homebrew Hub](https://hh.gbdev.io/), [RetroBat free content](https://wiki.retrobat.org/navigation/main-menu),
[ScummVM freeware](https://www.scummvm.org/games/),
[FreeDOS](https://www.ibiblio.org/pub/micro/pc-stuff/freedos/files/repositories/1.4/),
[MAME-authorized ROMs](https://www.mamedev.org/roms/),
[MSXdev](https://www.msxdev.org/msxdev-archive/),
[Libretro Content Downloader](https://buildbot.libretro.com/assets/cores/),
[DOS Games Archive](https://www.dosgamesarchive.com/), and
[Progetto-SNAPS](https://www.progettosnaps.net/snapshots/). Featured evidence
comes from the [critical-consensus list](https://en.wikipedia.org/wiki/List_of_video_games_listed_among_the_best)
and [World Video Game Hall of Fame](https://www.museumofplay.org/exhibits/world-video-game-hall-of-fame/inducted-games/).

[Architecture](ARCHITECTURE.md) defines runtime and trust invariants.
[Dependencies](DEPENDENCIES.md) explains reproducible assembly.
[`THIRD-PARTY-ASSETS.txt`](packaging/THIRD-PARTY-ASSETS.txt) records exact URLs,
versions, hashes, and licences.
