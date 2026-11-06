# Windows installer verification

**Verdict**: PASS
**Profile**: standard
**Diff range**: 0ce8766..8f66224 (fix range eddeecf..8f66224)
**Round**: 3 - scoped
**Verifier**: independent sub-agent (author != verifier)

Round 2 (at `eddeecf`) failed on one surviving mutant: F9, swapping `VOXIS_MODELS` and
`LOCALAPPDATA` where `models fetch` read them, survived because C24's proofs called only
`fetch_target`. The fix `8f66224` moves the two environment-variable names out of `main.rs` into a new
helper `models::fetch_target_from_env` (`models.rs:50-52`), leaves `main.rs:87` passing only a generic
`std::env::var_os` lookup, adds the proof `models_fetch_reads_the_install_folder_from_the_environment`
(`models.rs:236-251`) and names it under C24 in `checks.md`. The fix touches nothing else.

F9 re-injected where the lookup now lives (`models.rs:51`) is killed, and so are two further faults on
the same line (VOXIS_MODELS ignored, LOCALAPPDATA misspelled). The meetrec suite was re-run in full at
`8f66224`: 24 passed, 0 failed. The tray code, installer files, CI and scripts are untouched by
`eddeecf..8f66224`, so the tray suite (56 passed at `eddeecf`) and the installer smokes (at `cf30309`)
are carried.

## Binding sources

Carried from `eddeecf` (door 5 re-read there; the fix changes neither `PINS` nor plan.md). The fix touches no interface or screen, so step 1 is not re-run.

| Source | Opened | Contradiction | Uncovered |
| --- | --- | --- | --- |
| Owner's answers 2026-11-06 (recorded in plan `## Sources`): NSIS, per-user, two editions CUDA + Vulkan published in the repo | yes - plan.md `## Sources`, `## Problem` | none | - |
| `C:\Users\samde\AppData\Local\Temp\handoff-20261106-voxis-bigger-models.md` | yes (round 1) - lines 60-126 (build commands, tray.json keys, HKCU Run note) | none | - |
| plan `## Landing` door 5 (pins and models folder) | yes - plan.md:78, re-read at `eddeecf` | none - `PINS` (`meetrec/src/models.rs:17-28`) and the C24 test literals (`models.rs:208-222`, line numbers at `8f66224`) match plan.md:78 character for character | - |
| UI design | n/a - the plan names no design | - | - |

## Checks

C1-C23, C25 and C26 carried from `eddeecf` (C1, C5, C6, C15-C19 from `cf30309`). C24 verified at
`8f66224`. The MEETREC_TEST proofs were all re-run green at `8f66224` (24 passed); citations in
`meetrec/src/models.rs`, the only source file whose line numbers moved, are refreshed to `8f66224`.
TRAY_TEST results carried from the `eddeecf` run (56 passed): no tray file changed since.

| Check | Claim | Proof run | Evidence | Result |
| --- | --- | --- | --- | --- |
| C1 | `/S` non-elevated exits 0; VOXIS.exe + meetrec.exe same folder under %LOCALAPPDATA%; VOXIS.lnk in Start menu | carried from `cf30309`: `SMOKE -Edition cuda -Check install -InstallerDir <rc2>` exit 0 | `scripts/windows/installer-smoke.ps1:57`, `:78`, `:79-80`, `:83` | PASS |
| C2 | unset miris_bin/laris_bin resolve to `<dir of VOXIS.exe>\meetrec.exe` | TRAY_TEST (carried from `eddeecf`) (56 passed) - `settings::tests::an_unset_meetrec_path_runs_the_copy_beside_the_tray ... ok` | `apps/tray/src-tauri/src/settings.rs:253-254` - `assert_eq!(PathBuf::from(&cfg.miris_bin), beside_tray)` | PASS |
| C3 | a set miris_bin/laris_bin is used as written | TRAY_TEST (carried from `eddeecf`) - `settings::tests::a_meetrec_path_set_in_tray_json_wins_over_the_bundled_copy ... ok` | `apps/tray/src-tauri/src/settings.rs:263-264` | PASS |
| C4 | daemon spawned with VOXIS_FRONTEND = frontend resource dir | TRAY_TEST (carried from `eddeecf`) - `laris::tests::the_dictation_daemon_is_told_where_the_installed_orb_page_is ... ok` | `apps/tray/src-tauri/src/laris.rs:228` | PASS |
| C5 | installed tray, checkout frontend renamed: GET /overlay 200 with `<title>` | carried from `cf30309`: `SMOKE -Edition cuda -Check overlay -InstallerDir <rc2>` exit 0 | `scripts/windows/installer-smoke.ps1:107`, `:120`, `:121` | PASS |
| C6 | installed CUDA `meetrec devices --json` exits 0 with PATH = System32;Windows | carried from `cf30309`: `SMOKE -Edition cuda -Check no-toolkit -InstallerDir <rc2>` exit 0 | `scripts/windows/installer-smoke.ps1:90`, `:95` | PASS |
| C7 | lookup order VOXIS_MODELS, LOCALAPPDATA, bench; one-file folder skipped; none -> Err naming all three | MEETREC_TEST at `8f66224` (24 passed) - `models::tests::models_are_found_in_the_first_place_that_holds_both_files ... ok` | `spikes/windows-rust/meetrec/src/models.rs:266` - `assert_eq!(order, vec![env.clone(), installed.clone(), bench.clone()])`; `:271` none -> Err naming each; `:278` one-file skip; `:281` installed; `:284` env | PASS |
| C8 | matching pins: both files written, no .part, `ready <dir>`, exit 0 | MEETREC_TEST at `8f66224` - `models::tests::a_fetch_whose_hashes_match_saves_both_models ... ok` | `spikes/windows-rust/meetrec/src/models.rs:296` exit 0; `:297-298` bytes; `:299` no `.part`; `:300` `ready <dir>` | PASS |
| C9 | hash mismatch: neither final nor .part remains, exit 2 | MEETREC_TEST at `8f66224` - `models::tests::a_model_whose_hash_differs_is_deleted_and_the_fetch_exits_2 ... ok` | `spikes/windows-rust/meetrec/src/models.rs:312` exit 2; `:313-314` final and `.part` absent | PASS |
| C10 | server closes halfway: `.part` left, no final, exit 1 | MEETREC_TEST at `8f66224` - `models::tests::an_interrupted_download_leaves_only_the_part_file ... ok` | `spikes/windows-rust/meetrec/src/models.rs:326-328` | PASS |
| C11 | connection refused: exit 1, URL in stderr | MEETREC_TEST at `8f66224` - `models::tests::an_unreachable_model_server_exits_1 ... ok` | `spikes/windows-rust/meetrec/src/models.rs:341-342` | PASS |
| C12 | both present: no request, exit 0 | MEETREC_TEST at `8f66224` - `models::tests::fetching_models_that_are_already_there_downloads_nothing ... ok` | `spikes/windows-rust/meetrec/src/models.rs:358-360` | PASS |
| C13 | `progress f 400 1000` -> tooltip 40%; exit 0 starts dictation; exit 1/2 toasts | TRAY_TEST (carried from `eddeecf`) - `models::tests::the_model_download_drives_the_tooltip_and_its_outcome ... ok` | `apps/tray/src-tauri/src/models.rs:74-75`, `:77`, `:78`, `:79` | PASS |
| C14 | while missing/downloading: no daemon spawn; disabled item `Ditado indisponível: baixando modelos (N%)` | TRAY_TEST (carried from `eddeecf`) - `models::tests::while_models_download_dictation_stays_off_and_the_menu_says_why ... ok` | `apps/tray/src-tauri/src/models.rs:99-100` - `!spawned`, `!daemon.owns_running()`; `:101-104` label; disabled flag at `lib.rs:349` | PASS |
| C15 | Vulkan install has no `cublas*.dll`; its meetrec transcribes `transcrição` | carried from `cf30309`: `SMOKE -Edition vulkan -Check transcribe -InstallerDir <rc2>` exit 0 | `scripts/windows/installer-smoke.ps1:135-136`, `:154` | PASS |
| C16 | Vulkan over CUDA: one HKCU entry `VOXIS`, no `cublas*.dll` | carried from `cf30309`: `SMOKE -Edition cuda,vulkan -Check replace -InstallerDir <rc2>` exit 0 | `scripts/windows/installer-smoke.ps1:160-161`, `:163` | PASS |
| C17 | uninstall keeps `%LOCALAPPDATA%\voxis\models` | carried from `cf30309`: `SMOKE -Edition vulkan -Check uninstall-keeps-models -InstallerDir <rc2>` exit 0 | `scripts/windows/installer-smoke.ps1:173`, `:175-176` | PASS |
| C18 | draft release for the tag lists both edition assets; script exits 1 naming a missing one | carried from `cf30309`: `check-release-assets.ps1 -Tag v0.2.0-rc2` exit 0; missing-asset branch exit 1 | `scripts/windows/check-release-assets.ps1:13-16` | PASS |
| C19 | README Windows section: /releases/latest, both patterns, NVIDIA on the CUDA row, `1.6 GB` | carried from `cf30309`: `check-readme.ps1` exit 0 | `scripts/windows/check-readme.ps1:12`, `:16` | PASS |
| C20 | enable writes HKCU Run value = quoted exe path; disable removes it | TRAY_TEST (carried from `eddeecf`) - `autostart::tests::enabling_autostart_writes_the_run_value_and_disabling_removes_it ... ok` | `apps/tray/src-tauri/src/autostart.rs:157`, `:159` | PASS |
| C21 | menu item checked exactly when the Run value exists; click flips both | TRAY_TEST (carried from `eddeecf`) - `autostart::tests::the_autostart_menu_item_mirrors_the_run_value ... ok` | `apps/tray/src-tauri/src/autostart.rs:121`, `:125-129` | PASS |
| C22 | no Google creds: `completed`, `ata.md` in session dir, output names ata.md; toast `Ata salva em <path>\ata.md` | MEETREC_TEST at `8f66224` - `session::tests::without_google_credentials_the_minutes_stay_in_ata_md ... ok`; TRAY_TEST (carried from `eddeecf`) - `tests::a_meeting_kept_locally_names_its_ata_in_the_toast ... ok` | `spikes/windows-rust/meetrec/src/session.rs:322`, `:324`; `apps/tray/src-tauri/src/lib.rs:889` | PASS |
| C23 | no Anthropic key: `completed`, transcript kept, no ata.md; toast with secrets.toml path | MEETREC_TEST at `8f66224` - `session::tests::without_an_anthropic_key_the_session_completes_with_the_transcript ... ok`; TRAY_TEST (carried from `eddeecf`) - `tests::a_meeting_without_a_key_says_where_to_put_it ... ok` | `spikes/windows-rust/meetrec/src/session.rs:336-338`; `apps/tray/src-tauri/src/lib.rs:898` | PASS |
| C24 | shipped pins are door 5's literal URLs and SHA-256; with `VOXIS_MODELS` unset, `models fetch` saves into `%LOCALAPPDATA%\voxis\models` | MEETREC_TEST at `8f66224` - `models::tests::the_shipped_pins_name_the_reviewed_commits_and_hashes ... ok`; `models::tests::a_fetch_on_an_installed_machine_saves_into_local_app_data ... ok`; `models::tests::models_fetch_reads_the_install_folder_from_the_environment ... ok` | `spikes/windows-rust/meetrec/src/models.rs:208-222` - `assert_eq!(pins, vec![(name, url, sha256) x2])` over the real `PINS`; `:232` - `assert_eq!(installed, PathBuf::from(r"C:\Users\u\AppData\Local\voxis\models"))`; `:249` - `assert_eq!(from_install, PathBuf::from(r"C:\Users\u\AppData\Local\voxis\models"))` with an env lookup where only `LOCALAPPDATA` is set; `:250` - `assert_eq!(from_override, PathBuf::from(r"D:\m"))`. The variable names `models fetch` reads now live in `fetch_target_from_env` (`models.rs:51`), which `main.rs:87` calls; F9', F10, F11 killed | PASS |
| C25 | Google counts as set up only with both `google-oauth-client.json` and `google-token.json`; with only one, minutes stay in `ata.md` | MEETREC_TEST at `8f66224` - `minutes::tests::google_counts_as_set_up_only_with_both_its_files ... ok` | `spikes/windows-rust/meetrec/src/minutes.rs:332` - `assert!(!only_client.google)`; `:333` - `assert!(both.google)`; the "stays in ata.md" half rides on C22 (`google: false` -> ata.md) | PASS |
| C26 | after a failed download the menu shows the disabled item `Ditado indisponível: falha ao baixar modelos` | TRAY_TEST (carried from `eddeecf`) - `models::tests::after_a_failed_download_the_menu_says_the_models_failed ... ok` | `apps/tray/src-tauri/src/models.rs:86` - `assert_eq!(label.as_deref(), Some("Ditado indisponível: falha ao baixar modelos"))`; disabled flag read at `lib.rs:349` as in C14 | PASS |

Every named MEETREC_TEST proof was matched individually in the `8f66224` run output (24 passed,
0 failed); the C24 proof added by the fix lives in `models.rs`, which the fix changes. Tray proofs
were matched in the `eddeecf` run (56 passed).

Precision notes:

- C24 (round 2's FAIL, closed): the env-variable names and their order are now decided in
  `fetch_target_from_env` and asserted at `models.rs:249-250`. What remains in `main.rs:87` is the
  generic lookup `|k| std::env::var_os(k).map(Into::into)` and `home()`: no name, no order, no branch.
  No test runs `models_fetch()` or the CLI, so that one line of wiring is still unreached; it is the
  same level note as the `lib.rs` wiring below, not a surviving mutant (a plausible wrong edit there,
  such as swapping or misspelling a name, no longer has anything to swap). `home()` feeds only the
  `~\.voxis\models` fallback, which no check claims.
- C25: `Credentials::here()` (`minutes.rs:258-260`) now delegates to `in_dir(&config_dir(), ...)`.
  The directory it passes is asserted by no test; it is the same `config_dir()` used before the
  fix, so this is a level note, not a new surface. Carried from `eddeecf`; the fix does not touch `minutes.rs`. Not fault-injected.
- C13, C14, C21, C26 prove pure functions; the `lib.rs` wiring has no automated proof (carried
  from round 1, a level note).

## Scope

Carried from `eddeecf`. Fix range `eddeecf..8f66224` touches three files, re-traced at `8f66224`:
`checks.md` (C24 gains the new Proof line and the Done line now says meetrec 24), `main.rs` (one
line, `:87`) and `models.rs` (helper `:48-52`, test `:236-251`). Nothing outside C24 changed.

| File | Traces to |
| --- | --- |
| `.github/workflows/tray-build.yml` | Flow 1, doors 3, 7, 12; C18 (carried) |
| `.specs/features/windows-installer/checks.md` | the feature's own checks; fix adds C24-C26 and the coverage rows (menu row, Landing doors 12, one-Google-file member) |
| `.specs/features/windows-installer/plan.md` | the feature's own plan |
| `README.md` | AC 15, C19 |
| `apps/tray/src-tauri/Cargo.toml` | door 11, C20 |
| `apps/tray/src-tauri/src/autostart.rs` | Flow 6, door 11, C20, C21 |
| `apps/tray/src-tauri/src/laris.rs` | Flow 4, C4 |
| `apps/tray/src-tauri/src/lib.rs` | Flow 4-5 wiring, C13, C14, C21, C22/C23 `note_message` |
| `apps/tray/src-tauri/src/models.rs` | Flow 5 tray side, C13, C14; fix adds the C26 test only (`:82-87`) |
| `apps/tray/src-tauri/src/settings.rs` | Flow 3, C2, C3 |
| `apps/tray/src-tauri/tauri.cuda.conf.json` | door 4, C6 |
| `apps/tray/src-tauri/tauri.windows.conf.json` | doors 1, 2, 8, 10; C1, C5 |
| `apps/tray/src-tauri/windows/installer-hooks.nsh` | door 10, C16 |
| `scripts/windows/build-installer.ps1` | Flow 1, doors 2-4 |
| `scripts/windows/check-readme.ps1` | C19 proof |
| `scripts/windows/check-release-assets.ps1` | C18 proof |
| `scripts/windows/installer-smoke.ps1` | C1, C5, C6, C15, C16, C17 proof |
| `scripts/windows/meetrec-test.cmd` | MEETREC_TEST and the Gate |
| `spikes/windows-rust/Cargo.lock` | door 9 (`sha2`), `tempfile` dev-dependency |
| `spikes/windows-rust/build-stt-bench.cmd` | door 7, Flow 1 |
| `spikes/windows-rust/meetrec/Cargo.toml` | door 9, `tempfile` |
| `spikes/windows-rust/meetrec/src/main.rs` | Flow 5, C7-C12; round-2 fix replaced the inline target choice with `models::fetch_target(...)`; round-3 fix reduces it to `models::fetch_target_from_env(&|k| std::env::var_os(k)..., &home())` (`:87`), C24 |
| `spikes/windows-rust/meetrec/src/minutes.rs` | Flow 7, C22, C23; fix extracts `Credentials::in_dir` (`:262-267`) and adds the C25 test (`:323-334`) |
| `spikes/windows-rust/meetrec/src/models.rs` | Flow 5, doors 5, 9, C7-C12; `fetch_target` (`:40-46`), `fetch_target_from_env` (`:48-52`, round 3) and the C24 tests (`:204-251`) |
| `spikes/windows-rust/meetrec/src/session.rs` | Flow 7, C22, C23 |

No unplanned change. The round-3 fix is a behaviour-preserving extraction plus one test:
`fetch_target_from_env` passes `VOXIS_MODELS` then `LOCALAPPDATA` to `fetch_target`, the same names
and order `main.rs:88-89` passed at `eddeecf`.

## Coverage

The fetch-target row recomputed at `8f66224` (the only authority the fix touched); the others carried from `eddeecf`.

| Set (size) | Recomputed from | Member -> proof | Unproven |
| --- | --- | --- | --- |
| model lookup places (4) - carried | `meetrec/src/models.rs:32-62` `places`/`find` | VOXIS_MODELS C7 · LOCALAPPDATA C7 · bench C7 · none C7 · one-file skip C7 | - |
| `models fetch` exits (3) - carried | `meetrec/src/models.rs:69-157` returns | `0` C8, C12 · `1` C10, C11 · `2` C9 | - |
| fetch target (3 branches, fed from 2 env names) - recomputed at `8f66224` | `meetrec/src/models.rs:42-46` `fetch_target`, `:50-52` `fetch_target_from_env` | VOXIS_MODELS set C24 (`:233`, `:250` through the env helper) · LOCALAPPDATA -> `voxis\models` C24 (`:232`, `:249` through the env helper) · neither -> `~\.voxis\models` not claimed by any check (fallback for a machine with no LOCALAPPDATA, outside AC 8) | - |
| tray reaction to fetch (4) - carried | `tray/src/models.rs:23-46` | progress C13 · exit 0 C13 · exit 1 C13 · exit 2 C13 | - |
| tray dictation menu label (3 states) - recomputed | `tray/src/models.rs:49-55` `dictation_label` | Ready C14 (`:105`) · Downloading C14 (`:101-104`) · Failed C26 (`:86`) | - |
| editions (2) - carried | plan Flow 1 matrix | cuda C1, C5, C6 · vulkan C15, C16 | - |
| meetrec path resolution (2) - carried | `settings.rs:82-97` | unset -> sidecar C2 · set -> as written C3 | - |
| GitHub release asset statuses (2) - carried | plan `Surface` | present C18 · missing C18 | - |
| Landing doors (12) - recomputed | plan `## Landing` rows 1-12; checks.md row now says 12 | 1 C1, C16 · 2 C1, C2 · 3 C18 · 4 C6 · 5 models dir and pins C24 (real `PINS` asserted; fault F6 killed) · 6 superseded by 11 · 7 C18 · 8 C1 · 9 C8, C9 · 10 C16 · 11 C20 · 12 C18 | - |
| meeting outcome without setup (3) - recomputed | `minutes.rs:262-267`, `minutes.rs:291-316`, `session.rs:171-183` | no Google C22 · one Google file only C25 (`:332`) · no Anthropic key C23 | - |
| `note_message` toast branches (4) - carried | `lib.rs:214-223` | ata.md C22 · transcript.md C23 · other name (pre-existing test) · no file name (pre-existing fallback) | - |
| startup config: orb page location (2 assemblies) - carried | `lib.rs:666-675`, `laris.rs:214-229` | installed app C5 · tray spawn C4 | - |

Every member has a proof. Round 2's open gap (a proof one call short of the env names) is closed:
the names are now asserted through `fetch_target_from_env`.

## Test policy rows

The boundary row (it classifies `models.rs` and `main.rs`, both touched) re-judged at `8f66224`; the others carried from `eddeecf`.

| Row | Files it classifies | Required proof | Expectation met |
| --- | --- | --- | --- |
| Decides, reached across a boundary (model fetch over HTTP, the installed app) | `meetrec/src/models.rs`, `meetrec/src/main.rs` (`models fetch`), the installed app | boundary: C8-C12 against a real local HTTP server; C1, C5, C6 against the installed app · own layer: C7, C24 (real `PINS`, `fetch_target`, `fetch_target_from_env`) | yes - each lookup place, each fetch exit, each shipped pin and both env names `models fetch` reads are asserted; `main.rs:87` is now decision-free wiring (level note, see Precision notes) |
| Decides, not reached across a boundary (path resolution, tooltip/toast mapping, menu state, Google detection) | `settings.rs`, `tray/src/models.rs`, `autostart.rs`, `lib.rs` `note_message`, `minutes.rs` `Credentials::in_dir` | own layer: C2, C3, C13, C14, C21, C22/C23 tray tests, C25, C26 | yes - one asserted case per branch claimed, now including the download-error menu label (C26) and both Google-file outcomes (wording changed in round 3 only so the gate does not read the label name as a verdict) |
| Instrumentation (Tauri config, CI YAML, NSIS) - carried | `tauri.windows.conf.json`, `tauri.cuda.conf.json`, `installer-hooks.nsh`, `tray-build.yml`, `build-installer.ps1` | smoke/release scripts over the CI-built rc2 installers: C1, C5, C6, C15-C18 | yes |

## Faults injected

Round 3, verified at `8f66224`. Worktree `C:\vxwt` at `8f66224`; meetrec only, so no sidecar or
`dist` copy was needed. Built with `CARGO_TARGET_DIR=C:\vxf` through the worktree's
`scripts\windows\meetrec-test.cmd`. Real-tree `git status --porcelain` before = ` M
apps/tray/src-tauri/Cargo.toml` plus the untracked `verification.md`; identical after
`git worktree remove --force C:\vxwt` (diffed against the saved baseline: no difference). One fault at
a time, each reverted with `git checkout -- .` before the next. F1-F8 carried (F1-F4 from `cf30309`,
F5-F8 from `eddeecf`; their asserted lines did not change, only `models.rs` line numbers shifted).
Round-2 F9 is superseded by F9' at the new location of the lookup. Three faults this round: each of
the two assertions the fix added was made to fail at least once.

| Mutation | Location | Killed |
| --- | --- | --- |
| F1 (carried from `cf30309`): sidecar default `meetrec.exe` -> `meetrec-cli.exe` | `apps/tray/src-tauri/src/settings.rs:97` | yes - C2 |
| F2 (carried from `cf30309`): `spawn_dictation` guard `state != Ready` -> `state == Failed` | `apps/tray/src-tauri/src/models.rs:60` | yes - C14 |
| F3 (carried from `cf30309`): Run value written unquoted | `apps/tray/src-tauri/src/autostart.rs:26` | yes - C20 |
| F4 (carried from `cf30309`): hash mismatch no longer deletes `<name>.part` | `spikes/windows-rust/meetrec/src/models.rs:148` (line at `8f66224`) | yes - C9 |
| F5 (carried from `eddeecf`): Google detection `is_file() && ...is_file()` -> `\|\|` | `spikes/windows-rust/meetrec/src/minutes.rs:265` | yes - C25 `google_counts_as_set_up_only_with_both_its_files` FAILED at `minutes.rs:332` (`assertion failed: !only_client.google`); 22 others passed |
| F6 (carried from `eddeecf`): turbo pin SHA-256 `1fc70f774d38eb16...` -> `1fc70f774d38eb17...` | `spikes/windows-rust/meetrec/src/models.rs:21` | yes - C24 `the_shipped_pins_name_the_reviewed_commits_and_hashes` FAILED at the pins assertion (now `models.rs:208`) |
| F7 (carried from `eddeecf`): `fetch_target` LOCALAPPDATA branch drops `voxis` (`d.join("models")`) | `spikes/windows-rust/meetrec/src/models.rs:44` | yes - C24 `a_fetch_on_an_installed_machine_saves_into_local_app_data` FAILED at the installed assertion (now `models.rs:232`) |
| F8 (carried from `eddeecf`): Failed menu copy `falha ao baixar modelos` -> `falha ao obter modelos` | `apps/tray/src-tauri/src/models.rs:53` | yes - C26 `after_a_failed_download_the_menu_says_the_models_failed` FAILED at `models.rs:86` (left `Some("Ditado indisponível: falha ao obter modelos")`) |
| F9': swap the names where they now live: `fetch_target(env("LOCALAPPDATA"), env("VOXIS_MODELS"), home)` (supersedes round-2 F9 at `main.rs:88-89`, code that no longer exists) | `spikes/windows-rust/meetrec/src/models.rs:51` | yes - C24 `models_fetch_reads_the_install_folder_from_the_environment` FAILED at `models.rs:249` (left `C:\Users\u\AppData\Local`, right `...\Local\voxis\models`); 23 others passed |
| F10: helper ignores the override: `fetch_target(None, env("LOCALAPPDATA"), home)` | `spikes/windows-rust/meetrec/src/models.rs:51` | yes - C24 `models_fetch_reads_the_install_folder_from_the_environment` FAILED at `models.rs:250` (left `...\Local\voxis\models`, right `D:\m`); 23 others passed |
| F11: helper misspells the install variable: `env("LOCAL_APPDATA")` | `spikes/windows-rust/meetrec/src/models.rs:51` | yes - C24 `models_fetch_reads_the_install_folder_from_the_environment` FAILED at `models.rs:249` (left `C:\Users\u\.voxis\models`); 23 others passed |

## Gate

Verified at `8f66224`:

`cmd /c "scripts\windows\meetrec-test.cmd > %TEMP%\mt.log 2>&1"` (= `cargo test -p meetrec --release --no-default-features --features cuda` inside vcvars64, Ninja, LIBCLANG_PATH) - 24 passed, 0 failed, exit 0

Carried from `eddeecf` (no tray file in `eddeecf..8f66224`): `cargo test --release` in
`apps\tray\src-tauri` - 56 passed, 0 failed, exit 0.

Installer smokes (C1, C5, C6, C15-C17), `check-release-assets.ps1` (C18) and `check-readme.ps1` (C19)
carried from `cf30309`: no fix since touches the files they exercise.

## Ranked gaps

None fails the feature. Level notes, ranked:

1. C24 - `spikes/windows-rust/meetrec/src/main.rs:87`. `models_fetch()` and the CLI dispatch
   (`main.rs:44`) are reached by no test; the line now holds only the generic `std::env::var_os`
   lookup and `home()`, with no name, order or branch, so the round-2 swap fault has nowhere left to
   live there. A process-level `meetrec models fetch` run with `LOCALAPPDATA` set to a temp dir would
   close the last call.
2. C25 - `spikes/windows-rust/meetrec/src/minutes.rs:258-260`. `Credentials::here()` passes
   `config_dir()` to `in_dir`, and no test asserts that directory. The directory choice predates this
   feature; carried from `eddeecf`, not fault-injected.
3. C13, C14, C21, C26 - `apps/tray/src-tauri/src/lib.rs` (menu and tooltip wiring, disabled flag at
   `:349`). The proofs cover the pure functions; the wiring has no automated proof. Carried from
   `cf30309`.
