# Architecture contract

The native Rust executable owns four responsibilities:

1. present the library and approved catalog;
2. install only integrity-pinned, explicitly licensed content;
3. preserve an ownership manifest so uninstall never deletes changed files;
4. launch RetroBat directly on Windows or through a local Wine prefix on Linux.

RetroBat remains the mature emulator configuration and EmulationStation layer.
This project does not duplicate its emulator matrix.

## Security boundaries

- Catalog records are rejected on unknown schema, duplicate identifiers, unsafe
  paths, absent licenses, malformed URLs, malformed hashes, or unverified trust.
- Downloads enter a staging directory on the same volume and are promoted only
  after exact size and SHA-256 verification.
- Artwork follows the same integrity policy and is cached under a content hash;
  corrupt cached or downloaded images are discarded before decoding.
- Absolute paths, `..`, and symlinked destination directories are rejected.
- An existing unowned ROM is never overwritten.
- Uninstall removes only files whose current hash still equals the ownership
  manifest; modified files are preserved.

The built-in catalog is trusted as part of the signed application release.
Remote catalog updates will require a separately pinned signing key before they
are enabled.
