#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")"
count=$(wc -l < SHA256SUMS)
printf 'Verifying %s bundle files...\n' "$count"
sha256sum -c --quiet SHA256SUMS
printf 'VERIFICATION PASSED. All %s files are intact.\n' "$count"
