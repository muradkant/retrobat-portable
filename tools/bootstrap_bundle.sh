#!/usr/bin/env bash
set -euo pipefail

project=$(cd "$(dirname "$0")/.." && pwd)

for command in cargo python3 rsync; do
    if ! command -v "$command" >/dev/null; then
        echo "missing required command: $command" >&2
        exit 1
    fi
done

"$project/tools/bootstrap_retrobat_base.sh" "$project"
"$project/tools/bootstrap_supplementary_runtime.sh" "$project"
mkdir -p "$project/.retrobat-portable" "$project/Artwork"

cargo build --release --target x86_64-unknown-linux-gnu \
    --manifest-path "$project/Cargo.toml"

if ! cargo xwin --version 2>/dev/null | grep -q ' 0\.23\.0$'; then
    echo "Installing pinned cargo-xwin 0.23.0..."
    cargo install cargo-xwin --version 0.23.0 --locked
fi
cargo xwin build --release --target x86_64-pc-windows-msvc \
    --manifest-path "$project/Cargo.toml"

"$project/tools/deploy_bundle.sh" "$project"
"$project/RetroPort-Linux" --self-check --bundle-root "$project" >/dev/null
"$project/VERIFY-LINUX.sh"

echo
echo "RetroPort is ready."
echo "Linux:  $project/RetroPort-Linux"
echo "Windows: $project/RetroPort.exe"
