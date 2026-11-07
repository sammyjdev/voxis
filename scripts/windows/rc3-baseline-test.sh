#!/usr/bin/env bash
set -eu
runner="$(cd "$(dirname "$0")" && pwd)/rc3-baseline.sh"
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
mkdir -p "$work/bin"
export VOXIS_BASELINE_PROBE="$work/calls"
cat > "$work/bin/cmd.exe" <<'PROBE'
#!/usr/bin/bash
printf 'cmd: %s\n' "$*" >> "$VOXIS_BASELINE_PROBE"
exit "$VOXIS_CMD_EXIT"
PROBE
cat > "$work/bin/cargo" <<'PROBE'
#!/usr/bin/bash
printf 'cargo: %s\n' "$*" >> "$VOXIS_BASELINE_PROBE"
exit "$VOXIS_CARGO_EXIT"
PROBE
chmod +x "$work/bin/cmd.exe" "$work/bin/cargo"
export PATH="$work/bin:$PATH"

if VOXIS_CMD_EXIT=7 VOXIS_CARGO_EXIT=0 bash "$runner" "$work/meetrec-failure" > "$work/output" 2>&1; then exit 1; else code=$?; fi
[[ "$code" -eq 7 && $(cat "$work/meetrec-failure/meetrec.exit") == 7 ]]
[[ ! -f "$work/meetrec-failure/tray.exit" ]]
[[ ! -f "$work/meetrec-failure/readme.exit" ]]
echo 'PASS: baseline_stops_and_records_meetrec_failure'

if VOXIS_CMD_EXIT=0 VOXIS_CARGO_EXIT=9 bash "$runner" "$work/tray-failure" > "$work/output" 2>&1; then exit 1; else code=$?; fi
[[ "$code" -eq 9 && $(cat "$work/tray-failure/tray.exit") == 9 ]]
[[ ! -f "$work/tray-failure/readme.exit" ]]
echo 'PASS: baseline_stops_and_records_tray_failure'

VOXIS_CMD_EXIT=0 VOXIS_CARGO_EXIT=0 bash "$runner" "$work/success" > "$work/output" 2>&1
[[ $(cat "$work/success/readme.exit") == 0 ]]
grep -Fq 'PASS: Windows rc3 baseline' "$work/output"
grep -Fq 'cmd: /d /c scripts\windows\meetrec-test.cmd' "$work/calls"
grep -Fq 'cargo: test --release --manifest-path apps/tray/src-tauri/Cargo.toml' "$work/calls"
echo 'PASS: baseline_runs_original_commands_and_bash_readme_check'
