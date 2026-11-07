#!/usr/bin/env bash
# Run from Git Bash. No PowerShell scripts or execution-policy changes.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
evidence=${1:-$(mktemp -d)}
mkdir -p "$evidence"
evidence=$(cd "$evidence" && pwd)
cd "$repo"

run_check() {
    local name=$1 code
    shift
    if "$@" > "$evidence/$name.log" 2>&1; then code=0; else code=$?; fi
    cat "$evidence/$name.log"
    printf '%s\n' "$code" > "$evidence/$name.exit"
    printf '%s: exit %s\n' "$name" "$code"
    return "$code"
}

run_check meetrec env MSYS2_ARG_CONV_EXCL='*' CARGO_TARGET_DIR="${MEETREC_TARGET_DIR:-C:\vxc}" \
    cmd.exe /d /c 'scripts\windows\meetrec-test.cmd'
run_check tray env CARGO_TARGET_DIR="${TRAY_TARGET_DIR:-C:\vxg}" \
    cargo test --release --manifest-path apps/tray/src-tauri/Cargo.toml
run_check readme bash scripts/windows/check-readme.sh
printf 'PASS: Windows rc3 baseline; evidence: %s\n' "$evidence"
