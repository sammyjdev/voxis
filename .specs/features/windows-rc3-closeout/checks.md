# Windows rc3 closeout - draft acceptance proofs

Profile: standard
Plan: `plans/sammyjdev/voxis/windows-rc3-closeout/plan.md`
Build: `bash scripts/windows/rc3-baseline.sh`

Draft only. No proof below was run by the planner. Dani must finalize executable selectors after
Sammy approves; no tests or test harness are authored in this planning session. RC numbers are
local to the closeout plan; original C1-C29 remain unchanged. Pending evidence is not PASS.

## Checks

### S1 - rc3 installed artifact boundary

**C1** - R, tag target, run head and two asset hashes/IDs agree; release is draft (AC 1).
Proof: `powershell -File scripts\windows\check-release-assets.ps1 -Tag v0.2.0-rc3`
Proof: walk - compare tag target, PR candidate, run/attempt, draft metadata, exact asset names/IDs/sizes, downloaded SHA-256 and reference payload manifests; record equality or FAIL.

**C2** - Both installers exit 0 and place both executables and shortcut in the per-user location (AC 2).
Proof: `powershell -File scripts\windows\installer-smoke.ps1 -Edition cuda -Check install -InstallerDir <rc3-only-directory>`
Proof: `powershell -File scripts\windows\installer-smoke.ps1 -Edition vulkan -Check install -InstallerDir <rc3-only-directory>`

**C3** - Installed CUDA overlay returns 200 and the installed page without checkout/config dependency (AC 3).
Proof: `powershell -File scripts\windows\installer-smoke.ps1 -Edition cuda -Check overlay -InstallerDir <rc3-only-directory>`
Proof: walk - compare the served title/page with installed resources; confirm checkout frontend and developer tray.json are unavailable during the request; verify recovery journal and restored hashes.

**C4** - Installed CUDA devices exits 0 without toolkit PATH (AC 4).
Proof: `powershell -File scripts\windows\installer-smoke.ps1 -Edition cuda -Check no-toolkit -InstallerDir <rc3-only-directory>`

**C5** - Vulkan has no cuBLAS and a fresh transcript contains the original smoke keyword; the actual transcription exits 0 (AC 5).
Proof: `powershell -File scripts\windows\installer-smoke.ps1 -Edition vulkan -Check transcribe -InstallerDir <rc3-only-directory>`
Proof: walk - before uninstall, independently install the pinned Vulkan artifact with /S, use a unique empty session with the smoke's SAPI-generated mic.wav, invoke installed meetrec.exe transcribe with the explicit pinned turbo/Silero model paths and pt, retain stdout/stderr, immediately capture LASTEXITCODE=0 and read fresh transcript.md for transcrição; reject nonzero even with matching text. Record installed binary hash and models source, never silently use bench fallback as fresh-install proof. Uninstall and restore through the recovery journal.

**C6** - CUDA -> Vulkan keeps one entry and the same directory, removes cuBLAS, and runs installed Vulkan transcription (AC 6).
Proof: `powershell -File scripts\windows\installer-smoke.ps1 -Edition cuda,vulkan -Check replace -InstallerDir <rc3-only-directory> -Keep`
Proof: walk - retain before/after directories and installed hashes, assert one entry and zero cuBLAS, run C5's direct transcription witness on this resulting installation without reinstalling either edition; then uninstall and recover.

**C7** - Vulkan -> CUDA keeps one entry and the same directory, restores both named DLLs, and runs installed CUDA without toolkit PATH (AC 7).
Proof: `powershell -File scripts\windows\installer-smoke.ps1 -Edition vulkan,cuda -Check replace -InstallerDir <rc3-only-directory> -Keep`
Proof: walk - on this resulting installation without reinstalling, match meetrec.exe and both named DLL hashes to the CUDA reference payload, set PATH to C:\Windows\System32;C:\Windows, run the absolute installed meetrec.exe devices --json, capture stdout/stderr and exit 0 immediately, restore PATH in finally; then uninstall and recover. The current unamended replace script alone cannot prove this check.

**C8** - Uninstall of either edition leaves every model byte unchanged and zero uninstall entries (AC 8).
Proof: `powershell -File scripts\windows\installer-smoke.ps1 -Edition cuda -Check uninstall-keeps-models -InstallerDir <rc3-only-directory>`
Proof: `powershell -File scripts\windows\installer-smoke.ps1 -Edition vulkan -Check uninstall-keeps-models -InstallerDir <rc3-only-directory>`
Proof: walk - compare before/after SHA-256 for both real pinned models, not just the script marker; restore any pre-existing smoke-marker.bin.

**C9** - A reverse-switch proof rejects a missing DLL or nonzero installed runtime, exit 1 (AC 9).
Proof: `bash scripts/windows/check-replacement-test.sh`

The focused native Bash regression command was implemented under the execution amendment.
It runs only the replacement assertion path with temporary files and controlled command exits,
without a real installer, registry mutation or real installation. Named cases:
`reverse_replacement_rejects_missing_cublas`, `reverse_replacement_rejects_missing_cublaslt`,
`reverse_replacement_rejects_failed_devices`, `reverse_replacement_accepts_valid_cuda`.
Each prints its name and result. Before the smoke change, the command must exit 1 because the
invalid cases are wrongly accepted. Afterward, it exits 0 only when all invalid fixtures yield
proof exit 1 and the valid fixture yields proof exit 0. Rita independently repeats it and checks
that it reaches the smoke's actual assertion path, not a duplicate implementation.

**C10** - Independent report accounts for original C1-C29 and C1-C9 here, with recovered state and draft PR (AC 10).
Proof: walk - Rita joins each original check to final-R evidence or an explicit unchanged-source carry, appends rc3 verification and runs the installed skill's check_proofs.py and validate_verification.py. Record outputs, authorship, candidate versus evidence-only head, restoration hashes, PR draft status, F8/F10 decisions and security degradation. Missing independent verification or recovery is BLOCKED, never self-verified PASS.

## Exact release and smoke matrix

`<rc3-only-directory>` is a freshly created directory with only the two SHA-256-pinned release
installers. Replace that placeholder in the frozen checks before running. No latest-by-time choice
across rc2/rc3 is allowed. Each script call runs in its own non-elevated PowerShell process; the
supervisor records that process's exit code. `-Keep` is intentional for switch witnesses so they
exercise the resulting installation rather than a later clean reinstall.

| Order | Owner | Action and decisive evidence |
| --- | --- | --- |
| 1 | Dani | Original gate baseline green; finalize C9 selector; red check then narrow smoke change and green development evidence |
| 2 | Rita | Independently review narrow diff and proof sensitivity; fixes only through owner-approved Felix |
| 3 | Dani + Sammy | Freeze full R and obtain exact-SHA tag authorization; recheck PR head, tag and release absence |
| 4 | Dani | Push approved rc3 tag; observe windows(cuda), windows(vulkan), windows-release; retain run head/attempt and failures separately |
| 5 | Rita | C1 asset/draft identity and payload reference manifests, then recovery preflight |
| 6 | Rita | C2 cuda, C2 vulkan, C3 overlay, C4 no-toolkit, C5 transcribe with direct exit witness |
| 7 | Rita | C6 cuda->vulkan with installed transcription before cleanup |
| 8 | Rita | C7 vulkan->cuda with DLL/reference hashes and stripped-PATH devices before cleanup |
| 9 | Rita | C8 cuda and vulkan uninstall plus real model hashes; complete restoration |
| 10 | Rita | Original gate at R, C9 independent sensitivity, full original coverage and C10 final report/gates |
| 11 | Coordination + Vera | DM acceptance package; Vera PASS/FAIL/BLOCKED; no membership change |
| 12 | Sammy | Final installer acceptance only; separate future decision for whole port/merge |

## Coverage

| Set (size) | Member -> proof | Unproven |
| --- | --- | --- |
| rc3 asset editions (2) | cuda C1; vulkan C1 | - |
| installer editions (2) | cuda C2; vulkan C2 | - |
| edition transitions (2) | cuda->vulkan C6; vulkan->cuda C7 | - |
| reverse CUDA DLLs (2) | cublas64_13.dll C7 C9; cublasLt64_13.dll C7 C9 | - |
| uninstall editions (2) | cuda C8; vulkan C8 | - |
| invalid reverse fixtures (3) | first DLL missing C9; second DLL missing C9; runtime exit 1 C9 | - |
| original checks (29) | C10, table-driven accounting over all 29 original IDs in Rita's report | - |
| closeout Landing doors (1) | rc3 identity C1 | - |

These are proof assignments, not observed coverage results. All observations await execution.

## Swept

- validation: C1 asset identities; C9 invalid reverse fixtures
- failure modes: C3 recovery on failure/interruption; C5 process exit; C10 blocks incomplete recovery
- idempotency: C1 stop on existing tag/release; C6 C7 installation replacement
- authorization: C2 non-elevated install; C1 exact-SHA owner authorization before remote write
- concurrency: existing - single-instance tray; smokes are serial with test-owned PID tracking
- data lifecycle: C8 model hashes; C10 backup and restoration
- dependency failure: C4 missing toolkit PATH; C5 direct process failure; C1 independent Windows release job
- state transitions: C6 C7 both replacement directions
- observability: C1 C10 identities, outputs, exit codes, evidence paths and independent verdict

## Handoff

- **Settled mid-build:** native Bash execution authorized by Sammy in message 11fa12c4-3882-464f-9b22-ffd425097c73; exact words are in the plan amendment.
- **Dispatched:** Rita independent review pending at implementation HEAD 26d839621abd4e47da26ef183f4f78c1907f7da1, range 55260fe4d019ae53e7d7895417539d9e0f124322..HEAD; read-only review of this Bash execution slice, not final installer acceptance.
- **Boundary:** baseline native tests passed (meetrec 25, tray 61); Bash README and replacement development fixtures pass. Installed rc3 artifacts and independent verification are pending; release SHA not frozen.
- **Abandoned:** none.
- **Walked:** none; every walk proof above is a future acceptance procedure.
- Approval must resolve existing-PR route. Exact R/tag approval occurs at execution freeze.
- Approval is recorded in plan.md with its source message.
- Original clone and user checkout are inspection sources; no reset, cleanup or execution occurred.

## Native Bash execution amendment

User authorization: Engineering message `11fa12c4-3882-464f-9b22-ffd425097c73`.

**C11** - The Bash baseline SHALL retain meetrec/tray commands and all original C19 README checks, exit nonzero on any failure, and call no PowerShell script.
Proof: `bash scripts/windows/check-readme-test.sh`
Proof: `bash scripts/windows/rc3-baseline-test.sh`
Proof: `bash scripts/windows/rc3-baseline.sh`

The new Bash route replaces the blocked development baseline invocation only;
All C1-C10 remain pending their required installed-artifact and independent proofs.