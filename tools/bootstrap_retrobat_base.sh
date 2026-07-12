#!/usr/bin/env bash
set -euo pipefail

project=$(cd "$(dirname "$0")/.." && pwd)
root=${1:-$project}
root=$(realpath -m "$root")
destination="$root/RetroBat"
cache=${RETROPORT_DOWNLOAD_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/retroport/bootstrap}
installer="$cache/RetroBat-v8.1.2-stable-win64-setup.exe"
url=https://github.com/RetroBat-Official/retrobat/releases/download/8.1.2/RetroBat-v8.1.2-stable-win64-setup.exe
installer_size=1838225617
installer_sha256=d6b6987f87d1903c02e6654f0e5f43de0b4b311f86bd355b5796a870c73a63d7

for command in curl sha256sum 7z realpath; do
    if ! command -v "$command" >/dev/null; then
        echo "missing required command: $command" >&2
        exit 1
    fi
done

verify_runtime() {
    local installed=$1 checks
    [[ -f "$installed/system/version.info" ]] || return 1
    [[ $(tr -d '\r\n\357\273\277' < "$installed/system/version.info") == 8.1.2-stable-win64 ]] || return 1
    checks=$(mktemp)
    cat >"$checks" <<'EOF'
9ddaab99656c969d3fcd69588a9a7aabe0dc6623d2cb289c2a5f710ae4d12d1d  RetroBat.exe
6d80d5aaa8adc8e76faf97ecfcbd0c93dbf1903d93339a2fbee3c8cd3bd37e0b  emulationstation/emulatorLauncher.exe
4f210b84cfce00b66f6e18d1f568b49b8523c1465a36ced92ef8b4aae38b3681  emulationstation/emulationstation.exe
EOF
    (cd "$installed" && sha256sum --check --status "$checks")
    local status=$?
    rm -f "$checks"
    return "$status"
}

if [[ -e "$destination" ]]; then
    if verify_runtime "$destination"; then
        echo "Verified existing RetroBat 8.1.2 base at $destination"
        exit 0
    fi
    echo "existing RetroBat directory is not the verified 8.1.2 base: $destination" >&2
    echo "move it aside or remove it, then rerun this command" >&2
    exit 1
fi

mkdir -p "$cache" "$root"
if [[ ! -f "$installer" ]] ||
    [[ $(stat -c %s "$installer") != "$installer_size" ]] ||
    ! echo "$installer_sha256  $installer" | sha256sum --check --status
then
    rm -f "$installer"
    curl --fail --location --continue-at - --output "$installer" "$url"
fi

echo "$installer_sha256  $installer" | sha256sum --check
if [[ $(stat -c %s "$installer") != "$installer_size" ]]; then
    echo "RetroBat installer has an unexpected byte size" >&2
    exit 1
fi

staging=$(mktemp -d "$root/.retrobat-base.XXXXXX")
cleanup() {
    rm -rf "$staging"
}
trap cleanup EXIT

# The official setup is a self-extracting ZIP. 7-Zip returns warning status 1
# because executable bytes precede/follow the ZIP payload; extracted content is
# still verified below. Any status greater than 1 is a real extraction failure.
set +e
7z x -y -o"$staging" "$installer"
extract_status=$?
set -e
if (( extract_status > 1 )); then
    echo "RetroBat extraction failed with status $extract_status" >&2
    exit 1
fi

version=$(tr -d '\r\n\357\273\277' < "$staging/system/version.info")
if [[ "$version" != 8.1.2-stable-win64 ]]; then
    echo "unexpected RetroBat version: $version" >&2
    exit 1
fi

if ! verify_runtime "$staging"; then
    echo "extracted RetroBat base failed executable verification" >&2
    exit 1
fi

mv "$staging" "$destination"
trap - EXIT
echo "Installed verified RetroBat 8.1.2 base at $destination"
echo "Use RetroBat Updates & Downloads and packaging/THIRD-PARTY-ASSETS.txt for supplementary backends."
