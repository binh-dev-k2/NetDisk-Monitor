#!/bin/bash
# Run from any directory: bash scripts/validate-macos-disk.sh
# Requires existing rustc and Xcode Command Line Tools; installs nothing.
set -euo pipefail
mode=${1:-live}
if [[ $mode != live && $mode != abi-only ]]; then
  echo 'usage: validate-macos-disk.sh [live|abi-only]' >&2
  exit 2
fi
if [[ $(uname -s) != Darwin ]]; then
  echo 'Native validation requires macOS; Windows can run the pure Rust tests.' >&2
  exit 2
fi
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/netdisk-macos-validation.XXXXXX")
trap 'rm -rf -- "$work"' EXIT
sw_vers
uname -m
rustc --version
xcrun --sdk macosx --show-sdk-version
xcrun --sdk macosx clang -std=c11 -Wall -Wextra -Werror \
  "$root/scripts/macos_disk_reference.c" -framework IOKit -framework CoreFoundation \
  -o "$work/reference"
rustc --edition 2021 -D warnings --test "$root/src-tauri/src/macos_disk.rs" -o "$work/pure-tests"
"$work/pure-tests"
rustc --edition 2021 -D warnings "$root/scripts/macos_disk_probe.rs" -o "$work/probe"
if [[ $mode == abi-only ]]; then
  echo 'SDK ABI and pure tests passed. Live disk counters were NOT validated.'
  exit 0
fi
"$work/probe" "$work/reference" "${NETDISK_TEST_DISK_DIR:-$work}"
