#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 BUNDLE_ROOT" >&2
    exit 2
fi

project=$(cd "$(dirname "$0")/.." && pwd)
bundle=$(realpath "$1")
linux_binary="$project/target/x86_64-unknown-linux-gnu/release/retrobat-portable"
windows_binary="$project/target/x86_64-pc-windows-msvc/release/retrobat-portable.exe"

if [[ ! -f "$bundle/RetroBat/RetroBat.exe" ]]; then
    echo "bundle is missing RetroBat/RetroBat.exe: $bundle" >&2
    exit 1
fi
for binary in "$linux_binary" "$windows_binary"; do
    if [[ ! -f "$binary" ]]; then
        echo "missing release build: $binary" >&2
        exit 1
    fi
done

mkdir -p "$bundle/Source"
python "$project/tools/build_source_snapshot.py" "$bundle/Source/RetroPort-source.zip"
cp -f "$linux_binary" "$bundle/RetroPort-Linux"
cp -f "$windows_binary" "$bundle/RetroPort.exe"
cp -f "$project/packaging/RetroPort-Linux.desktop" "$bundle/RetroPort-Linux.desktop"
cp -f "$project/packaging/README-FIRST.txt" "$bundle/README-FIRST.txt"
cp -f "$project/packaging/THIRD-PARTY-ASSETS.txt" "$bundle/THIRD-PARTY-ASSETS.txt"
cp -f "$project/packaging/VERIFY-LINUX.sh" "$bundle/VERIFY-LINUX.sh"
cp -f "$project/packaging/VERIFY-WINDOWS.cmd" "$bundle/VERIFY-WINDOWS.cmd"
cp -f "$project/packaging/VERIFY-WINDOWS.ps1" "$bundle/VERIFY-WINDOWS.ps1"
cp -f "$project/LICENSE" "$bundle/LICENSE-RETROPORT.txt"

rm -rf "$bundle/Artwork"
cp -a "$project/packaging/Artwork" "$bundle/Artwork"
chmod +x "$bundle/RetroPort-Linux" "$bundle/VERIFY-LINUX.sh"

"$project/tools/build_bundle_checksums.sh" "$bundle"
cp -f "$bundle/SHA256SUMS" "$project/packaging/SHA256SUMS"
echo "Deployed and checksummed RetroPort at $bundle"
