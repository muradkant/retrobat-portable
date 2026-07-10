RETROPORT — START HERE
======================

Everything used at run time is inside this folder. Keep the folder together;
do not move RetroPort.exe or RetroPort-Linux away from it.

WINDOWS
-------
1. Plug in the drive and open this folder.
2. Double-click RetroPort.exe.
3. If Windows shows an unknown-publisher warning, choose More info, then Run
   anyway. This personal build is not code-signed.

LINUX
-----
1. Install 64-bit Wine from your Linux distribution for the legacy systems.
2. Double-click RetroPort-Linux.desktop, or run ./RetroPort-Linux here.

RPCS3, Cemu, shadPS4, Eden, and Xenia run through native Linux AppImages. The
remaining RetroBat stack runs through Wine. Games, saves, artwork, emulator
state, and catalogues remain in this folder.

GET PLAYING
-----------
1. Browse FEATURED for 410 evidence-backed iconic/community-praised titles, or
   ALL SOURCES for all 80,734 catalogue records. Cards fill the available
   window in a multi-row grid. Search always spans every source and system.
2. Use the action on the game card:
   DOWNLOAD     Fetches, verifies, prepares, and installs a hosted game.
   IMPORT GAME  Selects a compatible local ROM, disc, executable, or game.
   PLAY         Starts the exact imported/downloaded game.
3. For extracted PS3, PS4, Wii U, or PC games, click IMPORT THIS FOLDER. This
   preserves EBOOT/RPX files, DLLs, and data directories together.

Imports deliberately do not demand one exact database dump. Alternate
revisions, translations, and patches are accepted. A known-dump mismatch is
recorded for troubleshooting and never blocks PLAY.

FIRMWARE
--------
RetroPort prefers established firmware-free routes where available: Play! for
PS2, YabaSanshiro for Saturn, and Cxbx-Reloaded for original Xbox.

INSTALL FIRMWARE downloads an exact publisher file and verifies it before the
emulator installer opens. Sony PS3 system software uses this path.

IMPORT FIRMWARE accepts a nonempty file selected by the user without an exact
hash gate. Switch prod.keys are copied automatically into both Windows and
Linux Eden profiles. Optional firmware uses the same screen. If a core has a
documented built-in fallback, no false required-firmware warning is shown.

CONTROLLER
----------
Connect the controller before PLAY. RetroPort passes control to RetroBat and
its established controller profiles. The supplied Xbox-compatible controller
path was probed through Linux xpad, Wine XInput, RetroBat/EmulationStation, and
RetroArch. RetroBat's usual exit shortcut is HOTKEY + START.

VERIFY THIS COPY
----------------
Windows: double-click VERIFY-WINDOWS.cmd.
Linux:   run ./VERIFY-LINUX.sh.

The commands verify the launchers, source snapshot, documentation, modern
Linux runtimes, supplementary Windows backends, and all bundled artwork. A
successful run ends with “VERIFICATION PASSED.”

IF A GAME DOES NOT START
------------------------
1. Reopen its card and read the backend/firmware line.
2. For CUE, GDI, and M3U, keep all referenced tracks/discs beside the
   descriptor before import.
3. For an extracted installation, import the folder rather than one executable.
4. Run the verifier above.
5. On Linux, confirm `wine --version` works and GPU drivers support Vulkan.

CONTENTS
--------
RetroPort.exe                 Windows GUI
RetroPort-Linux               Linux GUI
RetroPort-Linux.desktop       Linux graphical shortcut
RetroBat/                     frontend, adapters, emulators, ROMs, saves
Runtime/Linux/                native modern-console runtimes
Artwork/                      verified local artwork
Source/RetroPort-source.zip   exact Rust source snapshot
SHA256SUMS                    static bundle integrity manifest
VERIFY-LINUX.sh               Linux verifier
VERIFY-WINDOWS.cmd/.ps1       Windows verifier
THIRD-PARTY-ASSETS.txt        upstream URLs, versions, hashes, licences
LICENSE-RETROPORT.txt         RetroPort source licence

RetroBat and emulator licence/notices are retained beside their respective
programs. Imported games, saves, private machine data, and mutable configs are
not part of the public static checksum manifest.
