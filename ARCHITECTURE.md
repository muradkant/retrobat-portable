# Architecture contract

RetroPort is the single Rust control plane above mature emulator projects. It
owns catalogue presentation, verified acquisition, permissive local import,
firmware setup, backend selection, and cross-platform process launch. RetroBat
and EmulatorLauncher remain the Windows integration/configuration layer;
RetroArch and established standalone emulators provide execution.

## Runtime flow

```text
catalogue card
  ├─ DOWNLOAD ─ verify source bytes ─ transactional install ─ PLAY
  └─ IMPORT ─── copy file/tree safely ─ ownership manifest ─ PLAY
                                                        │
                         backend readiness + firmware ──┘
                                   │
              Windows: EmulatorLauncher / native .exe
              Linux: native modern AppImage or Wine adapter
```

The browse snapshot and featured evidence are compiled into the application.
Artwork loading and decoding are bounded background jobs; image dimensions and
allocation limits are applied before data reaches the UI thread. Filter/search
documents are precomputed and cached, preventing the 80,734-card catalogue from
being reparsed on each frame.

## Import contract

- A local game is accepted by system extension and structure, not by an exact
  dump hash. Known SHA-1 matches are diagnostic metadata only.
- CUE, GDI, and M3U descriptors copy all safely referenced companion files.
- Directory import recursively preserves PC DLL/data trees and extracted
  PS3/PS4/Wii U layouts. Symlinks and traversal are rejected.
- PS3 and PS4 directories become `.ps3`/`.ps4` launch targets understood by
  RetroBat's generators; Wii U and Windows directories resolve a launch file.
- Existing unowned destinations are never overwritten.

## Firmware contract

- Publisher downloads are immutable records with URL, exact byte size, and
  SHA-256. Unexpected bytes never reach an emulator.
- User-selected firmware is intentionally permissive: any nonempty regular file
  can be imported or replaced, and its SHA-256 is recorded after placement.
- Directory firmware requirements accept any safe filename within the declared
  directory.
- Documented HLE/built-in fallbacks suppress false required-firmware states.
- Emulator-specific data such as Eden `prod.keys` is mirrored into each
  platform's portable profile.

## Security and integrity boundaries

- Catalogue records reject unknown schemas, duplicate identifiers, unsafe
  paths, malformed URLs, malformed hashes, or unverified automatic installs.
- Network content enters a staging directory on the destination volume and is
  promoted only after verification.
- Absolute paths, `..`, symlink sources, and symlink destination parents are
  rejected.
- Uninstall removes only files whose current hash still matches the ownership
  manifest; modified files are preserved.
- `SHA256SUMS` covers the product launchers, source snapshot, documentation,
  supplementary backends, native Linux runtimes, and bundled artwork. Mutable
  saves, imported games, configs, caches, and privately installed firmware are
  deliberately outside the public static manifest.

Remote catalogue replacement is not enabled. A future update channel requires
a separately pinned signing key and rollback-safe snapshot activation.
