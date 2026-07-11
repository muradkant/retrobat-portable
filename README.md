# RetroPort

RetroPort turns a self-contained RetroBat folder into one artwork-first game
library for Windows and Linux. Open one executable, browse the catalogue, and
use the action on a game card:

- **DOWNLOAD** fetches, verifies, prepares, and installs a publisher- or
  project-hosted game.
- **IMPORT GAME** accepts a local ROM, disc descriptor, executable, or complete
  extracted game folder. It copies every required companion file into the
  portable library.
- **PLAY** launches that exact imported or downloaded game through the selected
  installed backend.

Search is global. Browsing is the primary interface: cards reflow into a
full-width grid in windowed and full-screen views, and every record has a
visual. Established catalogue artwork is used when available; the remaining
records receive a deterministic title/system catalogue cover instead of a
blank card.

## What is in the library?

The checked-in snapshot contains:

- **80,734** browseable records across **10** established catalogue sources;
- **4,153** records with a complete direct-download or local-import route;
- **410** evidence-backed iconic/community-praised title records resolving to
  **935** platform editions;
- **80,734 / 80,734** visual cards;
- **80,734 / 80,734** mapped import routes.

The default **FEATURED** collection is not an arbitrary hand-picked list. Its
records come from a critical-consensus dataset (a title must appear in at least
six independent editorial best-of lists) and the World Video Game Hall of
Fame. **ALL SOURCES** exposes the complete snapshot. Catalogue generators and
source pins live under [`tools/`](tools/), while the generated evidence and
browse snapshots live under [`catalog/`](catalog/).

## From opening the drive to playing

### Windows

1. Open the portable folder.
2. Double-click `RetroPort.exe`.
3. Browse **FEATURED**, a source collection, or a system; then click the game.
4. Click **DOWNLOAD** or **IMPORT GAME**. For an extracted PS3, PS4, Wii U, or
   PC game, choose **IMPORT THIS FOLDER** so DLLs and data files stay together.
   Arcade/MAME sets such as `mspacman.zip` must be selected as the original ZIP;
   do not extract their component files.
5. Click **PLAY**.

PLAY immediately changes to **LOADING**, then **RUNNING**, and remains disabled
until the emulator exits. RetroPort therefore cannot start a duplicate copy
while a game is opening or already running.

### Linux

1. Install 64-bit Wine from the distribution for the legacy RetroBat layer.
2. Double-click `RetroPort-Linux.desktop`, or run `./RetroPort-Linux`.
3. Follow the same DOWNLOAD/IMPORT → PLAY flow.

On the development machine, `tools/run_latest_linux.sh` is the stable
application-menu entry point. It asks Cargo to incrementally rebuild the
current checkout before every launch, then opens that binary against the
discovered complete data bundle. It therefore cannot silently run an older
executable copied to a USB deployment.

RPCS3, Cemu, shadPS4, Eden, and Xenia use bundled native Linux runtimes. The
remaining Windows RetroBat stack runs through Wine. All emulator configuration,
artwork, imported games, saves, native XDG state, and verification metadata stay
under the portable root; only Wine's symlink-heavy prefix is kept in the Linux
user data directory.

Connect a controller before PLAY. RetroPort passes launch control to RetroBat
and its established controller profiles; the supplied Xbox-compatible
controller path was probed through Linux xpad, Wine XInput,
RetroBat/EmulationStation, and RetroArch.

For arcade/MAME games, press **Back/View** on an Xbox-compatible controller to
insert a coin, **Start** to begin, and use the D-pad or left stick to move. On a
keyboard, press **5** to insert a coin, **1** to start, the arrow keys to move,
and **Esc** to quit. RetroPort supplies these mappings explicitly and enables
audio for direct RetroArch launches instead of depending on mutable inherited
frontend settings.

## Firmware flow

Firmware-free HLE routes are preferred when an established one is installed:
Play! for PS2, YabaSanshiro for Saturn, and Cxbx-Reloaded for original Xbox.
Higher-accuracy firmware-dependent routes remain available.

When a selected route needs machine data, the game card exposes one of two
actions:

- **INSTALL FIRMWARE** downloads an exact publisher-hosted file, verifies its
  size and SHA-256, and starts the emulator's installer. Sony's current PS3
  system-software package uses this path.
- **IMPORT FIRMWARE** accepts a nonempty file supplied by the user. Its hash is
  recorded for troubleshooting, but a database hash mismatch does not block
  import or play. Switch `prod.keys` are mirrored automatically into both the
  Windows and Linux portable Eden profiles.

Optional firmware uses the same flow. The UI does not show a firmware warning
when the installed core has a documented built-in fallback.

## Portable bundle layout

```text
RetroPort/
├── RetroPort.exe                 # Windows GUI
├── RetroPort-Linux               # Linux GUI
├── RetroPort-Linux.desktop
├── RetroBat/                     # frontend, adapters, emulators, ROMs, saves
├── Runtime/Linux/                # native modern-console AppImages
├── Artwork/                      # verified local MAME artwork
├── .retrobat-portable/           # imports, installs, cache, native state
├── Source/RetroPort-source.zip   # exact corresponding source snapshot
├── SHA256SUMS
├── VERIFY-LINUX.sh
├── VERIFY-WINDOWS.cmd
└── README-FIRST.txt
```

The removable-drive bundle is intentionally one directory. Do not move either
launcher away from the rest of the tree.

RetroPort also discovers a complete bundle mounted elsewhere on the machine.
This makes a desktop/application-menu shortcut safe: the shortcut may invoke
the installed launcher entry while the actual games, emulators, imports, and
current deployed binary remain together on the removable drive.

## Verify a copy

From the bundle root:

```sh
./VERIFY-LINUX.sh
```

On Windows, double-click `VERIFY-WINDOWS.cmd`. Both verifiers check the two
launchers, source snapshot, documentation, native Linux runtimes, supplementary
Windows backends, and all bundled artwork against `SHA256SUMS`. The GUI also
validates embedded catalogue schemas, counts, identifiers, and coverage during
self-check.

For a source checkout:

```sh
cargo fmt --check
cargo test --all-targets
cargo test --test live_source -- --ignored
cargo clippy --all-targets -- -D warnings
cargo run -- --self-check --bundle-root /path/to/portable/root
```

The ignored-by-default live test performs network I/O and verifies the pinned
upstream metadata and downloadable artifact.

## Build

Rust 1.92 or newer is required.

```sh
cargo build --release --target x86_64-unknown-linux-gnu
cargo xwin build --release --target x86_64-pc-windows-msvc  # from Linux
```

The second command uses
[`cargo-xwin`](https://github.com/rust-cross/cargo-xwin) and Microsoft's public
CRT/SDK packages to produce the Windows target from Linux. On a configured
Windows MSVC host, ordinary `cargo build --release` is sufficient.

To update an already assembled RetroBat portable root with both binaries,
documentation, artwork, a fresh source snapshot, and a regenerated integrity
manifest:

```sh
./tools/deploy_bundle.sh /path/to/portable/root
```

The Windows target is linked with the static C runtime and uses the Windows GUI
subsystem. The resulting executable does not open a console window and does not
require a separately installed Visual C++ runtime.

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for trust boundaries and
[`packaging/THIRD-PARTY-ASSETS.txt`](packaging/THIRD-PARTY-ASSETS.txt) for exact
upstream releases, URLs, hashes, and retained licences.

## Upstream projects and catalogue sources

The project does not hide the stack it is built on. The three RetroBat trees
inspected and integrated during development are referenced at their exact audit
commits:

| Integrated/forked tree | Audited revision | Role |
|---|---:|---|
| [RetroBat](https://github.com/RetroBat-Official/retrobat) | `c90884f56f278dc943e898d8f47376e9ea27fb52` | Portable frontend, configuration, templates, and emulator matrix |
| [RetroBat EmulatorLauncher](https://github.com/RetroBat-Official/emulatorlauncher) | `1a9571af3411333cefd196b6c2ce3dc460bf8d88` | Per-system command generation, controller/config integration |
| [RetroBat EmulationStation](https://github.com/RetroBat-Official/emulationstation) | `d77fbf1fb198a10bb44221e40e463e2e2c30f1a7` | Established library frontend retained in the bundle |

The cross-platform release build also uses
[`cargo-xwin`](https://github.com/rust-cross/cargo-xwin); it is a build tool,
not a runtime dependency.

Supplementary execution projects installed by this bundle are
[RetroArch](https://github.com/libretro/RetroArch),
[JAXE](https://github.com/kurtjd/jaxe),
[Xenia Canary](https://github.com/xenia-canary/xenia-canary),
[RPCS3](https://github.com/RPCS3/rpcs3),
[Cemu](https://github.com/cemu-project/Cemu),
[shadPS4](https://github.com/shadps4-emu/shadPS4),
[Eden](https://git.eden-emu.dev/eden-emu/eden),
[Cxbx-Reloaded](https://github.com/Cxbx-Reloaded/Cxbx-Reloaded), and
[Play!](https://github.com/jpd002/Play-). RetroBat's own bundled emulator/core
notices remain inside `RetroBat/`; this repository does not relabel their work
as RetroPort code.

The catalogue/index layer references [Libretro Database](https://github.com/libretro/libretro-database),
[Libretro Thumbnails](https://thumbnails.libretro.com/),
[LaunchBox Games Database](https://gamesdb.launchbox-app.com/),
[Homebrew Hub](https://hh.gbdev.io/),
[RetroBat free content](https://wiki.retrobat.org/navigation/main-menu),
[ScummVM freeware games](https://www.scummvm.org/games/),
[FreeDOS games](https://www.ibiblio.org/pub/micro/pc-stuff/freedos/files/repositories/1.4/),
[MAME-authorized ROMs](https://www.mamedev.org/roms/),
[MSXdev](https://www.msxdev.org/msxdev-archive/),
[Libretro Content Downloader](https://buildbot.libretro.com/assets/cores/),
[DOS Games Archive](https://www.dosgamesarchive.com/), and
[Progetto-SNAPS](https://www.progettosnaps.net/snapshots/). Featured evidence
comes from the [critical-consensus list](https://en.wikipedia.org/wiki/List_of_video_games_listed_among_the_best)
and [The Strong's World Video Game Hall of Fame](https://www.museumofplay.org/exhibits/world-video-game-hall-of-fame/inducted-games/).

Exact downloaded artifact URLs, versions, archive hashes, extracted hashes,
and licence notes are deliberately kept in
[`packaging/THIRD-PARTY-ASSETS.txt`](packaging/THIRD-PARTY-ASSETS.txt), rather
than only in a transient build log.
