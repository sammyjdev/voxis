# Windows port completion checks

Build: `MSYS2_ARG_CONV_EXCL='*' cmd /c 'scripts\windows\meetrec-test.cmd --no-run'`; `cargo build --release --manifest-path apps/tray/src-tauri/Cargo.toml`; `npm --prefix apps/tray run build`
Run native: `MSYS2_ARG_CONV_EXCL='*' cmd /c 'scripts\windows\meetrec-test.cmd'`
Run tray: `cargo test --release --manifest-path apps/tray/src-tauri/Cargo.toml`
Run frontend: `node --test apps/tray/tests/settings.test.ts`
Run bench: `uv run --no-project --with jiwer==4.0.0 --with num2words==0.5.14 --with numpy==2.3.4 --with soundfile==0.13.1 --with psutil==7.1.3 --with pytest==8.4.2 python -m pytest scripts/stt_bench -v`
Run contract: `bash scripts/windows/check-windows-contract.sh`
Plan: `.specs/features/windows-port/plan.md`

Check numbers equal the plan's acceptance criteria numbers (C<n> proves AC <n>). Proofs marked
`walk` are acceptance-matrix rows (plan M1-M17) or reviewer steps that run outside the author's
session: Vera runs them inside the Execution gate on the candidate bytes, Rita reviews the diff.
A unit proof at a pure-function seam is marked unit-proven, not forced on the host (AC 92).

92 checks in 11 slices · 8 one-way doors · Q3, Q4 and the candidate tag name open, none blocks the build

## Checks

### S1 - The overlay server serves only its frontend (F17)

**C1** - `GET /static/<path>` under the canonical frontend root answers 200 with the file's bytes, at the root and in a subfolder (AC 1)
Proof: run native | dictate::tests::a_file_under_the_frontend_root_is_served ... ok
Proof: run native | dictate::tests::a_file_in_a_subfolder_of_the_frontend_root_is_served ... ok

**C2** - Absolute, drive-prefixed, rooted, UNC, reserved-device, alternate-stream and `..` paths, percent-decoded and split on `/` and `\`, answer 400 `bad path` and open no file; the sentinel outside the root is never returned (AC 2)
Proof: run native | dictate::tests::unsafe_static_paths_are_refused_with_400_bad_path ... ok
Proof: run native | dictate::tests::a_sentinel_outside_the_root_is_never_served_over_http ... ok
Proof: walk - matrix M12 on the candidate installer

**C3** - A relative path that resolves through a junction inside the root to a target outside it answers 404 with no bytes (AC 3)
Proof: run native | dictate::tests::a_junction_inside_the_root_pointing_outside_answers_404 ... ok
Proof: walk - matrix M16 on the candidate installer

**C4** - With `VOXIS_FRONTEND` naming a missing folder, `/overlay` and `/static/*` answer 404 (AC 4)
Proof: run native | dictate::tests::a_missing_frontend_folder_answers_404_for_overlay_and_static ... ok

**C5** - `GET /overlay` without a token answers 200 with the page (AC 5)
Proof: run native | dictate::tests::the_overlay_page_is_served_without_a_token ... ok

**C6** - `GET` and `POST /theme` without `token=<token>` answer 403 and `orb-theme.json` stays byte-identical; with the token a valid update answers 200 and an invalid one 422 (AC 6, Surface)
Proof: run native | dictate::tests::theme_requests_without_the_token_get_403_and_leave_the_theme_file_unchanged ... ok
Proof: run native | dictate::tests::a_theme_update_with_the_token_answers_200_and_an_invalid_one_422 ... ok

### S2 - No recording is ever overwritten (F18)

**C7** - `meetrec meeting` reserves its folder with a create-new call that fails on an existing path (AC 7)
Proof: run native | tests::the_meeting_folder_is_reserved_with_a_create_new_call ... ok

**C8** - A second meeting in the same minute gets `<stamp>_2` and the first `mic.wav` hash is unchanged; the 100th collision exits 1 with `no free session folder` (AC 8)
Proof: run native | tests::a_second_meeting_in_the_same_minute_gets_its_own_folder_and_keeps_the_first ... ok
Proof: run native | tests::after_99_folders_in_one_minute_the_meeting_refuses_with_no_free_session_folder ... ok
Proof: walk - matrix M13

**C9** - `meetrec record <dir>` refuses a folder holding `mic.wav` or `system.wav` before any writer opens, files byte-identical; a missing or empty folder is used (AC 9)
Proof: run native | tests::recording_into_a_folder_that_already_holds_a_track_is_refused_and_leaves_it_unchanged ... ok
Proof: run native | tests::recording_into_a_missing_or_empty_folder_is_allowed ... ok

**C10** - `start` fails with exit 1 when `sessions/<id>` already exists and never reuses it (AC 10)
Proof: run native | session::tests::start_refuses_a_session_folder_that_already_exists ... ok

**C11** - `recover` changes only the header size fields of a zero-size WAV, sample bytes identical; a valid WAV stays byte-identical (AC 11)
Proof: run native | session::tests::recover_fixes_only_the_header_sizes_and_keeps_every_sample_byte ... ok
Proof: run native | session::tests::recover_leaves_a_valid_wav_byte_identical ... ok

### S3 - Sessions recover and never block a new recording (F19, F23)

**C12** - `start` with the newest active session owned by a live owner exits 1 with `a session is already active: <dir>` and creates nothing (AC 12)
Proof: run native | session::tests::start_refuses_while_the_newest_active_session_has_a_live_owner ... ok

**C13** - A dead recorder in `starting`, `recording` or `stopping` becomes `interrupted` with `owner_died`, files kept, and the new session starts; a pid reused by another program is dead (AC 13)
Proof: run native | session::tests::a_dead_recorder_is_marked_interrupted_with_owner_died_and_a_new_session_starts ... ok
Proof: run native | session::tests::a_pid_reused_by_another_program_is_not_a_live_owner ... ok
Proof: run native | session::tests::a_session_whose_recorder_exited_does_not_block_a_new_start_under_the_system_owner_check ... ok
Proof: walk - matrix M7

**C14** - A dead pipeline in `processing` becomes `captured`, files kept, and `process <id>` is offered (AC 14)
Proof: run native | session::tests::a_dead_pipeline_puts_its_session_back_to_captured ... ok
Proof: walk - matrix M8

**C15** - The tray shows a live recorder under `C:\Users\Sam Doe\...` as Recording; a reused pid with a different argv is not live (AC 15)
Proof: run tray | miris::tests::a_live_recorder_under_a_profile_with_spaces_shows_recording ... ok
Proof: run tray | miris::tests::a_reused_pid_with_a_different_argv_is_not_live ... ok
Proof: walk - matrix M9

**C16** - A recorder that exits before `recording` leaves state `failed` with `recorder_error`, exit 1, no session in `starting` (AC 16)
Proof: run native | session::tests::a_recorder_that_exits_before_recording_fails_the_session_with_recorder_error ... ok

**C17** - `stop` without `captured` within 60 s ends the verified recorder and writes `interrupted` with `recorder_timeout`, files recoverable; a recorder that cannot be ended keeps `stopping` and exits 1 `recorder did not stop; owner still live` (AC 17)
Proof: run native | session::tests::stop_ends_a_recorder_that_never_finishes_and_marks_it_interrupted ... ok
Proof: run native | session::tests::stop_keeps_stopping_when_the_recorder_cannot_be_ended ... ok
Proof: run native | session::tests::stop_keeps_the_terminal_failure_the_recorder_wrote_after_the_stop_request ... ok

**C18** - A WAV write failure finalizes the headers of the data written, sets `failed` with `disk_write_error`, and keeps both files (AC 18, unit-proven)
Proof: run native | tests::a_failed_wav_write_finalizes_the_header_of_the_samples_written ... ok
Proof: run native | session::tests::a_disk_write_error_fails_the_session_and_keeps_both_files ... ok
Proof: run native | tests::a_full_disk_under_the_buffered_writer_leaves_a_wav_that_counts_the_samples_on_disk ... ok
Proof: run native | tests::a_half_written_sample_at_the_end_is_left_out_of_the_header ... ok
Proof: run native | tests::a_disk_that_took_less_than_a_header_reports_the_repair_failure_with_the_write_error ... ok
Proof: run native | tests::a_track_that_cannot_be_created_ends_in_an_error_not_a_panic ... ok

**C19** - Nothing deletes, moves or compresses a file in a session, `meetings/` or log folder on its own (AC 19)
Proof: run contract | PASS: no_automatic_retention
Proof: run tray | laris::tests::a_windows_run_is_appended_to_the_log_and_nothing_is_moved_or_replaced ... ok
Proof: run tray | tests::an_exit_is_classified_from_the_latest_run_not_from_an_older_one_in_the_appended_log ... ok
Proof: run tray | laris::tests::macos_and_linux_keep_one_generation_of_the_log ... ok

### S4 - Settings and configuration tell the truth (F20, F22, legacy controls)

**C20** - Native commands read `config.toml` as TOML and return the tray's value for `mic`, `speaker` and `minutes_model`: commented, single-quoted, escaped, nested-table and empty examples (AC 20)
Proof: run native | tests::config_toml_values_are_read_as_toml_like_the_tray ... ok

**C21** - An invalid `config.toml` prints `config.toml invalid: <message>` and the defaults are used without exiting (AC 21)
Proof: run native | tests::an_invalid_config_toml_is_reported_and_the_defaults_are_used ... ok

**C22** - On Windows the key list is exactly `f5` to `f15` and preselects the daemon's key, `f13` when none or unsupported (AC 22)
Proof: run tray | commands::tests::windows_offers_exactly_f5_to_f15_for_dictation ... ok
Proof: run frontend | the key list is the platform's and preselects the key the daemon uses
Proof: run native | dictate::tests::the_daemon_uses_f13_when_the_saved_key_is_unsupported ... ok

**C23** - On Windows `save_laris_settings` with a `DICTATE_KEY` outside `f5`..`f15` errors naming the key and writes nothing; `f13` saves (AC 23)
Proof: run tray | commands::tests::windows_rejects_a_dictate_key_outside_f5_to_f15_and_writes_nothing ... ok
Proof: run tray | commands::tests::windows_saves_f13 ... ok

**C24** - A saved `cmd_r` shows `Tecla cmd_r não é suportada no Windows; usando F13` and the file stays byte-identical until a save (AC 24)
Proof: run frontend | a saved unsupported key shows the F13 notice
Proof: walk - matrix M5, file hash before and after opening the window

**C25** - On Windows the Vault, Comando LLM, Categorias, Modelo STT and Redução de ruído controls are disabled with `Não disponível no Windows`, the Ditado save omits `STT_MODEL` and `NOISE_REDUCTION`, and their saved values stay identical (AC 25)
Proof: run frontend | legacy controls are disabled with the Windows notice
Proof: run frontend | the Windows form leaves STT_MODEL and NOISE_REDUCTION out of the save
Proof: run tray | commands::tests::saving_f13_from_the_windows_form_keeps_stt_model_and_noise_reduction ... ok

**C26** - On Windows a save carrying `STT_MODEL` or `NOISE_REDUCTION`, or `save_miris_config`, returns `unsupported on Windows: <key>` and writes nothing; `DICTATE_KEY`, `ASSISTANT_MODE` and `INITIAL_PROMPT` save (AC 26)
Proof: run tray | commands::tests::windows_refuses_stt_model_or_noise_reduction_from_any_caller ... ok
Proof: run tray | commands::tests::windows_refuses_the_meetings_config_save ... ok
Proof: run tray | commands::tests::windows_accepts_dictate_key_assistant_mode_and_initial_prompt ... ok

**C27** - The next `dictate` start applies `ASSISTANT_MODE` (`always_on` starts hands-free) and `INITIAL_PROMPT` (empty disables priming) (AC 27)
Proof: run native | dictate::tests::the_next_dictate_start_applies_assistant_mode_and_initial_prompt ... ok

**C28** - The saved mic and speaker are used by the next meeting and dictation; an absent named device falls back to the default with a warning line the recorder log or the dictation log keeps (AC 28)
Proof: run native | tests::a_chosen_device_is_used_and_an_absent_one_falls_back_with_a_warning ... ok
Proof: walk - matrix M6 and M3 with a chosen device

### S5 - Provider calls end and keep the data (F21)

**C29** - Every Anthropic, OAuth, Drive list/create and Drive upload call carries its deadline: connect 10 s; 300 s; 30 s; 30 s; 120 s (AC 29)
Proof: run native | minutes::tests::every_provider_call_carries_its_deadline ... ok
Proof: run native | minutes::tests::a_stalled_anthropic_call_ends_at_its_deadline ... ok

**C30** - A call past its deadline errors naming provider and deadline; the session returns to `captured` with `provider_timeout`, `transcript.md` and `ata.md` kept; a malformed answer returns to `captured` with the provider error (AC 30)
Proof: run native | minutes::tests::a_timeout_names_the_provider_and_the_deadline ... ok
Proof: run native | session::tests::a_provider_timeout_returns_the_session_to_captured_and_keeps_the_files ... ok
Proof: run native | session::tests::a_malformed_provider_answer_returns_to_captured_with_the_files ... ok
Proof: walk - matrix M10

**C31** - A Drive upload timeout says the upload may already have completed and names `VOXIS Atas`; `ata.md` stays (AC 31)
Proof: run native | minutes::tests::a_drive_upload_timeout_says_it_may_have_completed_and_keeps_ata_md ... ok

**C32** - `ata.md` is written in the session folder before the first Drive call (AC 32)
Proof: run native | minutes::tests::ata_md_is_written_before_the_first_drive_call ... ok

**C33** - Without `anthropic_api_key` the session completes with `transcript.md` and no network call; without Google files it completes with `ata.md` (AC 33)
Proof: run native | session::tests::without_an_anthropic_key_the_session_completes_with_the_transcript ... ok
Proof: run native | session::tests::without_google_credentials_the_minutes_stay_in_ata_md ... ok

**C34** - With both Google files and a Drive success the note is the `.url` shortcut to the Doc in `VOXIS Atas` (AC 34)
Proof: run native | session::tests::a_drive_upload_makes_the_note_a_url_shortcut_to_the_doc ... ok

### S6 - Dictation, overlay and privacy

**C35** - A hold of at least 250 ms released with more than 300 ms of audio is transcribed and pasted followed by one space (AC 35)
Proof: run native | dictate::tests::a_hold_of_250_ms_with_audio_is_transcribed_and_pasted_with_a_space ... ok
Proof: walk - matrix M3

**C36** - A release within 250 ms pastes nothing; a second such tap within 450 ms turns hands-free on (AC 36)
Proof: run native | dictate::tests::a_short_tap_pastes_nothing_and_two_taps_turn_on_hands_free ... ok

**C37** - Hands-free closes an utterance after 400 ms of speech and 700 ms of silence, and at 30 s (AC 37)
Proof: run native | dictate::tests::hands_free_closes_an_utterance_after_700_ms_of_silence ... ok
Proof: run native | dictate::tests::hands_free_closes_an_utterance_at_30_s ... ok
Proof: walk - matrix M4

**C38** - Any press during hands-free ends it, transcribes cut speech of at least 400 ms, and ignores that press's release (AC 38)
Proof: run native | dictate::tests::a_press_during_hands_free_ends_it_and_keeps_the_cut_speech ... ok

**C39** - Every dictate sink holds the mode, audio seconds, processing seconds and character count, and no transcript text (AC 39)
Proof: run native | dictate::tests::the_utterance_log_holds_counts_and_timings_but_no_text ... ok
Proof: walk - matrix M3, `dictate-<date>.log`, `laris.log` and `laris.log.1` searched for the utterance text

**C40** - A paste sets the clipboard, sends Ctrl+V, waits 250 ms and restores the previous text (AC 40)
Proof: run native | dictate::tests::a_paste_puts_the_text_sends_ctrl_v_and_restores_the_previous_text ... ok

**C41** - Fewer than 4 injected events leave the text on the clipboard, skip the restore and log `paste failed: injected <n> of 4` (AC 41, unit-proven)
Proof: run native | dictate::tests::a_partial_injection_leaves_the_text_on_the_clipboard ... ok
Proof: run native | dictate::tests::a_paste_that_did_not_happen_has_no_completion_timestamp ... ok

**C42** - A clipboard that cannot be opened or set logs `paste failed: clipboard <error>` without the text and hides the orb (AC 42)
Proof: run native | dictate::tests::a_clipboard_failure_is_logged_without_the_text ... ok
Proof: run native | dictate::tests::a_clipboard_failure_hides_the_orb_in_both_modes ... ok

**C43** - An orb window or WebView2 that cannot be created leaves dictation running and logs `overlay unavailable: <error>` (AC 43, unit-proven)
Proof: run native | dictate::tests::an_orb_that_cannot_be_created_leaves_dictation_running ... ok

**C44** - An input stream error logs `microfone desconectado: <error>`, hides the orb, exits 3; the tray does not respawn exit 3, shows `Ditado: microfone desconectado`, and one click starts `dictate` once (AC 44)
Proof: run native | dictate::tests::a_lost_microphone_logs_and_exits_with_code_3 ... ok
Proof: run tray | tests::a_dictation_that_lost_its_microphone_is_not_respawned ... ok
Proof: run tray | tests::the_dictation_row_offers_one_restart_after_a_lost_microphone ... ok
Proof: run native | dictate::tests::a_dictate_that_starts_with_no_microphone_exits_3_like_a_lost_one ... ok
Proof: run tray | tests::a_poll_that_sees_exit_3_or_a_model_failure_does_not_respawn_in_the_same_tick ... ok
Proof: run tray | tests::a_poll_after_another_exit_keeps_the_three_attempt_respawn ... ok
Proof: walk - matrix M11

**C45** - An input format other than `f32`/`i16`, or a model that fails to load, logs `formato de áudio não suportado: <format>` or `modelo não carregou: <path>: <error>` and exits 1 (AC 45, unit-proven)
Proof: run native | dictate::tests::an_unsupported_sample_format_or_model_failure_is_logged_with_exit_1 ... ok
Proof: run native | dictate::tests::a_model_that_fails_to_make_its_state_or_to_warm_up_is_logged_not_a_panic ... ok

### S7 - Hardware, backend and model recovery

**C46** - The selected backend (`CUDA0`, `Vulkan0`, `CPU`) is recorded from whisper.cpp's `using <name> backend` line: by the bench driver from stderr, and in `laris.log` for dictate (no product code added only to print it) (AC 46)
Proof: run bench | test_the_backend_is_read_from_the_whisper_cpp_stderr_line PASSED
Proof: walk - matrix M3, the backend line in `laris.log`

**C47** - A GPU init failure falling back to CPU logs `sem GPU: usando CPU` in dictate and `backend: "CPU"` in the benchmark JSON (AC 47, unit-proven)
Proof: run native | dictate::tests::a_cpu_fallback_is_logged_as_sem_gpu ... ok
Proof: run bench | test_a_gpu_init_failure_is_recorded_as_the_cpu_backend PASSED
Proof: run native | dictate::tests::the_backend_whisper_cpp_names_is_read_from_its_own_log_line ... ok
Proof: run native | dictate::tests::a_gpu_that_whisper_cpp_did_not_use_is_logged_as_sem_gpu_whatever_ggml_registered ... ok
Proof: run native | dictate::tests::without_a_backend_report_the_registered_devices_decide ... ok
Proof: run native | dictate::tests::the_log_hook_is_wired_to_whisper_cpp_itself ... ok

**C48** - `models fetch --verify` hashes each present file against its pin and replaces a differing one only after a verified download through `<name>.part`; plain `fetch` keeps the name-based `ready`; the tray's retry row after `modelo não carregou` runs `--verify` (AC 48)
Proof: run native | models::tests::verify_replaces_a_file_whose_hash_differs_after_a_verified_download ... ok
Proof: run native | models::tests::verify_leaves_files_that_match_their_pin ... ok
Proof: run native | models::tests::fetching_models_that_are_already_there_downloads_nothing ... ok
Proof: run tray | models::tests::a_model_load_failure_offers_verify_in_the_retry_row ... ok
Proof: walk - matrix M2, hash duration recorded

**C49** - A cut or disk-full `--verify` download leaves the old file untouched, deletes only its `.part`, exits 1, and writes nothing unverified under the final name (AC 49, unit-proven)
Proof: run native | models::tests::a_cut_verify_download_keeps_the_old_file_and_removes_its_part ... ok
Proof: run native | models::tests::a_cut_download_with_no_old_file_leaves_nothing_under_the_final_name ... ok
Proof: run native | models::tests::a_full_disk_during_verify_keeps_the_old_file ... ok

**C50** - README and `INSTALL.md` name the verified Windows configuration and list the untested ones (AC 50)
Proof: run contract | PASS: verified_windows_configuration

### S8 - Documents and release claims match the product

**C51** - No document calls Windows an external fork or removed, except dated history lines; `INSTALL.md` points to the Windows install section (AC 51)
Proof: run contract | PASS: no_windows_fork_statement

**C52** - The README download statement matches `gh release view` read with `gh-dev` in the same session: the tag and "release candidate, not published" while no non-draft release exists (AC 52)
Proof: run contract | PASS: readme_download_statement_names_the_draft_tag
Proof: walk - at ready time, `gh release view` with `gh-dev` compared to the README line

**C53** - The README gives the installer sizes (CUDA 523,385,745 bytes, Vulkan 9,641,882 bytes at rc3) and labels GPU generations and "Fastest" as vendor-stated or measured by S10 (AC 53)
Proof: run contract | PASS: installer_sizes_and_labels

**C54** - The README states `f13` default and `f5`-`f15`, `ggml-large-v3-turbo` with Silero VAD, notes to `ata.md` or a Google Doc not Obsidian, hands-free on an energy gate (AC 54)
Proof: run contract | PASS: windows_dictation_and_notes_facts

**C55** - The README privacy section states what leaves a Windows machine and does not claim `0600` there (AC 55)
Proof: run contract | PASS: windows_privacy_statement

**C56** - README and `INSTALL.md` state where Windows keeps audio, transcripts, notes and logs, that nothing deletes them, and how to remove them (AC 56)
Proof: run contract | PASS: windows_data_locations

**C57** - `INSTALL.md` states `%LOCALAPPDATA%\VOXIS` and `%LOCALAPPDATA%\voxis` are one folder, uninstall keeps the models, layout unchanged (AC 57)
Proof: run contract | PASS: install_folder_equals_data_folder

**C58** - The developer notes state Git Bash rewrites `/S` and `/v` and give `MSYS2_ARG_CONV_EXCL='*'` or `//S`/`//v` (AC 58)
Proof: run contract | PASS: msys_argument_rewrite_note

**C59** - The README states dictation replaces non-text clipboard contents, Windows blocks injection into administrator windows silently, and what VOXIS does then (AC 59)
Proof: run contract | PASS: clipboard_and_elevated_window_limits

**C60** - The audit claim names the lockfiles the workflow scans; while the workflow does not scan `spikes/windows-rust/Cargo.lock`, the README says the native binary is not covered (AC 60; the Rec workflow job needs a token with `workflow` scope, see Handoff)
Proof: run contract | PASS: audit_scope_names_the_native_lock

### S9 - Verification, immutability and the acceptance matrix

**C61** - Read with `gh-dev`, tag `v0.2.0-rc3` is still at `889d48c9a2cca7f61a0648b832c3c3ac20953c78` and the draft assets 719717274 and 719717270 keep their sha256 (AC 61)
Proof: walk - verifier reads tag and release with `gh-dev`; unreadable is reported unverified

**C62** - A change to shipped bytes, packaging, workflow or runtime inputs gets a new tag chosen by Sammy, built by CI from a reviewed SHA into a draft release; rc3 acceptance stays valid for rc3 bytes only (AC 62)
Proof: walk - candidate step of the delivery order, after review

**C63** - CUDA 13.3.1, the two model pins, the per-user NSIS layout and the menu wiring are unchanged, shown by an empty diff on those lines (AC 63)
Proof: run contract | PASS: pins_cuda_layout_and_menu_wiring_unchanged

**C64** - Each S1-S5 data-loss, disclosure or stuck-session test, and those of AC 18, 49 and 86, fails at the frozen SHA before its production change and passes after it, in the same change (AC 64)
Proof: walk - `development.md` red/green log against `889d48c`, each red named with its exit and first failing line

**C65** - Rita reviews the diff and Vera verifies the matrix; Dani and Felix do not review or verify their own changes (AC 65)
Proof: walk - review and verification records of the candidate SHA

**C66** - Native (25), Windows tray (61), Linux tray (56) and both Python suites stay green; no file under `packages/` and no macOS or Linux build file changes; the lock gains only `toml_edit`, `sysinfo` and their dependencies with no version change (AC 66)
Proof: run native | test result: ok.
Proof: run tray | test result: ok.
Proof: run contract | PASS: no_packages_or_macos_linux_build_changes
Proof: run contract | PASS: lock_adds_only_toml_edit_sysinfo_and_their_dependencies
Proof: walk - CI `python` and `linux` jobs green at the candidate SHA

**C67** - Each matrix row ends PASS, FAIL or BLOCKED with raw output, exit code, source SHA and host (AC 67)
Proof: walk - Vera's matrix report

**C68** - The PR stays a draft; no merge, publication, tag or asset change happens without Sammy; the macOS `GrantMic.app` red is reported pre-existing (AC 68)
Proof: walk - `gh pr view` shows draft; PR body names the macOS red

### S10 - Rust performance evaluation (layer 1 harness built here; runs belong to Vera)

**C69** - `perf-protocol.md` is written with its SHA-256 before the first measured run; the driver runs nothing without a protocol file (AC 69)
Proof: run bench | test_a_run_without_a_protocol_file_runs_nothing PASSED
Proof: walk - protocol hash recorded before Vera's first run

**C70** - The corpus hashes are the pinned ones; a recomputed difference stops with `corpus mismatch` and runs nothing (AC 70)
Proof: run bench | test_a_corpus_whose_audio_hash_differs_stops_with_corpus_mismatch PASSED
Proof: run bench | test_the_pinned_corpus_hashes_are_the_ones_the_plan_names PASSED

**C71** - Native and Python variants use `pt`, beam 5, no context, no VAD and one prompt from `DEFAULT_PROMPT` saved as `prompt.txt` (through `STT_PROMPT` and `STT_PROMPT_FILE`), plus a no-prompt table (AC 71)
Proof: run bench | test_the_shipped_default_prompt_is_extracted_from_dictate_rs PASSED
Proof: run bench | test_the_promptfile_config_reads_its_prompt_from_stt_prompt_file PASSED
Proof: walk - Vera's prompt and noprompt tables

**C72** - CUDA, Vulkan and Python on the same host; a variant that cannot run reports `unavailable: <exact error>` (AC 72)
Proof: run bench | test_a_variant_that_fails_is_recorded_with_its_exit_code_and_error PASSED
Proof: run bench | test_a_variant_that_cannot_launch_is_recorded_as_unavailable_with_its_launch_error PASSED
Proof: run bench | test_a_run_with_an_unlaunchable_variant_still_writes_the_manifest PASSED
Proof: walk - Vera's runs

**C73** - Each variant run writes schema 2 raw JSON with exit code, stderr, binary hash, source SHA, backend, build record and host record; an unreadable field is null with a reason (AC 73)
Proof: run bench | test_a_fake_variant_run_writes_raw_json_with_exit_code_backend_and_hashes PASSED
Proof: run bench | test_a_missing_source_sha_keeps_the_reason_it_could_not_be_read PASSED
Proof: run bench | test_a_python_variant_record_names_the_engine_package_the_model_and_the_prompt PASSED
Proof: run bench | test_a_native_variant_record_carries_the_model_file_hash PASSED
Proof: run bench | test_voice_py_reports_the_python_package_versions_and_the_model_identity PASSED

**C74** - The report gives warm p50 and p95, RTF per sweep and cold time per variant, with run counts and min, median, max (AC 74)
Proof: run bench | test_process_stats_give_warm_percentiles_over_all_sweeps_and_rtf_per_sweep PASSED
Proof: run bench | test_runs_are_summarized_with_count_min_median_and_max PASSED

**C75** - CPU time, peak working set and GPU memory are sampled at a documented interval; a missing counter is unavailable, never 0 (AC 75)
Proof: run bench | test_a_per_process_gpu_counter_shown_as_na_is_unavailable_not_zero PASSED
Proof: run bench | test_a_resource_summary_never_turns_a_missing_counter_into_zero PASSED

**C76** - 6 serial blocks use the 6 orderings of V1, V2, V3, one process per variant per block, each a cold start, a warm-up and 10 warm sweeps (AC 76)
Proof: run bench | test_six_blocks_use_every_ordering_of_three_variants_once PASSED
Proof: walk - Vera's run manifest

**C77** - A difference is claimed only when the per-block sign is the same in all 6 blocks, else `no consistent difference` (AC 77)
Proof: run bench | test_a_difference_is_claimed_only_when_every_block_has_the_same_sign PASSED
Proof: run bench | test_one_block_with_the_other_sign_means_no_consistent_difference PASSED
Proof: run bench | test_a_difference_needs_all_six_distinct_protocol_blocks PASSED
Proof: run bench | test_a_report_from_fewer_than_six_blocks_makes_no_difference_claim PASSED

**C78** - `voice_score.py` reports WER, CER, term recall, empty outputs, repeated n-grams and omitted words over one normalization and reference, schema 1 and 2, and states the 19-against-20 mismatch (AC 78)
Proof: run bench | test_scoring_reports_wer_cer_terms_empty_outputs_repeats_and_omissions PASSED
Proof: run bench | test_a_schema_1_file_and_a_schema_2_file_read_the_same_hypotheses PASSED

**C79** - A generated 10 s silence clip and a 10 s noise clip run through every variant and each non-empty output is reported with the clip hashes (AC 79)
Proof: run bench | test_generated_silence_and_noise_clips_are_ten_seconds_of_16_khz_mono PASSED
Proof: run bench | test_a_control_run_reports_every_non_empty_output PASSED

**C80** - The dictation log carries key-release and paste-complete timestamps (no text) for release-to-paste p50 and p95 on the candidate bytes (AC 80)
Proof: run native | dictate::tests::the_utterance_log_carries_release_and_paste_timestamps ... ok
Proof: run native | dictate::tests::a_completed_paste_reports_when_ctrl_v_was_sent_and_when_the_clipboard_came_back ... ok
Proof: run native | dictate::tests::the_utterance_line_names_paste_and_restore_times_only_for_a_paste_that_happened ... ok
Proof: walk - layer 2 on the candidate installer

**C81** - Stop-to-`transcript_ready` time of a stored two-track fixture is measured with the candidate's shipped `meetrec`, apart from provider time, with loopback fakes (AC 81)
Proof: run native | session::tests::the_meeting_latency_marks_name_the_stop_and_the_ready_transcript ... ok
Proof: walk - layer 2 on the candidate installer

**C82** - Measurements needing a provider, free speech or a per-phase action run inside the gate; evidence holds no audio, free speech, meeting content or dictation text (AC 82)
Proof: walk - Vera's evidence inventory

**C83** - The report presents numbers, host, limits and trade-offs and states no target, allowance or "Rust is faster" sentence (AC 83)
Proof: walk - Vera's report

**C84** - Layer 1 at `H` (`889d48c` plus the harness commit) stands while the engine inputs are unchanged from `H` to the candidate (AC 84)
Proof: run contract | PASS: engine_inputs_unchanged_since_h
Proof: walk - Vera records `H` in the protocol

### S11 - Additions from the plan review

**C85** - An owner is live only when `sysinfo` shows this `meetrec` image and the argv is exactly the marker and the session folder (AC 85)
Proof: run native | session::tests::an_owner_is_live_only_with_this_image_and_exactly_the_marker_and_folder ... ok
Proof: run native | session::tests::sysinfo_reads_the_argv_elements_of_a_spawned_process ... ok

**C86** - `process <id>` and `recover <id>` on a session with a live owner exit 1 `session is active: <dir>` and change nothing; `stop` writes its stop file only after verifying the recorder (AC 86)
Proof: run native | session::tests::process_and_recover_refuse_a_session_whose_owner_is_live ... ok
Proof: run native | session::tests::stop_writes_the_stop_file_only_after_checking_the_recorder ... ok
Proof: run native | session::tests::stop_settles_a_session_whose_recorder_exited_at_once_under_the_system_owner_check ... ok

**C87** - The harness commit changes exactly `stt-bench/src/main.rs`, `voice.py`, `bench.py`, `voice_score.py` and the new `perf_driver.py` on top of `889d48c` (AC 87)
Proof: run contract | PASS: harness_commit_changes_exactly_the_listed_files

**C88** - `process <id>` with `transcript_ready` reuses a non-empty `transcript.md` newer than both WAVs, and an `ata.md` newer than the transcript, making no provider call for a finished step (AC 88)
Proof: run native | session::tests::a_retry_reuses_a_fresh_transcript_and_runs_only_the_minutes ... ok
Proof: run native | session::tests::a_wav_newer_than_the_transcript_runs_the_full_pipeline ... ok
Proof: run native | minutes::tests::a_fresh_ata_md_is_reused_without_asking_anthropic ... ok

**C89** - A Drive upload first lists `VOXIS Atas` for `appProperties` `voxis_session=<id>`, reuses that link, and otherwise creates the file with the property (AC 89)
Proof: run native | minutes::tests::a_retry_after_an_upload_that_completed_returns_the_existing_doc ... ok
Proof: run native | minutes::tests::a_new_upload_carries_the_session_app_property ... ok

**C90** - An elevated foreground window, or one whose token cannot be queried, skips Ctrl+V, leaves the text on the clipboard without restore, and logs `paste skipped: elevated target` (AC 90)
Proof: run native | dictate::tests::an_elevated_target_skips_the_paste_and_keeps_the_text ... ok
Proof: walk - matrix M15

**C91** - Windows recognizes Portuguese only: `dictate` and the pipeline use `pt`, the settings window shows `Idioma: Português (Brasil)`, the README says so (AC 91)
Proof: run native | dictate::tests::dictation_recognizes_portuguese_only ... ok
Proof: run frontend | the Windows settings show the fixed Portuguese language
Proof: run contract | PASS: portuguese_only_statement

**C92** - Criteria 3, 18, 41, 43, 45, 47 and 49 are proven at pure-function seams with injected failures and marked unit-proven, not forced on the host (AC 92)
Proof: walk - matrix M17 lists the seam tests of C3, C18, C41, C43, C45, C47 and C49 as unit-proven

## Coverage

| Set (size) | Member -> proof | Unproven |
| --- | --- | --- |
| `GET /static/<path>` statuses (3) | 200 C1 · 400 C2 · 404 C3 C4 | - |
| `POST /theme` statuses (3) | 200 C6 · 403 C6 · 422 C6 | - |
| unsafe static path kinds, AC 2 (8) | drive-prefixed C2 · encoded `..` C2 · UNC C2 · `NUL` C2 · `CON` C2 · ADS C2 · rooted C2 · plain `..` C2, table-driven over all 8 | - |
| one-way doors (8) | `session.json` schema 1 C13 C14 C16 C17 · `toml_edit` C20 C66 · folder naming C8 C10 · CLI output contract C81 C88 · Windows settings contract C22 C23 C24 · benchmark schema 2 C73 C78 · `sysinfo` identity C85 · Drive `appProperties` key C89 | - |
| new `error.code` values (3) | `owner_died` C13 · `provider_timeout` C30 · `disk_write_error` C18 | - |
| session states touched (5) | `interrupted` C13 C17 · `captured` C14 C30 · `failed` C16 C18 · `stopping` C17 · `starting` C16 | - |
| CLI commands guarded by liveness (4) | `start` C12 · `stop` C86 · `process` C86 · `recover` C86 | - |
| config keys read natively (3) | `mic` C20 · `speaker` C20 · `minutes_model` C20 | - |
| AC 20 examples (5) | comment C20 · single quote C20 · escape C20 · nested table C20 · empty C20, table-driven | - |
| legacy controls (5) | Vault C25 · Comando LLM C25 · Categorias C25 · Modelo STT C25 · Redução de ruído C25 | - |
| provider deadlines (5) | connect C29 · Anthropic C29 · OAuth C29 · Drive list and create C29 · Drive upload C29 C31 | - |
| dictate log sinks (4) | `dictate-<date>.log` C39 · stdout/stderr C39 · `laris.log` C39 walk · `laris.log.1` C39 walk | - |
| injected-failure seams, AC 92 (7) | junction C3 · write failure C18 · `SendInput` count C41 · orb creation C43 · sample format C45 · backend result C47 · cut download C49 | - |
| layer 1 variants (3) | V1 CUDA C72 C76 · V2 Vulkan C72 C76 · V3 Python C72 C76 | - |
| control clips (2) | silence C79 · noise C79 | - |
| documents (4) | README C50 C52 C53 C54 C55 C56 C59 C60 C91 · `INSTALL.md` C50 C51 C56 C57 · `BACKLOG.md` C51 · developer notes C58 | - |
| matrix rows (17) | M1 C67 · M2 C48 · M3 C35 C39 · M4 C37 · M5 C24 · M6 C28 · M7 C13 · M8 C14 · M9 C15 · M10 C30 · M11 C44 · M12 C2 · M13 C8 · M14 C67 · M15 C90 · M16 C3 · M17 C92 | - |

## Swept

- validation: C2, C20, C21, C23, C26, C70
- failure modes: C17, C18, C30, C31, C43, C44, C45, C49
- idempotency: C88, C89
- authorization: C6, C90
- concurrency: C8, C10, C12
- data lifecycle: C11, C19, C25, C49, C56
- dependency failure: C29, C33, C47
- state transitions: C13, C14, C16, C17
- observability: C39, C46, C73

## Handoff

- **Settled mid-build:** none; no user answer was needed while building
- **Dispatched:** none by the author; independent review (Rita) and verification (Vera) are routed by Otto, per the Engineering channel and AC 65
- **Round 1 (Felix):** fixes for Rita's findings on `581fefc`, code head `b2d6b5c`; 160 run proofs and 3 builds pass in `proofs.md`, and the 34 `walk` rows are still pending (see `development.md`, "Review round 1"); this is development evidence, not review or verification
- **Boundary:** C1-C92 proofs that run in this session pass at `1ff3403` (131 run proofs and the build, `proofs.md`); harness `H` = `77a4ba37d7ee72c99f3831c737fce1c34904bc89`
- **Abandoned:** the native-lock job in `security-audit.yml` (push refused without a `workflow`-scope token; C60 uses the README fallback AC 60 allows); `toml_edit` 0.22 (moved two locked versions; 0.25 already locked is used); whisper-rs `raw-api` and `log_backend` (feature changes to the engine inputs of AC 84)
- **Walked:** none. The 34 `walk` proofs are matrix rows and reviewer steps still to run; `check_proofs.py` lists them as "pass (Walked)" only because this line exists, not because anything was walked
