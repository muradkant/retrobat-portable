#!/usr/bin/env bash
set -euo pipefail

# Application-menu entry point for the development/source machine. Cargo's
# incremental build makes this cheap when nothing changed and guarantees that
# the process being opened corresponds to the current checkout, never to a
# stale executable copied to a deployment drive.
project=$(cd "$(dirname "$0")/.." && pwd)
target=x86_64-unknown-linux-gnu

for required in \
    "$project/RetroBat/RetroBat.exe" \
    "$project/RetroBat/emulationstation/emulatorLauncher.exe" \
    "$project/RetroBat/emulationstation/.emulationstation/es_systems.cfg"
do
    if [[ ! -f "$required" ]]; then
        command -v notify-send >/dev/null && \
            notify-send --urgency=critical "RetroPort local bundle is incomplete" \
            "Missing $required. The local copy will not borrow files from another drive."
        echo "local RetroPort bundle is incomplete: missing $required" >&2
        exit 1
    fi
done

if ! cargo build --quiet --release --target "$target" --manifest-path "$project/Cargo.toml"; then
    command -v notify-send >/dev/null && \
        notify-send --urgency=critical "RetroPort build failed" \
        "Open a terminal in $project and run cargo build --release."
    exit 1
fi

exec "$project/target/$target/release/retrobat-portable" --bundle-root "$project" "$@"
