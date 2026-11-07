#!/usr/bin/env bash
set -eu
script_dir=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
mkdir -p "$work/installed folder"
touch "$work/installed folder/cublas64_13.dll" "$work/installed folder/cublasLt64_13.dll"
export VOXIS_SMOKE_LOG="$work/runtime.log"
cat > "$work/installed folder/meetrec.exe" <<'PROBE'
#!/usr/bin/bash
printf '%s\n' "$1 $2" "$PATH" > "$VOXIS_SMOKE_LOG"
[[ "$1" == devices && "$2" == --json ]] || exit 2
[[ "$PATH" == '/c/Windows/System32:/c/Windows' ]] || exit 3
exit 0
PROBE
chmod +x "$work/installed folder/meetrec.exe"
if ! source "$script_dir/check-replacement.sh"; then
    echo 'FAIL: reverse_replacement_accepts_valid_cuda'
    exit 1
fi
check_replacement "$work/installed folder" "$work/installed folder" cuda 1
grep -Fxq 'devices --json' "$work/runtime.log"
grep -Fxq '/c/Windows/System32:/c/Windows' "$work/runtime.log"
echo 'PASS: reverse_replacement_accepts_valid_cuda'

rm -- "$work/installed folder/cublas64_13.dll"
if check_replacement "$work/installed folder" "$work/installed folder" cuda 1 > "$work/output" 2>&1; then
    echo 'FAIL: reverse_replacement_rejects_missing_cublas'
    exit 1
fi
grep -Fq 'cublas64_13.dll' "$work/output"
echo 'PASS: reverse_replacement_rejects_missing_cublas'

touch "$work/installed folder/cublas64_13.dll"
rm -- "$work/installed folder/cublasLt64_13.dll"
if check_replacement "$work/installed folder" "$work/installed folder" cuda 1 > "$work/output" 2>&1; then exit 1; fi
grep -Fq 'cublasLt64_13.dll' "$work/output"
echo 'PASS: reverse_replacement_rejects_missing_cublaslt'
touch "$work/installed folder/cublasLt64_13.dll"

cat > "$work/installed folder/meetrec.exe" <<'PROBE'
#!/usr/bin/bash
[[ "$1" == devices && "$2" == --json ]] || exit 2
[[ "$PATH" == '/c/Windows/System32:/c/Windows' ]] || exit 3
exit 1
PROBE
chmod +x "$work/installed folder/meetrec.exe"
if check_replacement "$work/installed folder" "$work/installed folder" cuda 1 > "$work/output" 2>&1; then
    echo 'FAIL: reverse_replacement_rejects_failed_devices'
    exit 1
fi
grep -Fq 'devices --json exited 1' "$work/output"
echo 'PASS: reverse_replacement_rejects_failed_devices'

mkdir -p "$work/other folder"
if check_replacement "$work/installed folder" "$work/other folder" cuda 1 > "$work/output" 2>&1; then exit 1; fi
grep -Fq 'installation directory changed' "$work/output"
echo 'PASS: replacement_rejects_changed_directory'

if check_replacement "$work/installed folder" "$work/installed folder" cuda 2 > "$work/output" 2>&1; then exit 1; fi
grep -Fq '2 uninstall entries' "$work/output"
echo 'PASS: replacement_rejects_duplicate_entries'

if check_replacement "$work/installed folder" "$work/installed folder" vulkan 1 > "$work/output" 2>&1; then exit 1; fi
grep -Fq 'cublas DLLs left behind' "$work/output"
echo 'PASS: vulkan_replacement_rejects_leftover_cublas'
rm -- "$work/installed folder/cublas64_13.dll" "$work/installed folder/cublasLt64_13.dll"
check_replacement "$work/installed folder" "$work/installed folder" vulkan 1
echo 'PASS: vulkan_replacement_accepts_absent_cublas'

# Command-line boundary: the registry helper output arrives as native Windows
# Python prints it, with CRLF line endings. A python3 stub replaces the helper.
cli="$work/cli"
mkdir -p "$cli/bin" "$cli/installed folder"
touch "$cli/installed folder/cublas64_13.dll" "$cli/installed folder/cublasLt64_13.dll"
cat > "$cli/bin/python3" <<'STUB'
#!/usr/bin/bash
cat > /dev/null
cat -- "$VOXIS_REGISTRY_OUTPUT"
STUB
cat > "$cli/installed folder/meetrec.exe" <<'PROBE'
#!/usr/bin/bash
printf '%s\n' "$1 $2" > "$VOXIS_SMOKE_LOG"
[[ "$1" == devices && "$2" == --json ]] || exit 2
[[ "$PATH" == '/c/Windows/System32:/c/Windows' ]] || exit 3
exit "$VOXIS_DEVICES_EXIT"
PROBE
chmod +x "$cli/bin/python3" "$cli/installed folder/meetrec.exe"
export VOXIS_REGISTRY_OUTPUT="$cli/registry.out" VOXIS_SMOKE_LOG="$cli/runtime.log"
installed_windows_path=$(cygpath -w "$cli/installed folder")

run_cli() {
    local code=0
    PATH="$cli/bin:$PATH" bash "$script_dir/check-replacement.sh" cuda "$cli/installed folder" \
        > "$cli/output" 2>&1 || code=$?
    echo "$code"
}

printf '1\r\n%s\r\n' "$installed_windows_path" > "$VOXIS_REGISTRY_OUTPUT"
rm -f -- "$VOXIS_SMOKE_LOG"
export VOXIS_DEVICES_EXIT=0
code=$(run_cli)
if [[ "$code" != 0 ]] || ! grep -Fxq 'devices --json' "$VOXIS_SMOKE_LOG" 2> /dev/null; then
    cat -- "$cli/output"
    echo "FAIL: cli_accepts_crlf_registry_output_for_valid_cuda (exit $code)"
    exit 1
fi
echo 'PASS: cli_accepts_crlf_registry_output_for_valid_cuda'

printf '2\r\n' > "$VOXIS_REGISTRY_OUTPUT"
export VOXIS_DEVICES_EXIT=0
code=$(run_cli)
if [[ "$code" != 1 ]] || ! grep -Fq 'FAIL: 2 uninstall entries' "$cli/output"; then
    cat -- "$cli/output"
    echo "FAIL: cli_rejects_crlf_duplicate_entries (exit $code)"
    exit 1
fi
echo 'PASS: cli_rejects_crlf_duplicate_entries'

printf '1\r\n%s\r\n' "$installed_windows_path" > "$VOXIS_REGISTRY_OUTPUT"
export VOXIS_DEVICES_EXIT=1
code=$(run_cli)
if [[ "$code" != 1 ]] || ! grep -Fq 'devices --json exited 1' "$cli/output"; then
    cat -- "$cli/output"
    echo "FAIL: cli_rejects_crlf_failed_devices (exit $code)"
    exit 1
fi
echo 'PASS: cli_rejects_crlf_failed_devices'
