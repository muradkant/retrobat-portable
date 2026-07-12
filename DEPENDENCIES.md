# Reproducible assembly

RetroPort has two artifacts with different jobs:

1. The **Git repository** is reviewable: Rust source, catalogue and controls
   evidence, tests, tools, metadata, documentation, and redistributable artwork.
2. The **assembled installation** is playable: generated launchers, RetroBat,
   emulator runtimes, configuration, and user state.

A clone never borrows files from an installation, and one installation never
borrows from another.

## Inclusion boundary

| Component | In Git | Reconstructed from |
| --- | --- | --- |
| Rust source and lockfile | Yes | `src/`, `Cargo.toml`, `Cargo.lock` |
| Catalogue and evidence | Yes | `catalog/` |
| Redistributable MAME artwork | Yes | `packaging/Artwork/` |
| Build and audit tools | Yes | `tools/` |
| Provenance and binary evidence | Yes | this file and `packaging/THIRD-PARTY-ASSETS.txt` |
| RetroBat runtime | No | official 8.1.2 release into `RetroBat/` |
| Supplementary Windows backends | No | pinned official archives |
| Native Linux AppImages | No | pinned official archives |
| Generated launchers | No | tagged Rust source |
| Games, saves, caches, private firmware | Never | user-owned mutable state |

The runtime is about 5 GB before a game library grows. GitHub rejects ordinary
objects above 100 MiB and advises repositories below 1 GB; Git is also a poor
updater for thousands of generated files owned by independent projects. Pinned
release URLs and hashes give each binary a clearer provenance. Submodules would
only add emulator source: RetroPort consumes official releases rather than
rebuilding every upstream toolchain.

Some machine-installed files cannot be redistributed. Games, saves,
credentials, and Sony system software therefore never enter public history.
Redistributable large artifacts belong in versioned release storage after a
licence audit, not ordinary Git objects.

## Complete build

On Debian or Ubuntu Linux:

```sh
sudo apt-get update
sudo apt-get install -y build-essential curl git p7zip-full python3 rsync wine64
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"
git clone https://github.com/muradkant/retrobat-portable.git
cd retrobat-portable
./tools/bootstrap_bundle.sh
```

The bootstrap:

1. downloads only the pinned official artifacts in
   `packaging/THIRD-PARTY-ASSETS.txt`;
2. verifies each archive before extraction;
3. validates RetroBat's version and central executables;
4. installs supplementary Windows and native Linux backends;
5. builds `RetroPort-Linux` and `RetroPort.exe`;
6. assembles the local installation, runs application self-check, and verifies
   its integrity manifest.

Successful downloads and the validated RetroBat base are cached across reruns.
Set `RETROPORT_DOWNLOAD_CACHE=/path` to move that cache. Allow roughly 7 GB
beyond Cargo's build cache.

## Independent stages

### Application only

Rust 1.92.0 is pinned by `rust-toolchain.toml`. Embedded catalogue and controls
snapshots make emulator source checkouts unnecessary.

```sh
cargo build --release --target x86_64-unknown-linux-gnu
cargo xwin build --release --target x86_64-pc-windows-msvc
```

The bootstrap installs pinned `cargo-xwin` 0.23.0 when needed. Its Windows
target uses the static MSVC runtime.

### RetroBat base only

```sh
./tools/bootstrap_retrobat_base.sh
```

This downloads the official RetroBat 8.1.2 self-extracting release, verifies
its exact 1,838,225,617-byte payload, extracts `RetroBat/`, then checks the
version marker and three central executables. A later run reuses that directory
only if the same checks pass; it never replaces an unknown tree.

Windows users can install the same release into `RetroBat/` from the
[official 8.1.2 page](https://github.com/RetroBat-Official/retrobat/releases/tag/8.1.2).
RetroBat deliberately ships its frontend and RetroArch base while **Updates &
Downloads** supplies further standalone emulators.

### Supplementary runtime and verification

The complete installation adds native Linux RPCS3, Cemu, shadPS4, Eden, and
Xenia AppImages; Windows RPCS3, Cemu, shadPS4, Eden, Cxbx-Reloaded, Play!, Xenia,
and JAXE; installed RetroArch cores; configuration; and readiness metadata.
`bootstrap_supplementary_runtime.sh` reconstructs these from the asset ledger.

After running both bootstrap stages and building both launchers:

```sh
cargo test --all-targets
cargo test --test live_source -- --ignored
cargo clippy --all-targets -- -D warnings
cargo run -- --self-check --bundle-root "$PWD"
./tools/deploy_bundle.sh /destination
```

Deployment writes the corresponding source snapshot and `SHA256SUMS`. Verify
both source and destination with `VERIFY-LINUX.sh` or `VERIFY-WINDOWS.cmd`.

## Mutable state

Game import accepts compatible structure without demanding one canonical dump.
Firmware is either downloaded from a declared publisher URL and verified, or
selected by the user and recorded without a known-hash gate. Both remain
user-owned state; neither makes a private installation byte-for-byte identical
to the public repository.
