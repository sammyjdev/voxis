# Windows installer

## Problem

On Windows, VOXIS only runs from a developer checkout. The tray is started from
`apps\tray\src-tauri\target\release\tray.exe`, and `~/.config/voice-at/tray.json` points
`miris_bin` and `laris_bin` at a build folder (`C:\vxc\release\meetrec.exe`). `meetrec` reads the
LARIS orb page from the checkout path baked in at compile time (`packages/laris/frontend`). It reads
the whisper models from the benchmark cache (`~/.cache/voxis-bench/models`). The CUDA build only
starts because the CUDA Toolkit puts `cublas64_13.dll` and `cublasLt64_13.dll` on `PATH`
(about 530 MB).

Who pays: the owner restarts VOXIS by hand after every login, and the Windows autostart was never
set up. Anyone else cannot use it at all: there is no download, and a copied exe fails on the first
missing path or DLL. The owner wants two downloadable editions in the repository: CUDA for NVIDIA
GPUs, and Vulkan for any GPU. No usage numbers exist; this is a distribution gap, not a measured
loss.

When this ships, a user downloads one installer from the GitHub release and installs it without an
admin prompt. VOXIS then appears in the Start menu, and on first launch it fetches the models
itself. From then on dictation and meetings work, and VOXIS can start with Windows.

## Flow

Reuses Tauri's own NSIS bundler, sidecar mechanism and resource directory, and the existing
`tray-build.yml` release pipeline (tag `v*` drafts a GitHub release). No custom installer and no
second release channel.

1. tag `v*` -> `tray-build.yml` (exists) - adds a Windows job, matrix `cuda` and `vulkan`. Each
   builds `meetrec` with that feature and places it as the sidecar (door 2). The CUDA job also
   copies the cuBLAS DLLs as resources (door 4). The job then runs the Tauri NSIS bundle (door 1)
   and uploads the edition-named installer to the draft release (door 3).
2. installer -> NSIS per-user install - `VOXIS.exe` (the tray), `meetrec.exe` next to it,
   `resources/frontend` (the orb page), plus the DLLs in the CUDA edition, and a Start menu
   shortcut.
3. launch -> `apps/tray` `settings.rs` (exists) - an unset `miris_bin` or `laris_bin` resolves
   to the sidecar next to `VOXIS.exe`; an explicit `tray.json` value still wins.
4. `apps/tray` `laris.rs` (exists) - spawns `meetrec dictate` with `VOXIS_FRONTEND` set to the
   resource dir.
5. `meetrec` `models()` (exists) - looks in `VOXIS_MODELS`, then `%LOCALAPPDATA%\voxis\models`
   (door 5), then the legacy bench cache. If none is found, the tray runs `meetrec models fetch`
   (new command), which downloads the pinned files to `.part`, verifies SHA-256, renames them, and
   prints progress lines that the tray shows in the tooltip and a toast.
6. tray menu `Iniciar com o Windows` -> `apps/tray` `autostart.rs` (door 11) - writes or removes the
   HKCU `Run` entry, path quoted.
7. meeting -> `meetrec` `session.rs` / `minutes.rs` (exists) - without Google credentials, the
   session ends `completed` with `ata.md` kept locally instead of `failed`.

## Impact

| Front | What changes |
| --- | --- |
| domain | new term: `edition` - `cuda` or `vulkan`, the GPU backend baked into an installer; lives in the CI matrix and the asset name |
| domain | existing term: `miris_bin` / `laris_bin` defaults were `~/.local/bin/miris` and empty (Python daemon); on Windows they become the bundled sidecar. macOS/Linux defaults unchanged. Read by `lib.rs` (start/stop/process) and `laris.rs` (dictation spawn) |
| domain | existing behaviour: a meeting with no Google credentials was `failed`; now `completed` with a local `ata.md`. Read by the tray's toast on `process` result (`miris.rs`) |
| stored data | models: nothing moves. The owner's bench cache keeps working as the last lookup; new installs write `%LOCALAPPDATA%\voxis\models` |
| stored data | the owner's `tray.json` still points at `C:\vxc`: it keeps winning until its two keys are removed (said in the README and at handoff), and it is never edited by the installer |
| macOS build | `tauri.conf.json` is shared; Windows-only settings go in `tauri.windows.conf.json`, so the `.dmg` job is unchanged |

## Relations

None - no stored-data shape change (files on disk only, no schema).

## Surface

| Route | In | Out | Status |
| --- | --- | --- | --- |
| GitHub release assets | tag `v<ver>` | `VOXIS_<ver>_x64_cuda-setup.exe`, `VOXIS_<ver>_x64_vulkan-setup.exe` | `200`, `404` (asset download: published, edition job failed) |

## Landing

| One-way door | Literal shape | Alternative rejected |
| --- | --- | --- |
| 1. install mode | `tauri.windows.conf.json`: `"bundle": {"targets": ["nsis"], "windows": {"nsis": {"installMode": "currentUser"}}}`; identifier stays `dev.sammyj.voxis` for both editions, so one replaces the other | `perMachine`: needs a UAC prompt, and moving an existing install base between modes leaves stale copies. MSI (WiX): no per-user mode without elevation tricks |
| 2. sidecar layout | `"bundle": {"externalBin": ["binaries/meetrec"]}`, file `binaries/meetrec-x86_64-pc-windows-msvc.exe`, installed as `meetrec.exe` beside `VOXIS.exe` | a resource copy run through a hand-built path: Tauri would not resolve it, and every path lookup in the tray would be ours to maintain |
| 3. release asset names | `VOXIS_<ver>_x64_cuda-setup.exe`, `VOXIS_<ver>_x64_vulkan-setup.exe`, on the same draft release as the `.dmg` | Tauri's default `VOXIS_<ver>_x64-setup.exe`: both editions would collide on one name. README links and anyone scripting downloads bind to these names |
| 4. NVIDIA redistributables | CUDA edition ships `cublas64_13.dll` and `cublasLt64_13.dll` from CUDA 13.5 in the install dir | requiring the CUDA Toolkit on the user's PC: a 3 GB developer install for a dictation app. Static cuBLAS: not offered for this whisper.cpp build |
| 5. models location and pins | `%LOCALAPPDATA%\voxis\models\`; `ggml-large-v3-turbo.bin` from `huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/`, sha256 `1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69`, 1624555275 B; `ggml-silero-v5.1.2.bin` from `huggingface.co/ggml-org/whisper-vad/resolve/9ffd54a1e1ee413ddf265af9913beaf518d1639b/`, sha256 `29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf` | bundling the models: 1.6 GB per installer, and GitHub caps a release asset at 2 GiB. `resolve/main`: a moved branch would change the bytes under a pinned hash |
| 6. autostart dependency | `tauri-plugin-autostart` (HKCU `...\CurrentVersion\Run` value `VOXIS`) | a hand-written registry call: same effect, but the plugin also covers macOS (LaunchAgent) for the shared tray |
| 7. CI builds the CUDA edition | `windows-latest` job installs CUDA 13.5 (`Jimver/cuda-toolkit`, pinned by SHA like the other actions) and the Vulkan SDK; `CUDAARCHS="75;86;89;120"` | building on the owner's PC and uploading by hand: not reproducible, and it ties every release to one machine |

| 8. main executable name (found while building S1) | `tauri.windows.conf.json`: `"mainBinaryName": "VOXIS"`, so the install holds `VOXIS.exe` | Tauri's default, the crate name `tray.exe`: the Start menu target, the autostart Run value and anything a user scripts would bind to an internal name |
| 9. SHA-256 in meetrec (found while building S2) | dependency `sha2 = "0.10"` (RustCrypto) in `spikes/windows-rust/meetrec/Cargo.toml` | Windows CNG through `windows-sys`: Windows-only, and meetrec is meant to stay portable to the other two OSes; shelling out to `certutil`: a second process and output parsing for every model |
| 10. installer hook (found while building S3) | `tauri.windows.conf.json`: `"installerHooks": "windows/installer-hooks.nsh"`, whose `NSIS_HOOK_PREINSTALL` deletes `$INSTDIR\cublas64_13.dll` and `$INSTDIR\cublasLt64_13.dll`, which the CUDA installer then writes back | a custom NSIS template: the whole installer becomes ours to keep in step with Tauri; leaving the DLLs: the Vulkan edition would carry 530 MB it never loads and C16 fails |
| 11. autostart write, supersedes 6 (owner's choice, 2026-11-06, while building S5) | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` value `VOXIS` = `"<install dir>\VOXIS.exe"` (quoted), written and removed with the `windows` crate's registry API already in the tray | `tauri-plugin-autostart` (row 6): it writes the path unquoted, which a profile with a space in its name turns into the unquoted-path pattern, and the item is Windows-only so its macOS support buys nothing |
- Nothing else in this change is hard to reverse

## Criteria

### S1: the owner installs the CUDA edition and dictates without a checkout (P1)

The thinnest real path: installer, Start menu, tray, bundled `meetrec`, orb page from resources.

**Acceptance Criteria**

1. WHEN the CUDA installer runs for a user without admin rights THEN the installer SHALL finish without a UAC prompt, place `VOXIS.exe` and `meetrec.exe` in the same per-user folder, and create the Start menu shortcut `VOXIS`
2. WHERE `tray.json` has no `miris_bin` and no `laris_bin` the installed tray SHALL run the `meetrec.exe` that sits beside `VOXIS.exe` for meetings and dictation
3. WHERE `tray.json` sets `miris_bin` or `laris_bin` the tray SHALL run that path instead of the sidecar
4. WHILE the installed dictation daemon runs the orb server SHALL answer `GET /overlay` with `200` and the page from the installed `frontend` resource, with the checkout's `packages/laris/frontend` absent
5. WHEN the installed CUDA `meetrec.exe devices --json` runs with a `PATH` that contains no CUDA Toolkit directory THEN it SHALL exit `0`

**Independent test:** install on this PC, remove the two keys from `tray.json`, rename `C:\vxc`, open VOXIS from the Start menu, hold F13 and dictate one sentence.

### S2: a fresh install fetches its models (P1)

**Acceptance Criteria**

6. The `meetrec` model lookup SHALL try `VOXIS_MODELS`, then `%LOCALAPPDATA%\voxis\models`, then `%USERPROFILE%\.cache\voxis-bench\models`, and use the first folder that holds both model files
7. WHEN the tray starts and the lookup finds no folder THEN the tray SHALL run `meetrec models fetch` and show the download percentage in the tray tooltip
8. WHEN `meetrec models fetch` completes THEN `%LOCALAPPDATA%\voxis\models` SHALL hold both files with the pinned SHA-256 values, and the command SHALL exit `0`
9. IF a downloaded file's SHA-256 differs from its pin THEN `meetrec models fetch` SHALL delete it and exit `2`
10. IF the download stops before the last byte THEN no file SHALL exist under the final name, only `<name>.part`
11. WHILE the models are missing or downloading the tray SHALL NOT start the dictation daemon and SHALL show the disabled menu item `Ditado indisponível: baixando modelos (N%)` (amended 2026-11-06, owner OK: the key is hooked by the daemon, so a key-press toast cannot exist)

**Independent test:** `VOXIS_MODELS` unset, bench cache renamed, empty `%LOCALAPPDATA%\voxis\models`: launch, watch the tooltip reach 100 %, dictate.

### S3: the Vulkan edition (P2)

**Acceptance Criteria**

12. The Vulkan installer SHALL contain no `cublas*.dll`, and its `meetrec.exe` SHALL transcribe a meeting on this PC
13. WHEN one edition is installed over the other THEN the second installer SHALL replace the first in the same folder, leaving one `VOXIS` entry in Apps & features

**Independent test:** install Vulkan over CUDA, check Apps & features, record and process a 1-minute meeting.

### S4: downloadable from the repository (P2)

**Acceptance Criteria**

14. WHEN a tag `v<ver>` is pushed THEN `tray-build.yml` SHALL attach `VOXIS_<ver>_x64_cuda-setup.exe` and `VOXIS_<ver>_x64_vulkan-setup.exe` to that tag's draft release
15. The README SHALL have a Windows section that links the latest release, says which edition fits which GPU, and says that the first launch downloads 1.6 GB

**Independent test:** push a `v0.2.0-rc1` tag on the branch; the draft release lists both installers plus the `.dmg`.

### S5: starts with Windows (P2)

**Acceptance Criteria**

16. WHEN the user checks `Iniciar com o Windows` in the tray menu THEN `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` SHALL hold a `VOXIS` value pointing at the installed `VOXIS.exe`, and unchecking SHALL remove it

**Independent test:** toggle the item, read the Run key, log off and on.

### S6: meetings for someone without the owner's Google setup (P3)

**Acceptance Criteria**

17. IF `google-oauth-client.json` or `google-token.json` is missing THEN processing a meeting SHALL end with `session.json` `state: completed`, `ata.md` written in the session folder, and the tray toast naming that file
18. IF no Anthropic key is set THEN processing a meeting SHALL end `completed` with `transcript.md` kept and the toast `Ata não gerada: configure anthropic_api_key em %APPDATA%\voxis\secrets.toml`

**Independent test:** rename the two Google files, record a 1-minute meeting, check the toast and the folder.

## Out of scope

| Excluded | Why |
| --- | --- |
| code signing (Authenticode) | needs a paid certificate; until then SmartScreen shows "unknown publisher" and the README says so |
| auto-update (Tauri updater) | needs a signing key and an update feed; a manual reinstall is enough for now |
| a GUI to enter the Anthropic key or connect Google | the files in `%APPDATA%\voxis` stay the interface; S6 only makes their absence non-fatal |
| picking CUDA or Vulkan at runtime in one installer | the owner chose two editions |
| ARM64 Windows, Windows 10 testing | one x64 Windows 11 PC is the only test machine |

## Assumptions

| Assumption | Chosen default | Rationale | Confirmed? |
| --- | --- | --- | --- |
| cuBLAS may be redistributed | ship the two DLLs under NVIDIA's CUDA EULA, Attachment A (open question 1, resolved) | the only way to run CUDA without the toolkit | n |
| GPU architectures for the CUDA edition | `75;86;89;120` (Turing, Ampere, Ada, Blackwell) | covers the NVIDIA cards in use; each arch adds compile time and size | n |
| model download size and source | only turbo f16 + Silero VAD, from Hugging Face at pinned commits | what both editions use today; q8 or large-v3 are not used | n |
| owner's existing `tray.json` | untouched; the owner removes `miris_bin` and `laris_bin` to switch to the installed copy | an installer must not rewrite a user's config | n |
| app version | the release tag sets `version`; Tauri's `0.1.0` stays until the first tag | the installer file name carries the version | n |

**Open questions:**

| # | Kind | Question | Until answered |
| --- | --- | --- | --- |
| 1 | resolved 2026-11-06 | Does the CUDA 13.5 EULA list `cublas64_13.dll` and `cublasLt64_13.dll` as redistributable? | Yes: `EULA.txt` Attachment A lists "CUDA BLAS Library - Windows: cublas.dll, cublasLt.dll", version-named variants included; §1.1.2 asks for material added functionality and that only the app uses them, both true here |
| 2 | open | Can a CUDA whisper.cpp build for four archs finish inside GitHub's 6-hour job limit with a cache? | measured on the first CI run; fallback is fewer archs |

## Observable

| Surface | Decision | Landing |
| --- | --- | --- |
| installer | what it asks, where it installs | AC 1 |
| installer | upgrade or switch of edition | AC 13 |
| installer | uninstall | existing - Tauri NSIS uninstaller; models in `%LOCALAPPDATA%\voxis\models` are kept (user data) |
| tray menu `Iniciar com o Windows` | checked state reflects the Run key | AC 16 |
| tray tooltip and menu during the model download | progress and failure copy | AC 7, AC 11 |
| tray toast, model download failure | error copy | AC 9 (exit 2 -> toast `Falha ao baixar modelos: hash diferente; tente de novo`) |
| CLI `meetrec models fetch` (driven by the tray, same repo) | output format: lines `progress <file> <bytes> <total>` then `ready <dir>`; exit `0` ready, `1` network or HTTP error, `2` hash mismatch; halfway failure | AC 8, AC 9, AC 10 |
| CLI `meetrec models fetch` | flags | n/a - no flags; `VOXIS_MODELS` env is the only input |
| release page | asset naming, which edition to choose | AC 14, AC 15 |
| README Windows section | what the reader does next | AC 15 |
| meeting toast without Google / without key | copy | AC 17, AC 18 |

## Gate

`cargo test -p meetrec --release --no-default-features --features cuda` (inside vcvars64, `CMAKE_GENERATOR=Ninja`, `LIBCLANG_PATH`) and `cargo test --release` in `apps/tray/src-tauri` - from the Windows handoff notes; the repo has no `AGENTS.md` gate yet.

## Sources

- Owner's answers, 2026-11-06: NSIS installer; two editions (CUDA and Vulkan) published in the repository for others to download
- `C:\Users\samde\AppData\Local\Temp\handoff-20261106-voxis-bigger-models.md` - build commands, tray config, and the autostart that was never set up
