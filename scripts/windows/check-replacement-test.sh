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
