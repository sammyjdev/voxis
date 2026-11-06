# Windows installer checks

Profile: standard
Plan: `.specs\features\windows-installer\plan.md`

23 checks in 6 slices · 7 one-way doors · 2 open, of which 1 blocks go-live

Command shorthands used in the proofs:

- `MEETREC_TEST` = `cmd /c scripts\windows\meetrec-test.cmd` (vcvars64, Ninja, `LIBCLANG_PATH`, `cargo test -p meetrec --release --no-default-features --features cuda`)
- `TRAY_TEST` = `cargo test --release --manifest-path apps\tray\src-tauri\Cargo.toml`
- `SMOKE` = `powershell -File scripts\windows\installer-smoke.ps1`. It installs the given installer silently (`/S`) from a non-elevated shell, asserts, and uninstalls at the end unless `-Keep`

## Checks

### S1 - the owner installs the CUDA edition and dictates without a checkout · 6 files · 75 KB · ~19k

Done: C1-C6 green on 2026-11-06 (tray suite 48 passed; smoke install, overlay, no-toolkit PASS)

**C1** - The CUDA installer, run with `/S` from a non-elevated shell, exits `0`. It leaves `VOXIS.exe` and `meetrec.exe` in the same folder under `%LOCALAPPDATA%`, and `VOXIS.lnk` under `%APPDATA%\Microsoft\Windows\Start Menu\Programs` (AC 1)
Proof: `SMOKE -Edition cuda -Check install`

**C2** - With `tray.json` lacking `miris_bin` and `laris_bin`, both resolve to `<dir of VOXIS.exe>\meetrec.exe` on Windows (AC 2)
Proof: `TRAY_TEST an_unset_meetrec_path_runs_the_copy_beside_the_tray`

**C3** - A `miris_bin` or `laris_bin` set in `tray.json` is used as written, not the sidecar (AC 3)
Proof: `TRAY_TEST a_meetrec_path_set_in_tray_json_wins_over_the_bundled_copy`

**C4** - The native dictation daemon is spawned with `VOXIS_FRONTEND` = the tray's `frontend` resource dir (AC 4)
Proof: `TRAY_TEST the_dictation_daemon_is_told_where_the_installed_orb_page_is`

**C5** - With the installed tray running and the checkout's `packages\laris\frontend` renamed, `GET http://127.0.0.1:<orb port>/overlay` answers `200` and the body contains `<title>` from `overlay.html` (AC 4)
Proof: `SMOKE -Edition cuda -Check overlay`

**C6** - The installed CUDA `meetrec.exe devices --json` exits `0` with `PATH` reduced to `C:\Windows\System32;C:\Windows` (no CUDA Toolkit dir) (AC 5)
Proof: `SMOKE -Edition cuda -Check no-toolkit`

### S2 - a fresh install fetches its models · 5 files · 60 KB · ~15k

Done: C7-C14 green on 2026-11-06 (meetrec suite 18 passed, tray suite 50 passed; installed app fetched both pinned models in ~230 s, then started dictation)

**C7** - Model lookup: the first of `VOXIS_MODELS`, `%LOCALAPPDATA%\voxis\models`, `%USERPROFILE%\.cache\voxis-bench\models` that holds both files wins. A folder holding only one of them is skipped, and no match returns `Err` naming all three places (AC 6)
Proof: `MEETREC_TEST models_are_found_in_the_first_place_that_holds_both_files`

**C8** - `models fetch` against a local HTTP server serving the two files with matching pins writes both under the target dir, leaves no `.part`, prints `ready <dir>` and exits `0` (AC 8)
Proof: `MEETREC_TEST a_fetch_whose_hashes_match_saves_both_models`

**C9** - A served file whose SHA-256 differs from its pin is deleted (neither the final name nor `.part` remains), and `models fetch` exits `2` (AC 9)
Proof: `MEETREC_TEST a_model_whose_hash_differs_is_deleted_and_the_fetch_exits_2`

**C10** - A server that closes the connection halfway leaves `<name>.part` and no file under the final name, and `models fetch` exits `1` (AC 10)
Proof: `MEETREC_TEST an_interrupted_download_leaves_only_the_part_file`

**C11** - An unreachable server (connection refused) makes `models fetch` exit `1` with the URL in stderr (AC 9, dependency failure)
Proof: `MEETREC_TEST an_unreachable_model_server_exits_1`

**C12** - Running `models fetch` when both files are present with matching hashes makes no request and exits `0` (AC 8, idempotency)
Proof: `MEETREC_TEST fetching_models_that_are_already_there_downloads_nothing`

**C13** - The tray maps the fetch output to its state. `progress <file> 400 1000` gives tooltip `VOXIS - baixando modelos de voz (40%)`. Exit `0` starts the dictation daemon. Exit `1` shows the toast `Falha ao baixar modelos: sem conexão; tente de novo`. Exit `2` shows `Falha ao baixar modelos: hash diferente; tente de novo` (AC 7)
Proof: `TRAY_TEST the_model_download_drives_the_tooltip_and_its_outcome`

**C14** - While the models are missing or downloading, the tray does not spawn the dictation daemon, and its menu shows the disabled item `Ditado indisponível: baixando modelos (N%)` (AC 11)
Proof: `TRAY_TEST while_models_download_dictation_stays_off_and_the_menu_says_why`

### S3 - the Vulkan edition · 2 files · 10 KB · ~3k

Done: C15-C17 green on 2026-11-06 (Vulkan installer 9 MB; smoke transcribe, replace, uninstall-keeps-models PASS; C1 and C6 re-run PASS after the preinstall hook)

**C15** - The Vulkan install dir contains no file matching `cublas*.dll`. Its `meetrec.exe transcribe` on a SAPI-synthesised pt-BR sentence (`teste de transcrição do voxis`) writes a `transcript.md` containing `transcrição` (AC 12)
Proof: `SMOKE -Edition vulkan -Check transcribe`

**C16** - Installing the Vulkan installer over the CUDA install leaves exactly one HKCU uninstall entry with `DisplayName` `VOXIS`, and its install dir no longer contains `cublas*.dll` (AC 13)
Proof: `SMOKE -Edition cuda,vulkan -Check replace`

**C17** - Uninstalling leaves `%LOCALAPPDATA%\voxis\models` and its files in place (data lifecycle, Observable: uninstall)
Proof: `SMOKE -Edition vulkan -Check uninstall-keeps-models`

### S4 - downloadable from the repository · 3 files · 8 KB · ~2k

**C18** - After the run for tag `v<ver>` the draft release lists `VOXIS_<ver>_x64_cuda-setup.exe` and `VOXIS_<ver>_x64_vulkan-setup.exe`. The check script exits `1` naming any that is missing (AC 14)
Proof: `powershell -File scripts\windows\check-release-assets.ps1 -Tag v0.2.0-rc1`

**C19** - `README.md` has a `Windows` section with a link to `/releases/latest`, the two asset name patterns, `NVIDIA` beside the CUDA edition, and `1,6 GB` (AC 15)
Proof: `powershell -File scripts\windows\check-readme.ps1`

### S5 - starts with Windows · 3 files · 42 KB · ~10k

Done: C20-C21 green on 2026-11-06 (tray suite 52 passed; mechanism changed to door 11 with the owner's OK)

**C20** - Enabling autostart through the tray's autostart module writes `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` value `<name>` = the quoted exe path. Disabling removes the value (AC 16)
Proof: `TRAY_TEST enabling_autostart_writes_the_run_value_and_disabling_removes_it`

**C21** - The `Iniciar com o Windows` menu item is checked exactly when the Run value exists, and clicking it flips both (AC 16)
Proof: `TRAY_TEST the_autostart_menu_item_mirrors_the_run_value`

### S6 - meetings for someone without the owner's Google setup · 3 files · 46 KB · ~12k

Done: C22-C23 green on 2026-11-06 (meetrec suite 20 passed, tray suite 55 passed)

**C22** - With no `google-oauth-client.json`/`google-token.json`, processing ends with `state: completed`, `ata.md` in the session dir, and the process output naming `ata.md`. The tray's toast reads `Ata salva em <path>\ata.md` (AC 17)
Proof: `MEETREC_TEST without_google_credentials_the_minutes_stay_in_ata_md`
Proof: `TRAY_TEST a_meeting_kept_locally_names_its_ata_in_the_toast`

**C23** - With no Anthropic key, processing ends `completed` with `transcript.md` kept and no `ata.md`. The tray toast reads `Ata não gerada: configure anthropic_api_key em %APPDATA%\voxis\secrets.toml` (AC 18)
Proof: `MEETREC_TEST without_an_anthropic_key_the_session_completes_with_the_transcript`
Proof: `TRAY_TEST a_meeting_without_a_key_says_where_to_put_it`

## Coverage

| Set (size) | Member -> proof | Unproven |
| --- | --- | --- |
| model lookup places (4) | `VOXIS_MODELS` C7 · `%LOCALAPPDATA%\voxis\models` C7 · bench cache C7 · none found C7 (table-driven over all 4, plus the one-file-only skip) | - |
| `models fetch` exits (3) | `0` C8, C12 · `1` C10, C11 · `2` C9 | - |
| tray reaction to fetch (4) | progress line C13 · exit `0` C13 · exit `1` C13 · exit `2` C13 | - |
| editions (2) | `cuda` C1, C5, C6 · `vulkan` C15, C16 | - |
| meetrec path resolution (2 sources) | unset -> sidecar C2 · set in `tray.json` C3 | - |
| GitHub release asset statuses (2) | `200` C18 · `404` C18 (script exits `1` on a missing asset) | - |
| Landing doors (7) | 1 install mode C1, C16 · 2 sidecar C1, C2 · 3 asset names C18 · 4 NVIDIA DLLs C6 · 5 models dir and pins C8, C9 · 6 autostart (superseded by 11) C20 · 11 quoted Run value C20 · 7 CI CUDA build C18 | - |
| meeting outcome without setup (2) | no Google C22 · no Anthropic key C23 | - |
| startup config: orb page location (2 assemblies) | installed app C5 · tray spawn C4 | - |

- Claims naming an exit code, a status or a file: C1, C6, C8-C12, C18 - each proof runs the real binary or a real HTTP server, not a mock of the call
- No other check claims more than the single case its proof exercises

## Test policy

| Code | Required proofs | Coverage expectation |
| --- | --- | --- |
| Decides, reached across a boundary (model fetch over HTTP, the installed app) | one at the boundary **and** one at its own layer | the CLI exit contract against a real local server; each lookup place and each fetch outcome asserted once |
| Decides, not reached across a boundary (path resolution, tooltip/toast mapping, menu state) | one at its own layer | one asserted case per branch |
| Instrumentation (Tauri config, CI YAML, NSIS) | the smoke or release script that exercises the produced artifact | the installed file set and the release asset names |

Evidence:

- `meetrec` model lookup: 3 places plus the none case, 1 skip rule -> decides
- `models fetch`: 3 exits, a `.part` -> rename step, and a skip-when-present rule -> decides
- tray fetch handling: 4 outcomes -> decides; `settings.rs` path default: 2 branches -> decides
- closest analogue: `apps/tray/src-tauri/src/settings.rs` `laris_dir_default` (checkout vs fallback), already proven per branch at its own layer by `a_source_build_defaults_to_the_laris_venv_of_its_own_checkout` and `a_build_whose_checkout_has_no_venv_falls_back_to_code_voxis`

Cost: 16 unit proofs across 6 files plus 3 scripts. Without them, the fetch exits and the lookup order would be proven only by one smoke path.

## Swept

- validation: C8, C9 (SHA-256 against the pin), C7 (a folder with one file is skipped)
- failure modes: C10, C22, C23
- idempotency: C12
- authorization: n/a - per-user install with no elevation (C1); no new network listener (the orb server already binds `127.0.0.1`)
- concurrency: existing - `tauri-plugin-single-instance` keeps one tray, so one fetch writes a given `.part`
- data lifecycle: C17
- dependency failure: C11, C13 (exit `1` toast)
- state transitions: C22, C23 (session `completed` instead of `failed`), C14 (dictation off until models ready)
- observability: C13 (progress in the tooltip); `models fetch` progress lines go to the tray log like every other child's output

## Handoff

- S1-S6 ≈ 61k by `wc -c`/4 over the files listed per slice (settings.rs, laris.rs, lib.rs, miris.rs, toast.rs, Cargo.toml and the Windows conf for the tray; main.rs, minutes.rs, session.rs and Cargo.toml for meetrec; the workflow; README), plus about 15k for the new scripts and the `models` module. That is ~76k, under the 150k budget - one builder
- **Mechanism:** one builder
- **Plan change needed before S2:** AC 11 promised a toast when the dictation key is pressed while models download. The key is hooked by the dictation daemon, which is not running then, so nothing can see the press. C14 replaces it with a disabled menu item and no daemon. Owner OK 2026-11-06; plan AC 11 amended
- **Outward actions:** C18 needs a pushed `v0.2.0-rc1` tag, which creates a draft release on the public repo - asked for at S4. C20 writes the HKCU Run key from a test - if the permission classifier blocks it, the owner runs that test
