#!/usr/bin/env bash
set -eu
repo=$(cd "$(dirname "$0")/../.." && pwd)
readme=${1:-"$repo/README.md"}
if [[ ! -f "$readme" ]]; then
    echo "FAIL: README not found: $readme"
    exit 1
fi
awk '
    /^## Windows[[:space:]]*$/ && !found { found = 1; active = 1; next }
    active && /^## / { active = 0 }
    active {
        section = section $0 "\n"
        if (!cuda_seen && index($0, "_x64_cuda-setup.exe")) {
            cuda_seen = 1
            cuda_row = $0
        }
    }
    END {
        if (!found) { print "FAIL: no \"## Windows\" section"; exit 1 }
        split("/releases/latest|_x64_cuda-setup.exe|_x64_vulkan-setup.exe|NVIDIA|1.6 GB", needles, "|")
        for (i = 1; i <= 5; i++) {
            if (!index(section, needles[i])) missing = missing " " needles[i]
        }
        if (!index(cuda_row, "NVIDIA")) missing = missing " NVIDIA beside the CUDA edition"
        if (missing != "") { print "FAIL: Windows section lacks" missing; exit 1 }
        print "PASS: README Windows section links the release, names both editions and the download size"
    }
' "$readme"
