# Dependency and distribution contract

RetroPort has two distinct artifacts:

1. **The Git repository** is the reviewable Rust control plane, catalogue and
   evidence snapshots, generators, tests, packaging metadata, documentation,
   and redistributable packaged artwork.
2. **An assembled bundle** is a runnable product directory containing the
   compiled RetroPort launchers, RetroBat, emulator binaries and cores, native
   Linux runtimes, configuration, artwork, and mutable user state.

A Git clone is intentionally not presented as an already populated emulator
appliance. Each assembled installation is independent and does not borrow
runtime files from another copy.

## What Git contains

| Component | Git-tracked? | How it is represented |
| --- | --- | --- |
| RetroPort Rust source and lockfile | Yes | `src/`, `Cargo.toml`, `Cargo.lock` |
| Catalogue/evidence snapshots | Yes | `catalog/` |
| MAME catalogue artwork | Yes | `packaging/Artwork/` |
| Build, audit, and deployment tools | Yes | `tools/` |
| Exact upstream revisions and binary evidence | Yes | README tables and `packaging/THIRD-PARTY-ASSETS.txt` |
| RetroBat runtime tree | No | Official 8.1.2 release plus its updater; assembled at `RetroBat/` |
| Supplementary emulator binaries | No | Official URLs and hashes in `packaging/THIRD-PARTY-ASSETS.txt` |
| Native Linux AppImages | No | Official URLs and hashes in `packaging/THIRD-PARTY-ASSETS.txt` |
| Imported games, saves, caches, and private firmware | Never | Mutable/private bundle state under `RetroBat/` and `.retrobat-portable/` |
| Built Linux/Windows launchers | No | Produced from the tagged Rust source |

The ignored paths are explicit in `.gitignore`. `tools/deploy_bundle.sh` copies
an already complete canonical bundle to another directory; it is not a hidden
network bootstrapper.

## Why the runtime is not committed

- The current assembled dependency tree is approximately 5 GB before user
  libraries grow. GitHub blocks ordinary Git objects larger than 100 MiB and
  recommends repositories remain below 1 GB when possible.
- Git is a poor update mechanism for generated binaries and thousands of files
  owned by independent upstream projects. Immutable upstream release URLs and
  hashes preserve clearer provenance.
- Some files may be installed on a private machine but may not be redistributed
  by RetroPort. Imported commercial games, saves, account state, and Sony system
  software must never enter the public repository.
- Several catalogues require direct publisher/project download rather than
  third-party rebundling.
- Source-code submodules would not solve the runtime problem: RetroPort consumes
  official emulator binaries and does not rebuild every emulator toolchain.
  Audited source revisions are pinned in the main README instead.

Large public binaries belong in versioned release assets or an artifact store,
split where necessary and only after every included licence permits
redistribution—not in ordinary Git history.

## One-command reproducible setup

On a Debian/Ubuntu-family Linux host, install the small set of build tools and
run the bootstrap from a fresh clone:

```sh
sudo apt-get update
sudo apt-get install -y build-essential curl git p7zip-full rsync wine64
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"
git clone https://github.com/muradkant/retrobat-portable.git
cd retrobat-portable
./tools/bootstrap_bundle.sh
./RetroPort-Linux
```

`bootstrap_bundle.sh` downloads only the pinned official assets listed in
`packaging/THIRD-PARTY-ASSETS.txt`, checks every archive hash, validates the
RetroBat version and central executables, installs the supplementary backends,
builds both launchers, assembles the installation, runs RetroPort's self-check,
and verifies the resulting integrity manifest. It is safe to rerun: verified
downloads and the verified RetroBat base are reused.

The command needs roughly 7 GB of free space plus Cargo's build cache. Set
`RETROPORT_DOWNLOAD_CACHE=/path` to choose the verified-download cache.

## Build the source-only application

Rust 1.92.0 is pinned in `rust-toolchain.toml`. The embedded catalogues and
controls evidence are already checked in, so building the GUI does not require
cloning every emulator source tree.

```sh
cargo build --release --target x86_64-unknown-linux-gnu
cargo xwin build --release --target x86_64-pc-windows-msvc
```

`cargo-xwin` 0.23.0 is installed automatically and is needed only when
cross-compiling the Windows launcher from Linux. The Windows target uses the
static MSVC runtime.

## Bootstrap the official RetroBat base

On Linux, install `curl`, `7z`, and `sha256sum`, then run:

```sh
./tools/bootstrap_retrobat_base.sh
```

The script downloads the official RetroBat 8.1.2 self-extracting release,
checks its exact 1,838,225,617-byte SHA-256-pinned payload, extracts it into a
new `RetroBat/` directory, and verifies the installed version and three central
executables. On later runs it accepts only an existing base that passes those
same checks; it never silently replaces an unknown directory.

On Windows, the same official setup can be downloaded from the pinned release
and installed into `RetroBat/`:

<https://github.com/RetroBat-Official/retrobat/releases/tag/8.1.2>

RetroBat 8.1.2 intentionally ships the frontend and RetroArch base while its
own **Updates & Downloads** workflow installs other standalone emulators. This
is upstream's supported dependency mechanism. RetroPort's supplementary native
Linux and firmware-free routes are listed with exact destinations, URLs,
versions, archive hashes, and licence notes in
`packaging/THIRD-PARTY-ASSETS.txt`.

## Reconstructing the tested complete bundle

The tested canonical bundle additionally contains:

- official Linux AppImages for RPCS3, Cemu, shadPS4, Eden, and Xenia Canary;
- supplementary Windows RPCS3, Cemu, shadPS4, Eden, Cxbx-Reloaded, Play!,
  Xenia Canary, and JAXE installations;
- RetroBat-downloaded RetroArch cores and their retained notices;
- generated portable configuration and readiness metadata.

The canonical bootstrap installs all pinned assets automatically. To run its
stages separately, use `bootstrap_retrobat_base.sh`, then
`bootstrap_supplementary_runtime.sh`, build both launchers, and run:

```sh
cargo test --all-targets
cargo test --test live_source -- --ignored
cargo clippy --all-targets -- -D warnings
cargo run -- --self-check --bundle-root "$PWD"
./tools/deploy_bundle.sh /path/to/independent/bundle
```

The deployment step creates a source snapshot and `SHA256SUMS`. Both the local
and destination copies must finish `VERIFY-LINUX.sh` or `VERIFY-WINDOWS.cmd`
successfully.

## User-supplied and private dependencies

Game imports are deliberately permissive and remain user-owned bundle state.
Firmware follows one of two flows:

- publisher-authorized immutable downloads are fetched and verified by the
  product on the user's machine;
- user-supplied firmware is imported without a rigid known-dump gate.

Neither flow authorizes committing commercial ROMs, saves, credentials, or
non-redistributable firmware to Git. The public source and the private assembled
product therefore cannot—and should not—be byte-for-byte identical directories.
