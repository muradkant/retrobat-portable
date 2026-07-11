#!/usr/bin/env bash
set -euo pipefail

# Application-menu entry point for the development/source machine. Cargo's
# incremental build makes this cheap when nothing changed and guarantees that
# the process being opened corresponds to the current checkout, never to a
# stale executable copied to a deployment drive.
project=$(cd "$(dirname "$0")/.." && pwd)
target=x86_64-unknown-linux-gnu

if ! cargo build --quiet --release --target "$target" --manifest-path "$project/Cargo.toml"; then
    command -v notify-send >/dev/null && \
        notify-send --urgency=critical "RetroPort build failed" \
        "Open a terminal in $project and run cargo build --release."
    exit 1
fi

exec "$project/target/$target/release/retrobat-portable"
