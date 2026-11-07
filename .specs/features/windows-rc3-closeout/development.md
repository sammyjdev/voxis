# Native Bash execution - development evidence

This is author development evidence, not independent verification or installer acceptance.

## Authorization

Engineering message `11fa12c4-3882-464f-9b22-ffd425097c73` authorizes native Bash
execution. The exact user words are recorded in the plan amendment. Earlier
approval `ad171789-64a9-4e9f-8aa8-1f9166ce14bd` covers implementation on existing
draft PR #4. No tag, release publication or merge is authorized here.

## Red before green

- The first README fixture failed with exit 1 because `check-readme.sh` did not
  exist. After the native port, it passed. Negative fixtures cover all five
  original C19 strings, the Windows section boundary and NVIDIA on the CUDA row.
- The replacement witness initially preserved the old single-entry/Vulkan-DLL
  predicates. `reverse_replacement_rejects_missing_cublas` then failed with exit
  1 because the fixture was incorrectly accepted. Adding both CUDA DLL checks
  made that fixture green; missing cuBLASLt is covered separately.
- `reverse_replacement_rejects_failed_devices` next failed with exit 1 because a
  nonzero installed command was incorrectly accepted. Executing the installed
  command with the stripped PATH and propagating failure made it green.
- The changed-directory fixture failed before the folder assertion was added.
  After that assertion it passed. Duplicate entries and Vulkan cuBLAS leftovers
  retain negative controls.

## Development commands and observed results

| Command | Result |
| --- | --- |
| `bash scripts/windows/rc3-baseline.sh <evidence-directory>` | exit 0; meetrec 25 passed, tray 61 passed, README PASS |
| `bash scripts/windows/check-readme-test.sh` | exit 0; 11 named fixtures passed |
| `bash scripts/windows/rc3-baseline-test.sh` | exit 0; 3 named fixtures passed, including exact failure exits 7 and 9 |
| `bash scripts/windows/check-replacement-test.sh` | exit 0; 8 named fixtures passed |
| `bash -n` on the six new shell scripts | exit 0 |
| `validate_plan.py` | 0 errors, 1 warning about deferred questions |
| `validate_checks.py` | 0 errors, 14 selector warnings |

The isolated clone needed its generated ignored sidecar for the tray build.
It was copied from the existing owner checkout with matching SHA-256
`fa9bff6e60481bc95a0eeff7eb3ff835995430dca9baec043c4b9f45f70ea0e0`.
That copy is baseline setup, not evidence for an rc3 installer. The original
meetrec wrapper emitted a vswhere lookup warning but completed all 25 tests with
exit 0; the wrapper was not changed.

Full local logs and exit files:
`C:\Users\samde\OpenBot\Agents\agent-8157c88f-23fe-4c83-a4e3-b547de987e7d\rc3-evidence\`.
Native baseline logs are under `native-baseline/`; fixture outputs are named
`check-readme-test.log`, `rc3-baseline-test.log`, `check-replacement-test.log`.

## Review finding F16 - CRLF registry output

Rita's review 5809904701 at `ae47579da7199055dbf41a03e7571fa7aa766eeb` found that
native Windows Python prints the registry count as `1\r\n`, so `mapfile -t` kept
`1\r` and a valid installation failed the single-entry check. Correction of this
implementation was authorized in Engineering messages
`8d197ffe-c2b2-444a-b020-73ce4eccad1c` and `11fa12c4-3882-464f-9b22-ffd425097c73`.

- Criterion C12 was added to `checks.md` first; RC1-RC10 and C1-C11 are unchanged.
- Baseline at `ae47579`: the three Bash suites exited 0; `check-replacement.sh`
  SHA-256 `4bd0d2eec65bbb9e1708969aedcf359057dba7fa7999b05c489e89915d5bb7c2`.
- Red: three command-line cases run the script itself with a `python3` stub on
  PATH that prints fixed CRLF bytes. Against the unchanged script the suite
  exited 1 with `FAIL: 1\r uninstall entries named VOXIS` and
  `FAIL: cli_accepts_crlf_registry_output_for_valid_cuda (exit 1)`.
- Green: `main` removes carriage returns from the helper output before parsing.
  `check-replacement-test.sh` exited 0 with 11 named cases, including
  `cli_rejects_crlf_duplicate_entries` and `cli_rejects_crlf_failed_devices`, both
  exit 1 with their cause. `check-readme-test.sh` and `rc3-baseline-test.sh`
  exited 0; `bash -n` and `git diff --check` exited 0.
- A probe with this machine's native `python3` printing `1` and a folder showed
  the unchanged script exit 1 and the corrected script exit 0 (Vulkan, empty
  folder). No registry, installation or configuration was read or changed.
- `validate_plan.py` exit 0 (1 warning); `validate_checks.py` exit 0 (15 selector
  warnings).

Logs, exit files and hashes:
`C:\Users\samde\OpenBot\Shared\evidence\dani-voxis-f16-ae47579\{baseline,red,green}\`.
This is author development evidence; Rita's re-review and Vera's verification
at the new SHA are pending.

## Remaining obligations

- Rita independently reviews this slice and reruns its development proofs.
- RC1-RC8 and RC10 need final artifact, installed-state and restoration evidence.
  Historical PowerShell recipes remain pending, not successful Bash runs.
- Freeze the reviewed candidate SHA and obtain Sammy's exact-SHA tag approval.
- No installation, uninstall, configuration move, tag, release or merge was
  performed during this slice. PR #4 must remain draft.
