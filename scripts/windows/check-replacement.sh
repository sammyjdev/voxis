#!/usr/bin/env bash
# Read-only witness after an edition replacement. Does not install or uninstall.
# Usage: bash scripts/windows/check-replacement.sh cuda /c/path/to/original/install
set -euo pipefail

check_replacement() {
    local before=$1 after=$2 edition=$3 entries=$4
    if [[ "$entries" != 1 ]]; then
        echo "FAIL: $entries uninstall entries named VOXIS"
        return 1
    fi
    before=$(cd -P -- "$(cygpath -u "$before")" && pwd) || return 1
    after=$(cd -P -- "$(cygpath -u "$after")" && pwd) || return 1
    if [[ "${before,,}" != "${after,,}" ]]; then
        echo "FAIL: installation directory changed: $before -> $after"
        return 1
    fi
    if [[ "$edition" == vulkan ]] && compgen -G "$after/cublas*.dll" > /dev/null; then
        echo 'FAIL: cublas DLLs left behind after switching to vulkan'
        return 1
    fi
    if [[ "$edition" == cuda ]]; then
        local dll
        for dll in cublas64_13.dll cublasLt64_13.dll; do
            if [[ ! -f "$after/$dll" ]]; then
                echo "FAIL: $dll missing after switching to cuda"
                return 1
            fi
        done
        local code
        if (PATH='/c/Windows/System32:/c/Windows' "$after/meetrec.exe" devices --json); then
            echo 'PASS: installed CUDA devices --json exited 0 without toolkit PATH'
        else
            code=$?
            echo "FAIL: installed CUDA devices --json exited $code without toolkit PATH"
            return 1
        fi
    fi
    echo 'PASS: one VOXIS entry after replacement'
}

main() {
    if [[ $# != 2 || ( "$1" != cuda && "$1" != vulkan ) ]]; then
        echo 'Usage: check-replacement.sh <cuda|vulkan> <original-install-directory>' >&2
        return 1
    fi
    local registry
    registry=$(python3 - <<'PY'
import os
import re
import winreg

entries = []
try:
    root = winreg.OpenKey(winreg.HKEY_CURRENT_USER,
                         r"Software\Microsoft\Windows\CurrentVersion\Uninstall")
except FileNotFoundError:
    root = None
if root is not None:
    with root:
        for index in range(winreg.QueryInfoKey(root)[0]):
            with winreg.OpenKey(root, winreg.EnumKey(root, index)) as key:
                try:
                    name = winreg.QueryValueEx(key, "DisplayName")[0]
                except FileNotFoundError:
                    continue
                if name == "VOXIS":
                    command = winreg.QueryValueEx(key, "UninstallString")[0]
                    match = re.match(r'^"([^"]+)"|^(.+?\.exe)(?:\s|$)', command, re.I)
                    if not match:
                        raise ValueError("Unrecognized VOXIS uninstall command")
                    entries.append(os.path.dirname(match[1] or match[2]))
print(len(entries))
if len(entries) == 1:
    print(entries[0])
PY
    ) || return 1
    # Native Windows Python ends its lines with CRLF.
    registry=${registry//$'\r'/}
    local -a values
    mapfile -t values <<< "$registry"
    check_replacement "$2" "${values[1]:-}" "$1" "${values[0]}"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then main "$@"; fi
