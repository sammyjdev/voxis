# Windows installer verification

**Verdict**: PASS
**Profile**: standard
**Diff range**: 0ce8766..d370e44 (fix range eeb1638..d370e44)
**Round**: 7 - scoped
**Verifier**: independent sub-agent (author != verifier)

Round 7 re-verifies fix `d370e44` ("make the no-spawn probe prove it can see a spawn") on top of round
6's FAIL at `eeb1638`. Round 6 failed only on mutant F21 (`FetchRun::start` spawns, then refuses on
`stopped`), which survived because the probe in `a_download_refused_after_quit_runs_no_process` never
wrote its marker on Windows.

The fix range touches two files. In `apps/tray/src-tauri/src/models.rs` the test is rewritten
(`:276-307`): the probe is now a script file in the tempdir (`probe.cmd` on Windows, an executable
`probe.sh` elsewhere), and a control run first starts it on a fresh `FetchRun` and asserts the marker
appears (`:298`) before the stopped run asserts it does not (`:306`). In `checks.md` the C27-C29 Done
line now reads "tray suite 61 passed". No production code changed.

The tray suite re-ran in full at `d370e44`: 61 passed, 0 failed. Re-injected F21 is now killed at
`models.rs:306`. The round-6 diagnostic (delete `run.stop()` from the test) now goes red at the same
assertion. The GitHub `linux` workflow for `d370e44` (run 42322825039) completed `success`, tray 56
passed, all four C29 proofs `... ok`, which also shows the control assertion `:298` holds on Linux, so
the `probe.sh` branch does write its marker there.

F18 stays classified under the owner's Test policy decision of 2026-11-06 (accepted wiring).

## Binding sources

Carried from `2cb2545` (round 4). The fix range touches no binding source, no screen and no
interface named in the plan, so step 1 is not re-run.

| Source | Opened | Contradiction | Uncovered |
| --- | --- | --- | --- |
| Owner's answers 2026-11-06 (recorded in plan `## Sources`): NSIS, per-user, two editions CUDA + Vulkan published in the repo | yes - plan.md `## Sources`, `## Problem` (carried) | none | - |
| `C:\Users\samde\AppData\Local\Temp\handoff-20261106-voxis-bigger-models.md` | yes (round 1) - lines 60-126 (carried) | none | - |
| plan `## Landing` door 5 (pins and models folder) | yes - plan.md:78 (carried; `PINS` unchanged through `d370e44`) | none | - |
| CUDA 13.3 EULA, docs.nvidia.com/cuda/archive/13.3.1/eula/index.html (plan open question 3) | yes - fetched at round 4 (carried from `2cb2545`) | none - §2.6 Attachment A lists "CUDA BLAS Library" `cublas.dll, cublasLt.dll` with version-suffixed variations allowed; §1.1.2 conditions as in the 13.5 text | - |
| LunarG SHA for Vulkan SDK 1.4.363.1 | yes - fetched at round 4 (carried from `2cb2545`; `tray-build.yml` untouched since) | none - `c02932bd...0a6a` equals `.github/workflows/tray-build.yml:102` | - |
| UI design | n/a - the plan names no design | - | - |

## Checks

C1, C5, C6 and C15-C19 are carried from `cf30309` (installer smokes and release/README scripts). The
MEETREC_TEST proofs (C7-C12, C22-C25, C27) are carried from `2cb2545`: `git diff --stat
eeb1638..d370e44` lists only `checks.md` and `apps/tray/src-tauri/src/models.rs`, and no meetrec file
changed since `2cb2545`. Every TRAY_TEST proof re-ran at `d370e44` (`cargo test --release` in
`apps\tray\src-tauri`, PATH refreshed, 61 passed, 0 failed, exit 0). Citations in `tray/src/models.rs`
above `:275` are unchanged; the tests after the rewritten one moved down 15 lines.

| Check | Claim | Proof run | Evidence | Result |
| --- | --- | --- | --- | --- |
| C1 | `/S` non-elevated exits 0; VOXIS.exe + meetrec.exe same folder under %LOCALAPPDATA%; VOXIS.lnk in Start menu | carried from `cf30309`: `SMOKE -Edition cuda -Check install -InstallerDir <rc2>` exit 0 | `scripts/windows/installer-smoke.ps1:57`, `:78`, `:79-80`, `:83` | PASS |
| C2 | unset miris_bin/laris_bin resolve to `<dir of VOXIS.exe>\meetrec.exe` | TRAY_TEST at `d370e44` (61 passed) - `settings::tests::an_unset_meetrec_path_runs_the_copy_beside_the_tray ... ok` | `apps/tray/src-tauri/src/settings.rs:253-254` - `assert_eq!(PathBuf::from(&cfg.miris_bin), beside_tray)` | PASS |
| C3 | a set miris_bin/laris_bin is used as written | TRAY_TEST at `d370e44` - `settings::tests::a_meetrec_path_set_in_tray_json_wins_over_the_bundled_copy ... ok` | `apps/tray/src-tauri/src/settings.rs:263-264` | PASS |
| C4 | daemon spawned with VOXIS_FRONTEND = frontend resource dir | TRAY_TEST at `d370e44` - `laris::tests::the_dictation_daemon_is_told_where_the_installed_orb_page_is ... ok` | `apps/tray/src-tauri/src/laris.rs:228` | PASS |
| C5 | installed tray, checkout frontend renamed: GET /overlay 200 with `<title>` | carried from `cf30309`: `SMOKE -Edition cuda -Check overlay -InstallerDir <rc2>` exit 0 | `scripts/windows/installer-smoke.ps1:107`, `:120`, `:121` | PASS |
| C6 | installed CUDA `meetrec devices --json` exits 0 with PATH = System32;Windows | carried from `cf30309`: `SMOKE -Edition cuda -Check no-toolkit -InstallerDir <rc2>` exit 0 | `scripts/windows/installer-smoke.ps1:90`, `:95` | PASS |
| C7 | lookup order VOXIS_MODELS, LOCALAPPDATA, bench; one-file folder skipped; none -> Err naming all three | carried from `2cb2545`: MEETREC_TEST (25 passed) - `models::tests::models_are_found_in_the_first_place_that_holds_both_files ... ok` | `spikes/windows-rust/meetrec/src/models.rs:279` - `assert_eq!(order, vec![env.clone(), installed.clone(), bench.clone()])`; `:284`, `:291`, `:294`, `:297` | PASS |
| C8 | matching pins: both files written, no .part, `ready <dir>`, exit 0 | carried from `2cb2545` - `models::tests::a_fetch_whose_hashes_match_saves_both_models ... ok` | `spikes/windows-rust/meetrec/src/models.rs:309` exit 0; `:310-311` bytes; `:312` no `.part`; `:313` `ready <dir>` | PASS |
| C9 | hash mismatch: neither final nor .part remains, exit 2 | carried from `2cb2545` - `models::tests::a_model_whose_hash_differs_is_deleted_and_the_fetch_exits_2 ... ok` | `spikes/windows-rust/meetrec/src/models.rs:325` exit 2; `:326-327` | PASS |
| C10 | server closes halfway: `.part` left, no final, exit 1 | carried from `2cb2545` - `models::tests::an_interrupted_download_leaves_only_the_part_file ... ok` | `spikes/windows-rust/meetrec/src/models.rs:368-370` | PASS |
| C11 | connection refused: exit 1, URL in stderr | carried from `2cb2545` - `models::tests::an_unreachable_model_server_exits_1 ... ok` | `spikes/windows-rust/meetrec/src/models.rs:383-384` | PASS |
| C12 | both present: no request, exit 0 | carried from `2cb2545` - `models::tests::fetching_models_that_are_already_there_downloads_nothing ... ok` | `spikes/windows-rust/meetrec/src/models.rs:400-402` | PASS |
| C13 | `progress f 400 1000` -> tooltip 40%; exit 0 starts dictation; exit 1/2 toasts | TRAY_TEST at `d370e44` - `models::tests::the_model_download_drives_the_tooltip_and_its_outcome ... ok` | `apps/tray/src-tauri/src/models.rs:215-216`, `:218`, `:219`, `:220` | PASS |
| C14 | while missing/downloading: no daemon spawn; disabled item `Ditado indisponível: baixando modelos (N%)` | TRAY_TEST at `d370e44` - `models::tests::while_models_download_dictation_stays_off_and_the_menu_says_why ... ok` | `apps/tray/src-tauri/src/models.rs:345-346` - `!spawned`, `!daemon.owns_running()`; `:347-350` label; disabled flag at `lib.rs:278` | PASS |
| C15 | Vulkan install has no `cublas*.dll`; its meetrec transcribes `transcrição` | carried from `cf30309`: `SMOKE -Edition vulkan -Check transcribe -InstallerDir <rc2>` exit 0 | `scripts/windows/installer-smoke.ps1:135-136`, `:154` | PASS |
| C16 | Vulkan over CUDA: one HKCU entry `VOXIS`, no `cublas*.dll` | carried from `cf30309`: `SMOKE -Edition cuda,vulkan -Check replace -InstallerDir <rc2>` exit 0 | `scripts/windows/installer-smoke.ps1:160-161`, `:163` | PASS |
| C17 | uninstall keeps `%LOCALAPPDATA%\voxis\models` | carried from `cf30309`: `SMOKE -Edition vulkan -Check uninstall-keeps-models -InstallerDir <rc2>` exit 0 | `scripts/windows/installer-smoke.ps1:173`, `:175-176` | PASS |
| C18 | draft release for the tag lists both edition assets; script exits 1 naming a missing one | carried from `cf30309`: `check-release-assets.ps1 -Tag v0.2.0-rc2` exit 0; missing-asset branch exit 1 | `scripts/windows/check-release-assets.ps1:13-16` | PASS |
| C19 | README Windows section: /releases/latest, both patterns, NVIDIA on the CUDA row, `1.6 GB` | carried from `cf30309`: `check-readme.ps1` exit 0 | `scripts/windows/check-readme.ps1:12`, `:16` | PASS |
| C20 | enable writes HKCU Run value = quoted exe path; disable removes it | TRAY_TEST at `d370e44` - `autostart::tests::enabling_autostart_writes_the_run_value_and_disabling_removes_it ... ok` | `apps/tray/src-tauri/src/autostart.rs:157`, `:159` | PASS |
| C21 | menu item checked exactly when the Run value exists; click flips both | TRAY_TEST at `d370e44` - `autostart::tests::the_autostart_menu_item_mirrors_the_run_value ... ok` | `apps/tray/src-tauri/src/autostart.rs:121`, `:125-129` | PASS |
| C22 | no Google creds: `completed`, `ata.md` in session dir, output names ata.md; toast `Ata salva em <path>\ata.md` | MEETREC part carried from `2cb2545` - `session::tests::without_google_credentials_the_minutes_stay_in_ata_md ... ok`; TRAY_TEST at `d370e44` - `miris::tests::a_meeting_kept_locally_names_its_ata_in_the_toast ... ok` | `spikes/windows-rust/meetrec/src/session.rs:322`, `:324`; `apps/tray/src-tauri/src/miris.rs:558` - `assert_eq!(msg, r"Ata salva em C:\Users\x\.miris\sessions\20261106-101500\ata.md")` | PASS |
| C23 | no Anthropic key: `completed`, transcript kept, no ata.md; toast with secrets.toml path | MEETREC part carried from `2cb2545`; TRAY_TEST at `d370e44` - `miris::tests::a_meeting_without_a_key_says_where_to_put_it ... ok` | `spikes/windows-rust/meetrec/src/session.rs:336-338`; `apps/tray/src-tauri/src/miris.rs:569` | PASS |
| C24 | shipped pins are door 5's literal URLs and SHA-256; with `VOXIS_MODELS` unset, `models fetch` saves into `%LOCALAPPDATA%\voxis\models` | carried from `2cb2545` - `the_shipped_pins_name_the_reviewed_commits_and_hashes`, `a_fetch_on_an_installed_machine_saves_into_local_app_data`, `models_fetch_reads_the_install_folder_from_the_environment` all `... ok` | `spikes/windows-rust/meetrec/src/models.rs:221` - `assert_eq!(pins, vec![...])`; `:245`; `:262-263` | PASS |
| C25 | Google counts as set up only with both files; with only one, minutes stay in `ata.md` | carried from `2cb2545` - `minutes::tests::google_counts_as_set_up_only_with_both_its_files ... ok` | `spikes/windows-rust/meetrec/src/minutes.rs:332` - `assert!(!only_client.google)`; `:333` - `assert!(both.google)` | PASS |
| C26 | after a failed download the menu shows the disabled item `Ditado indisponível: falha ao baixar modelos` | TRAY_TEST at `d370e44` - `models::tests::after_a_failed_download_the_menu_says_the_models_failed ... ok` | `apps/tray/src-tauri/src/models.rs:332` - `assert_eq!(label.as_deref(), Some("Ditado indisponível: falha ao baixar modelos"))`; disabled flag at `lib.rs:278` | PASS |
| C27 | a server that sends headers and part of the body then stops sending makes `models fetch` give up within the stall limit, exit `1`, no final file | carried from `2cb2545` - `models::tests::a_download_that_stops_sending_gives_up_and_exits_1 ... ok` | `spikes/windows-rust/meetrec/src/models.rs:355` - `assert_eq!(rx.recv_timeout(Duration::from_secs(10)), Ok(1))`; `:356`; F12 killed | PASS |
| C28 | after a failed download the menu offers `Baixar modelos de novo`; while downloading or ready it does not | TRAY_TEST at `d370e44` - `models::tests::only_a_failed_download_offers_to_download_again ... ok` | `apps/tray/src-tauri/src/models.rs:227` - `assert_eq!(failed, Some("Baixar modelos de novo"))`; `:228` Downloading(40) -> None; `:229` Ready -> None; F13 killed (carried). Menu row at `lib.rs:282-283`; click wiring `lib.rs:454` under the owner-accepted Test policy line | PASS |
| C29 | quitting stops a model download still running: its process ends within 5 s and does not report success, and a download asked for after quitting never starts | TRAY_TEST at `d370e44` - `quitting_the_tray_stops_its_model_download ... ok`, `a_download_asked_for_after_quit_never_starts ... ok`, `quitting_stops_a_download_still_running ... ok`, `a_download_refused_after_quit_runs_no_process ... ok` | `apps/tray/src-tauri/src/models.rs:259` - `assert!(started.elapsed() < Duration::from_secs(5))` and `:260` - `assert_ne!(code, Some(0))` after `quit(&state)` (`:256`); `:271` - `assert!(started.is_err())`, `:272` - `assert!(!run.running())`; `:323-324` direct `FetchRun::stop`; `:298` - `assert!(marker.exists(), "the probe must leave its file when it runs")` (control), `:306` - `assert!(!marker.exists())` after `run.stop()` (`:301`). F21, F22, F23 killed; diagnostic red | PASS |

Every named TRAY_TEST proof was matched individually as `... ok` in the `d370e44` run (61 passed).
The four C29 proof names exist at `models.rs:242`, `:264`, `:276`, `:310`. The rewritten one is the
whole fix diff in that file.

Precision notes:

- C29, second clause: "never starts" names an observable (no process). The proof now observes it
  through a marker that a control run shows the probe does write (`:298`), so `:306` is no longer
  constant. Closed. The 2 s sleep (`:304`) bounds how long a spawned probe has to write; on this machine
  the mutant's probe wrote within it on every run, and it is a script that only redirects `echo`.
- C29, trigger: the Quit handler's call to `models::quit` (`lib.rs:451`) has no automated proof. It is
  classified under the owner's Test policy Evidence line of 2026-11-06 as accepted wiring, not unproven.
- `checks.md:128` Done line now reads "tray suite 61 passed", matching the run. Closed.
- C27: carried from round 4 unchanged (`STALL` = 60 s unasserted).

## Scope

Verified at `d370e44` for the fix range `eeb1638..d370e44`; rows for files outside it carried from
`eeb1638`.

| File | Traces to |
| --- | --- |
| `.github/workflows/tray-build.yml` | Flow 1, doors 3, 7, 12; C18 (carried) |
| `.specs/features/windows-installer/checks.md` | the feature's own checks. Fix range: the C27-C29 Done line only (61 passed, rounds 4-6), nothing else |
| `.specs/features/windows-installer/plan.md` | the feature's own plan (carried) |
| `.specs/features/windows-installer/verification.md` | this report |
| `README.md` | AC 15, C19 (carried) |
| `apps/tray/src-tauri/Cargo.toml` | door 11, C20 (carried) |
| `apps/tray/src-tauri/src/autostart.rs` | Flow 6, door 11, C20, C21 (carried) |
| `apps/tray/src-tauri/src/laris.rs` | Flow 4, C4 (carried) |
| `apps/tray/src-tauri/src/lib.rs` | Flow 4-5 wiring, Quit handler calls `models::quit` (`:451`), C29 (carried) |
| `apps/tray/src-tauri/src/miris.rs` | C22/C23 `note_message` (carried) |
| `apps/tray/src-tauri/src/models.rs` | Flow 5 tray side, C13, C14, C26, C28, C29 (carried). Fix range: test-only, `a_download_refused_after_quit_runs_no_process` (`:276-307`), C29. No production line changed |
| `apps/tray/src-tauri/src/settings.rs` | Flow 3, C2, C3 (carried) |
| `apps/tray/src-tauri/tauri.cuda.conf.json` | door 4, C6 (carried) |
| `apps/tray/src-tauri/tauri.windows.conf.json` | doors 1, 2, 8, 10; C1, C5 (carried) |
| `apps/tray/src-tauri/windows/installer-hooks.nsh` | door 10, C16 (carried) |
| `scripts/windows/build-installer.ps1` | Flow 1, doors 2-4 (carried) |
| `scripts/windows/check-readme.ps1` | C19 proof (carried) |
| `scripts/windows/check-release-assets.ps1` | C18 proof (carried) |
| `scripts/windows/installer-smoke.ps1` | C1, C5, C6, C15, C16, C17 proof (carried) |
| `scripts/windows/meetrec-test.cmd` | MEETREC_TEST and the Gate (carried) |
| `spikes/windows-rust/Cargo.lock` | door 9 (`sha2`), `tempfile` (carried) |
| `spikes/windows-rust/build-stt-bench.cmd` | door 7, Flow 1 (carried) |
| `spikes/windows-rust/meetrec/Cargo.toml` | door 9, `tempfile` (carried) |
| `spikes/windows-rust/meetrec/src/main.rs` | Flow 5, C7-C12, C24 (carried) |
| `spikes/windows-rust/meetrec/src/minutes.rs` | Flow 7, C22, C23, C25 (carried) |
| `spikes/windows-rust/meetrec/src/models.rs` | Flow 5, doors 5, 9, C7-C12, C24, C27 (carried) |
| `spikes/windows-rust/meetrec/src/session.rs` | Flow 7, C22, C23 (carried) |

No unplanned change. The fix range touches only the C29 test and the Done line.

## Coverage

The "fetch lifecycle on quit" row was recomputed at `d370e44`, the only row whose authority the fix
touched. The other rows are carried from `eeb1638`.

| Set (size) | Recomputed from | Member -> proof | Unproven |
| --- | --- | --- | --- |
| model lookup places (4) - carried | `meetrec/src/models.rs:32-62` | VOXIS_MODELS C7 · LOCALAPPDATA C7 · bench C7 · none C7 · one-file skip C7 | - |
| `models fetch` exits (3) - carried | `meetrec/src/models.rs:77-170` | `0` C8, C12 · `1` C10, C11, C27 · `2` C9 | - |
| download failure causes leading to exit `1` (3) - carried | `meetrec/src/models.rs:103-130` | refused C11 · closed mid-body C10 · stalled with connection open C27 | - |
| fetch target (3 branches) - carried | `meetrec/src/models.rs:42-52` | VOXIS_MODELS C24 · LOCALAPPDATA C24 · neither -> `~\.voxis\models` not claimed (outside AC 8) | - |
| tray reaction to fetch (4) - carried | `tray/src/models.rs` `progress_pct`, `outcome` | progress C13 · exit 0 C13 · exit 1 C13 · exit 2 C13 | - |
| tray dictation menu rows by model state (3 states x 2 rows) - carried | `tray/src/models.rs` `dictation_label`, `retry_label` (`:129-131`) | label: Ready C14 · Downloading C14 · Failed C26 · retry row: Failed shown C28 (`:227`) · Downloading hidden C28 (`:228`) · Ready hidden C28 (`:229`) | - |
| fetch lifecycle on quit (5 steps) - recomputed | claim C29 and `d370e44`: Quit handler (`lib.rs:451`) -> `models::quit` (`models.rs:148-151`) -> `FetchRun::stop` sets `stopped` and kills under the lock (`:188-194`); a later `start` is refused before spawning (`:166-169`); `wait` reports the exit (`:201-204`) | Quit handler calls `quit` - owner-accepted wiring (Test policy Evidence, 2026-11-06), F18 · `quit` stops the fetch C29 (`:259-260`, F17 carried) · `stop` kills within 5 s, not success C29 (`:323-324`, F14 carried, F23 carried) · `stop` sets `stopped` and `start` returns Err with an empty slot C29 (`:271-272`, F19 carried, F22 carried) · no process spawned after stop C29 (`:306` with control `:298`, F21 killed at `d370e44`) | - |
| editions (2) - carried | plan Flow 1 matrix | cuda C1, C5, C6 · vulkan C15, C16 | - |
| meetrec path resolution (2) - carried | `settings.rs:82-97` | unset -> sidecar C2 · set -> as written C3 | - |
| GitHub release asset statuses (2) - carried | plan `Surface` | present C18 · missing C18 | - |
| Landing doors (12) - carried | plan `## Landing` rows 1-12, open question 3 | 1 C1, C16 · 2 C1, C2 · 3 C18 · 4 C6 · 5 C24 · 6 superseded by 11 · 7 C18 · 8 C1 · 9 C8, C9 · 10 C16 · 11 C20 · 12 C18 | - |
| meeting outcome without setup (3) - carried | `minutes.rs:262-267`, `session.rs:171-183` | no Google C22 · one Google file C25 · no Anthropic key C23 | - |
| `note_message` toast branches (4) - carried | `tray/src/miris.rs:325-334` | ata.md C22 · transcript.md C23 · other name (pre-existing test) · no file name (pre-existing fallback) | - |
| CI job permissions (3 jobs + default) - carried, read directly | `.github/workflows/tray-build.yml` | default `read` `:11` · `build` write `:20` · `windows` inherits read · `windows-release` write `:144` | - |
| startup config: orb page location (2 assemblies) - carried | `lib.rs`, `laris.rs:214-229` | installed app C5 · tray spawn C4 | - |
| tray menu click handlers (3) - carried from `eeb1638` | `lib.rs` `handle_menu` (`:435`) | autostart `:441` -> `autostart::toggle` (C21 at its own layer) · quit `:451` -> `models::quit` (C29 at its own layer) · retry `:454` -> `models::retry` (none at its own layer; `retry` `models.rs:134-144` is not exercised). All three calls are owner-accepted wiring, F18 | - |

## Test policy rows

Re-judged at `d370e44`: the row that was unmet at round 6 and classifies `tray/src/models.rs`
(touched), plus the accepted Evidence line. The other rows are carried from `eeb1638`.

| Row | Files it classifies | Required proof | Expectation met |
| --- | --- | --- | --- |
| Decides, reached across a boundary (model fetch over HTTP, the installed app) | `meetrec/src/models.rs`, `meetrec/src/main.rs`, the installed app | boundary: C8-C12, C27 against a real local server; C1, C5, C6 against the installed app · own layer: C7, C24 | yes - carried from `2cb2545` |
| Decides, not reached across a boundary (path resolution, tooltip/toast mapping, menu state, Google detection, fetch lifecycle) | `settings.rs`, `tray/src/models.rs` (incl. `retry_label`, `quit`, `FetchRun`), `autostart.rs`, `miris.rs` `note_message`, `minutes.rs` | own layer: C2, C3, C13, C14, C21, C22/C23, C25, C26, C28, C29 | yes - `FetchRun::start`'s refused branch now has asserted cases for its return value and slot (`:271-272`, F19, F22) and for what it must not do, spawn (`:306`, with the control `:298`; F21 killed at `d370e44`) |
| Evidence: tray menu click handlers, wiring (owner's choice 2026-11-06, after round 5 mutant F18) | `lib.rs` `handle_menu` `:441`, `:451`, `:454` | none, by the owner's decision | accepted - wiring left without an automated proof, as the owner recorded. F18 (Quit bypasses `models::quit`) survives by construction. Reported, counted as accepted |
| Instrumentation (Tauri config, CI YAML, NSIS) | `tauri.windows.conf.json`, `tauri.cuda.conf.json`, `installer-hooks.nsh`, `tray-build.yml`, `build-installer.ps1` | smoke/release scripts over the CI-built installers: C1, C5, C6, C15-C18 | yes - carried from `2cb2545` (smokes from `cf30309`) |

## Faults injected

Round 7, verified at `d370e44`. Worktree `C:\vxwt` at `d370e44` (`git worktree add C:\vxwt d370e44`),
with the gitignored `apps\tray\dist` copied in. Tray built with `CARGO_TARGET_DIR=C:\vxg` and
`cargo test --release --lib` from `C:\vxwt\apps\tray\src-tauri`, PATH refreshed. Every edit used an
absolute path under `C:\vxwt`, and `git -C C:\vxwt diff` was shown before each run. One change at a
time, reverted with `git checkout -- .` before the next. At the end the worktree was removed with
`git worktree remove --force C:\vxwt`; `git worktree list` shows only the real tree and `C:\vxwt` no
longer exists. The real tree's `git status --porcelain` before and after (` M
.specs/features/windows-installer/verification.md`, ` M apps/tray/src-tauri/Cargo.toml`) was compared:
identical.

Diagnostic (not a fault, the round-6 check): with no production change, `run.stop();` was deleted from
`a_download_refused_after_quit_runs_no_process`. `start` then spawns the probe, and the test FAILED at
`models.rs:305:9` (`assertion failed: !marker.exists()`, line shifted by the deletion; `0 passed; 1
failed`). The probe now writes its marker on Windows, so the assertion can fail.

F1-F20, F22 and F23 are carried from earlier rounds. The lines they mutate did not change in the fix
range, which is test-only, and the assertions that killed them (`:260`, `:270-271`, `:309` now `:324`)
are unchanged in text.

| Mutation | Location | Killed |
| --- | --- | --- |
| F1 (carried from `cf30309`): sidecar default `meetrec.exe` -> `meetrec-cli.exe` | `apps/tray/src-tauri/src/settings.rs:97` | yes - C2 |
| F2 (carried from `cf30309`): `spawn_dictation` guard `state != Ready` -> `state == Failed` | `apps/tray/src-tauri/src/models.rs` `spawn_dictation` | yes - C14 |
| F3 (carried from `cf30309`): Run value written unquoted | `apps/tray/src-tauri/src/autostart.rs:26` | yes - C20 |
| F4 (carried from `cf30309`): hash mismatch no longer deletes `<name>.part` | `spikes/windows-rust/meetrec/src/models.rs` `download` | yes - C9 |
| F5 (carried from `eddeecf`): Google detection `&&` -> `\|\|` | `spikes/windows-rust/meetrec/src/minutes.rs:265` | yes - C25 |
| F6 (carried from `eddeecf`): turbo pin SHA-256 one digit changed | `spikes/windows-rust/meetrec/src/models.rs:21` | yes - C24 |
| F7 (carried from `eddeecf`): LOCALAPPDATA branch drops `voxis` | `spikes/windows-rust/meetrec/src/models.rs:44` | yes - C24 |
| F8 (carried from `eddeecf`): Failed menu copy `baixar` -> `obter` | `apps/tray/src-tauri/src/models.rs:53` | yes - C26 |
| F9' (carried from `8f66224`): swap the env names in `fetch_target_from_env` | `spikes/windows-rust/meetrec/src/models.rs:51` | yes - C24 |
| F10 (carried from `8f66224`): helper ignores `VOXIS_MODELS` | `spikes/windows-rust/meetrec/src/models.rs:51` | yes - C24 |
| F11 (carried from `8f66224`): helper misspells `LOCALAPPDATA` | `spikes/windows-rust/meetrec/src/models.rs:51` | yes - C24 |
| F12 (carried from `2cb2545`): stall limit not applied to reads | `spikes/windows-rust/meetrec/src/models.rs:107` | yes - C27 |
| F13 (carried from `2cb2545`): retry row also shown while downloading | `apps/tray/src-tauri/src/models.rs:130` | yes - C28 |
| F14 (carried from `2cb2545`): `FetchRun::stop` does not kill | `apps/tray/src-tauri/src/models.rs:192` | yes - C29 `quitting_stops_a_download_still_running` |
| F15 (round 4): Quit handler deletes `state.fetch.stop();` | `apps/tray/src-tauri/src/lib.rs:451` at `2cb2545` | superseded - re-injected against the new code as F17 and F18 |
| F16 (carried from `2cb2545`): moved `note_message` copy `Ata salva em` -> `Ata salva:` | `apps/tray/src-tauri/src/miris.rs:327` | yes - C22 |
| F17 (carried from `7692c7c`): `models::quit` no longer stops the fetch | `apps/tray/src-tauri/src/models.rs:149` | yes - C29 `quitting_the_tray_stops_its_model_download` |
| F18 (carried from `7692c7c`): Quit handler bypasses `models::quit` and stops only the daemon | `apps/tray/src-tauri/src/lib.rs:451` | accepted - owner's Test policy decision of 2026-11-06 (handle_menu wiring, no automated proof). Not re-run; survives by construction |
| F19 (carried from `7692c7c`): `start` ignores `stopped` | `apps/tray/src-tauri/src/models.rs:167` | yes - C29 `a_download_asked_for_after_quit_never_starts` |
| F20 (carried from `7692c7c`): `stop` sets the flag before taking the lock | `apps/tray/src-tauri/src/models.rs:189-190` | equivalent - argued in round 5: `start` holds the lock across check, spawn and store, so no interleaving differs |
| F21 (re-injected at `d370e44`): `start` refuses only after spawning (the `stopped` check moved below `cmd.spawn()?`; the child is dropped, not killed) | `apps/tray/src-tauri/src/models.rs:167-177` | yes - C29 `a_download_refused_after_quit_runs_no_process` FAILED at `models.rs:306:9` (`assertion failed: !marker.exists()`); 60 others passed |
| F22 (carried from `eeb1638`): `stop` no longer sets `stopped` | `apps/tray/src-tauri/src/models.rs:190` | yes - C29 `a_download_asked_for_after_quit_never_starts` |
| F23 (carried from `eeb1638`): `wait` reports exit 0 for any finished child | `apps/tray/src-tauri/src/models.rs:203` | yes - C29 `quitting_stops_a_download_still_running`, `quitting_the_tray_stops_its_model_download` |

## Gate

Verified at `d370e44`:

- `cargo test --release` in `apps\tray\src-tauri` (PATH refreshed from Machine + User) - 61 passed,
  0 failed, exit 0. Includes `quitting_the_tray_stops_its_model_download`,
  `a_download_asked_for_after_quit_never_starts`, `quitting_stops_a_download_still_running` and
  `a_download_refused_after_quit_runs_no_process`
- GitHub `linux` workflow, run 42322825039 for head `d370e447f96af680c3b3f0957c73a815d3d52b13`:
  completed `success`; tray `cargo test` 56 passed, 0 failed, all four C29 proofs `... ok`

Carried from `2cb2545`: `scripts\windows\meetrec-test.cmd` - 25 passed, 0 failed, exit 0 (no meetrec
file changed in `2cb2545..d370e44`). Installer smokes (C1, C5, C6, C15-C17), `check-release-assets.ps1`
(C18) and `check-readme.ps1` (C19) carried from `cf30309`.

## Ranked gaps

None fail the feature. Remaining notes, ranked:

1. Accepted, not unproven: `apps/tray/src-tauri/src/lib.rs:451` (Quit -> `models::quit`, F18), `:454`
   (retry -> `models::retry`), `:441` (autostart -> `autostart::toggle`). These are `handle_menu`
   wiring with no automated proof, by the owner's Test policy decision of 2026-11-06. A regression
   there passes every test.
2. C28 - `apps/tray/src-tauri/src/models.rs:134-144` (`retry`) and `lib.rs:282-283` (retry row pushed).
   Carried: `retry` itself is not exercised at its own layer, so the accepted wiring at `lib.rs:454`
   calls a function nothing proves.
3. C29 - `apps/tray/src-tauri/src/models.rs:304`. The no-spawn proof relies on a fixed 2 s sleep for a
   spawned probe to write its marker. It killed F21 here, but on a very slow machine a wrongly spawned
   probe could write late and the test would pass. Minor.
4. C29 - `apps/tray/src-tauri/src/models.rs:78-83`. Carried observation: when Quit refuses a pending
   start, `fetch_models` sets `Failed` and toasts "Falha ao baixar modelos: sem conexão; tente de novo"
   while the tray exits. Minor.
5. C27 - `spikes/windows-rust/meetrec/src/models.rs:67`, `:73`. Carried: the shipped stall limit
   (60 s) and the `fetch` -> `fetch_with(STALL)` wiring are not asserted (precision gap).
6. Carried level notes: `meetrec/src/main.rs:87` (C24 CLI wiring), `meetrec/src/minutes.rs:258-260`
   (C25 `config_dir()`), `lib.rs` menu and tooltip wiring for C13, C14, C26 (disabled flag `:278`).
