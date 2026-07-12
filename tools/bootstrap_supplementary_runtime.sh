#!/usr/bin/env bash
set -euo pipefail

project=$(cd "$(dirname "$0")/.." && pwd)
root=${1:-$project}
root=$(realpath -m "$root")
cache=${RETROPORT_DOWNLOAD_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/retroport/bootstrap}

for command in curl sha256sum 7z rsync realpath; do
    if ! command -v "$command" >/dev/null; then
        echo "missing required command: $command" >&2
        exit 1
    fi
done
if [[ ! -f "$root/RetroBat/RetroBat.exe" ]]; then
    echo "RetroBat base is missing; run tools/bootstrap_retrobat_base.sh first" >&2
    exit 1
fi

mkdir -p "$cache" "$root/Runtime/Linux/shadPS4"

fetch() {
    local name=$1 url=$2 expected=$3
    local output="$cache/$name"
    if [[ ! -f "$output" ]] ||
        ! echo "$expected  $output" | sha256sum --check --status
    then
        rm -f "$output"
        echo "Downloading $name" >&2
        curl --fail --location --retry 4 --retry-all-errors \
            --continue-at - --output "$output" "$url" >&2
    fi
    if ! echo "$expected  $output" | sha256sum --check >&2; then
        echo "refusing unverified download: $output" >&2
        return 1
    fi
    printf '%s\n' "$output"
}

install_archive() {
    local archive=$1 marker=$2 destination=$3
    local staging
    staging=$(mktemp -d "$root/.runtime-asset.XXXXXX")
    7z x -y -o"$staging" "$archive" >/dev/null
    local found
    found=$(find "$staging" -type f -iname "$marker" -print -quit)
    if [[ -z "$found" ]]; then
        echo "archive $(basename "$archive") does not contain $marker" >&2
        rm -rf "$staging"
        exit 1
    fi
    mkdir -p "$destination"
    rsync -a "$(dirname "$found")/" "$destination/"
    rm -rf "$staging"
}

install_direct() {
    local source=$1 destination=$2 mode=${3:-755}
    mkdir -p "$(dirname "$destination")"
    install -m "$mode" "$source" "$destination"
}

# Windows backends and cores.
jaxe=$(fetch \
    jaxe_libretro.dll.zip \
    https://buildbot.libretro.com/nightly/windows/x86_64/latest/jaxe_libretro.dll.zip \
    1b239ae91d742b615daffbf2d8ab6154d78912261d964918a4b301a18999e9e0)
install_archive "$jaxe" jaxe_libretro.dll "$root/RetroBat/emulators/retroarch/cores"

xenia_win=$(fetch \
    xenia_canary_windows-6e5b832.7z \
    https://github.com/xenia-canary/xenia-canary/releases/download/6e5b832/xenia_canary_windows.7z \
    fe43847b26b73140bdf131259f540b12ed7edcb2bf18dd846dc5bc1cf7e293dd)
install_archive "$xenia_win" xenia_canary.exe "$root/RetroBat/emulators/xenia-canary"

rpcs3_win=$(fetch \
    rpcs3-v0.0.41-19564-700ca262_win64_msvc.7z \
    https://github.com/RPCS3/rpcs3-binaries-win/releases/download/build-700ca262f44fda57ba260283c3f0a4772db8a573/rpcs3-v0.0.41-19564-700ca262_win64_msvc.7z \
    3d0e7b796df5ec05fa2d9448d4c1203f97ae2f605bc16dad3bc175ed858c191e)
install_archive "$rpcs3_win" rpcs3.exe "$root/RetroBat/emulators/rpcs3"

cemu_win=$(fetch \
    cemu-2.6-windows-x64.zip \
    https://github.com/cemu-project/Cemu/releases/download/v2.6/cemu-2.6-windows-x64.zip \
    a6bcc2bc42a362d10213819948f3152fae7d47f70067f25939b51d3ddcfb0896)
install_archive "$cemu_win" Cemu.exe "$root/RetroBat/emulators/cemu"

shad_win=$(fetch \
    shadps4-win64-sdl-0.16.0.zip \
    https://github.com/shadps4-emu/shadPS4/releases/download/v.0.16.0/shadps4-win64-sdl-0.16.0.zip \
    f6cdcca82f239fb69b2f820ad9dec07f2f00b423273851b67a0e24bc783acf46)
install_archive "$shad_win" shadPS4.exe "$root/RetroBat/emulators/shadps4"

shad_qt=$(fetch \
    shadPS4QtLauncher-win64-qt-v224.zip \
    https://github.com/shadps4-emu/shadps4-qtlauncher/releases/download/v224/shadPS4QtLauncher-win64-qt-v224.zip \
    021871249bae6867900f62efd728a6cbdbed9934b69c28488b41d413280e2e1d)
install_archive "$shad_qt" shadPS4QtLauncher.exe "$root/RetroBat/emulators/shadps4"

eden_win=$(fetch \
    Eden-Windows-v0.2.1-amd64-msvc-standard.zip \
    https://stable.eden-emu.dev/v0.2.1/Eden-Windows-v0.2.1-amd64-msvc-standard.zip \
    ff498e5da9630216926ac3cbe9fb493b14930665c728a40a8f5b59507fdd7ebf)
install_archive "$eden_win" eden.exe "$root/RetroBat/emulators/eden"

cxbx=$(fetch \
    CxbxReloaded-CI-585c49a.zip \
    https://github.com/Cxbx-Reloaded/Cxbx-Reloaded/releases/download/CI-585c49a/CxbxReloaded-Release.zip \
    010d1e85bee9f82f05ae57ca483e7ae61fecba06c1637bf6b5a74ca09b03bf43)
install_archive "$cxbx" cxbx.exe "$root/RetroBat/emulators/cxbx-reloaded"

play=$(fetch \
    Play-x86-64-0.70.exe \
    https://www.purei.org/downloads/play/stable/0.70/Play-x86-64.exe \
    d4cd4583694d555771483526b87b7ff29ca42b0fab0693ae69d1189575a56883)
install_direct "$play" "$root/RetroBat/emulators/play/Play.exe"

# Native Linux routes.
xenia_linux=$(fetch \
    XeniaCanary-6e5b832.AppImage \
    https://github.com/xenia-canary/xenia-canary/releases/download/6e5b832/xenia_canary_linux.AppImage \
    6e0dba4e56fd5b48c0043be0879cb219c5dc9e2eed5e30f3df3cc0acb1177482)
install_direct "$xenia_linux" "$root/Runtime/Linux/XeniaCanary.AppImage"

rpcs3_linux=$(fetch \
    RPCS3-700ca262.AppImage \
    https://github.com/RPCS3/rpcs3-binaries-linux/releases/download/build-700ca262f44fda57ba260283c3f0a4772db8a573/rpcs3-v0.0.41-19564-700ca262_linux64.AppImage \
    190cb796ffce3cfb61f56f03ae44efa7fd8331d49fb37412ba5e6d950a2bd59b)
install_direct "$rpcs3_linux" "$root/Runtime/Linux/RPCS3.AppImage"

cemu_linux=$(fetch \
    Cemu-2.6-x86_64.AppImage \
    https://github.com/cemu-project/Cemu/releases/download/v2.6/Cemu-2.6-x86_64.AppImage \
    0c20c4aeb800bb13d9bab9474ef45a6f8fcde6402cad9b32ac2a1bbd03186313)
install_direct "$cemu_linux" "$root/Runtime/Linux/Cemu.AppImage"

shad_linux=$(fetch \
    shadps4-linux-sdl-0.16.0.zip \
    https://github.com/shadps4-emu/shadPS4/releases/download/v.0.16.0/shadps4-linux-sdl-0.16.0.zip \
    7cbb19fe8c909e04129d2431eef723d4710499b40d1aed0047681d14a1dfc79b)
install_archive "$shad_linux" '*.AppImage' "$root/Runtime/Linux/shadPS4"
shad_appimage=$(find "$root/Runtime/Linux/shadPS4" -maxdepth 1 -type f -iname '*.AppImage' -print -quit)
if [[ "$shad_appimage" != "$root/Runtime/Linux/shadPS4/Shadps4-sdl.AppImage" ]]; then
    mv -f "$shad_appimage" "$root/Runtime/Linux/shadPS4/Shadps4-sdl.AppImage"
fi
chmod +x "$root/Runtime/Linux/shadPS4/Shadps4-sdl.AppImage"

eden_linux=$(fetch \
    Eden-Linux-v0.2.1-amd64-gcc-standard.AppImage \
    https://stable.eden-emu.dev/v0.2.1/Eden-Linux-v0.2.1-amd64-gcc-standard.AppImage \
    2fae658397daf13c118082a3eb65d61a6519967b5e22e6667756baecf6000c5a)
install_direct "$eden_linux" "$root/Runtime/Linux/Eden.AppImage"

echo "Installed and verified all pinned supplementary Windows and Linux backends."
