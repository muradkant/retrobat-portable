# Architecture contract

RetroPort is a Rust control plane over established emulators. It owns the
catalogue, acquisition, import, firmware setup, backend selection, and process
lifecycle—not emulation. RetroBat and EmulatorLauncher supply legacy Windows
integration; RetroArch and standalone emulators execute games.

## Runtime

```text
catalogue card
  ├─ DOWNLOAD ─ verify bytes ─ transactional install ─┐
  └─ IMPORT ─── safe copy ─── ownership manifest ────┤
                                                     ├─ readiness + firmware ─ PLAY
                                                     │
                    Windows: EmulatorLauncher or native executable
                    Linux:   native AppImage or Wine adapter
```

Catalogue, featured-title, and controls evidence is compiled into the
application. Search documents and import manifests are precomputed and cached.
Bounded workers load artwork, reject excessive dimensions or allocations, and
run initial and post-operation readiness audits before results reach the UI
thread.

## Controls

- Every catalogue record has **CONTROLS**; self-check and a full-catalogue test
  enforce that invariant.
- The base mapping comes from installed RetroArch/RetroBat configuration and
  the connected device's SDL autoconfiguration.
- MAME `-listxml` input declarations join the pinned Libretro MAME DAT; an
  imported ROM stem selects its machine variant.
- RetroBat's `gamesdb.xml` supplies special-device metadata.
- The view names its source, scope, and confidence. Core-runtime mappings stay
  labelled as such; absent action names are never fabricated.

## Process lifecycle

- Each game owns a Unix process group. Windows termination addresses the whole
  process tree.
- **PLAY** becomes **LOADING**, then **TERMINATE**; a game cannot launch twice.
- A watcher owns and reaps the child, so focus events and GUI polling do not
  govern exit detection.
- An active game stops needless frontend redraws while occluding the window.
  The native Wayland loop remains free for compositor pings, and the watcher
  requests the exit repaint.
- The gameplay probe holds a real imported game beyond compositor timeout
  thresholds, terminates its tree, and records every transition.
- On Linux, the optional
  [`linux-zhixu-controller-fix`](https://github.com/muradkant/linux-zhixu-controller-fix)
  guard is held from spawn until the process group disappears. Natural exit,
  termination, and launch failure release it; concurrent inhibitors remain
  independent. Other hosts use a no-op guard.

## Import

- System extension and structure decide compatibility; an exact dump hash does
  not. Known SHA-1 matches are diagnostic only.
- CUE, GDI, and M3U imports include every safely referenced companion file.
- Directory import preserves PC libraries/data and extracted PS3, PS4, and Wii
  U layouts recursively. Symlinks and traversal fail closed.
- PS3 and PS4 trees become `.ps3` or `.ps4` RetroBat targets; Wii U and Windows
  trees resolve a launch file.
- An unowned destination is never overwritten.

## Firmware

- Automatic installs accept only a declared publisher URL, byte count, and
  SHA-256; unexpected bytes never reach an emulator.
- Manual import accepts any nonempty regular file and records its resulting
  hash. Directory requirements accept any safe filename in that directory.
- Documented HLE and built-in fallbacks suppress false requirements.
- Backend-specific data such as Eden `prod.keys` is mirrored into each portable
  platform profile.

## Trust boundary

- Catalogue input rejects unknown schemas, duplicate identifiers, unsafe
  paths, malformed URLs, malformed hashes, and unverifiable automatic installs.
- Network bytes enter staging on the destination volume and move into place
  only after verification.
- Absolute paths, `..`, symlink sources, and symlink destination parents are
  forbidden.
- Uninstall removes an owned file only while its current hash still matches the
  manifest; user-modified files survive.
- `SHA256SUMS` covers launchers, source, documentation, supplementary backends,
  Linux runtimes, and bundled artwork. Saves, imports, configuration, caches,
  and privately installed firmware remain mutable and therefore outside it.

Remote catalogue replacement is disabled. Any future update channel must add a
pinned signing key and rollback-safe snapshot activation first.
