#!/usr/bin/env bash
set -eu
checker="$(cd "$(dirname "$0")" && pwd)/check-readme.sh"
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
valid='## Windows
Download /releases/latest and 1.6 GB on first launch.
VOXIS_x64_cuda-setup.exe for NVIDIA
VOXIS_x64_vulkan-setup.exe for other GPUs
## Development
'
printf '%s' "$valid" > "$work/README.md"
if bash "$checker" "$work/README.md" > "$work/output" 2>&1; then
    echo 'PASS: readme_accepts_complete_windows_section'
else
    cat "$work/output"
    echo 'FAIL: readme_accepts_complete_windows_section'
    exit 1
fi

for needle in '/releases/latest' '_x64_cuda-setup.exe' '_x64_vulkan-setup.exe' 'NVIDIA' '1.6 GB'; do
    NEEDLE="$needle" awk '{ gsub(ENVIRON["NEEDLE"], "removed"); print }' "$work/README.md" > "$work/missing.md"
    if bash "$checker" "$work/missing.md" > "$work/output" 2>&1; then
        echo "FAIL: readme_rejects_missing_requirement: $needle"
        exit 1
    else
        code=$?
        [[ "$code" -eq 1 ]] && grep -Fq "$needle" "$work/output"
        echo "PASS: readme_rejects_missing_requirement: $needle"
    fi
done

printf '%s' "$valid" | sed 's/## Windows/## Linux/' > "$work/no-windows.md"
if bash "$checker" "$work/no-windows.md" > "$work/output" 2>&1; then exit 1; fi
grep -Fq 'no "## Windows" section' "$work/output"
echo 'PASS: readme_rejects_missing_windows_section'

printf '%s' "$valid" | sed 's/ for NVIDIA//' > "$work/wrong-row.md"
printf 'NVIDIA\n' >> "$work/wrong-row.md"
if bash "$checker" "$work/wrong-row.md" > "$work/output" 2>&1; then exit 1; fi
grep -Fq 'NVIDIA beside the CUDA edition' "$work/output"
echo 'PASS: readme_rejects_nvidia_outside_cuda_row'

printf '## Windows\nIncomplete\n%s' "$valid" | sed '0,/## Windows/! s/## Windows/## Other/' > "$work/outside.md"
if bash "$checker" "$work/outside.md" > "$work/output" 2>&1; then exit 1; fi
echo 'PASS: readme_rejects_requirements_outside_windows_section'

sed 's/$/\r/' "$work/README.md" > "$work/crlf.md"
bash "$checker" "$work/crlf.md" > "$work/output"
echo 'PASS: readme_accepts_crlf'

if bash "$checker" "$work/absent.md" > "$work/output" 2>&1; then exit 1; fi
grep -Fq 'README not found' "$work/output"
echo 'PASS: readme_rejects_missing_file'
