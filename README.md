# RetroBat Portable

This is the first tested vertical slice of a cross-platform, single-entry-point
RetroBat bundle manager. It is intentionally conservative: catalog entries are
installable only when their source, license, byte size, and SHA-256 have been
reviewed and pinned.

The library is artwork-first. Source-provided images have their own immutable
URL, size, and SHA-256; they are fetched asynchronously into the portable
cache and decoded only after verification.

## Bundle layout

```text
Arcade/
├── RetroBat Portable.exe       # Windows build
├── retrobat-portable           # Linux build
├── RetroBat/
│   └── RetroBat.exe
└── .retrobat-portable/         # transaction and ownership records
```

Windows starts the bundled `RetroBat.exe` directly. Linux starts that same
payload with Wine and keeps the Wine prefix in the user's local data directory,
because an exFAT thumb drive cannot safely contain Wine's Unix symlinks.

## Development checks

```sh
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo run -- --self-check --bundle-root /path/to/Arcade
cargo run -- --bundle-root /path/to/Arcade --install homebrew-hub/2048gb
```

The Windows release is linked with the static C runtime and uses the Windows
GUI subsystem, so launching it does not require the Visual C++ redistributable
and does not open a console window.

No commercial ROMs or firmware are supplied. The initial live item is
`2048gb`, published under the Zlib license through Homebrew Hub. Its artifact is
pinned to a database commit and checked before it can reach `RetroBat/roms/gb`.
