# windows-port proofs

**HEAD**: ac0d020
**Tree**: clean
**Shell**: C:/Program Files/Git\bin\bash.exe
**Result**: 167 pass, 0 fail

| Check | Proof | Result |
| --- | --- | --- |
| C1 | `run native / dictate::tests::a_file_under_the_frontend_root_is_served ... ok` | pass |
| C1 | `run native / dictate::tests::a_file_in_a_subfolder_of_the_frontend_root_is_served ... ok` | pass |
| C2 | `run native / dictate::tests::unsafe_static_paths_are_refused_with_400_bad_path ... ok` | pass |
| C2 | `run native / dictate::tests::a_sentinel_outside_the_root_is_never_served_over_http ... ok` | pass |
| C2 | `walk - matrix M12 on the candidate installer` | pass (Walked) |
| C3 | `run native / dictate::tests::a_junction_inside_the_root_pointing_outside_answers_404 ... ok` | pass |
| C3 | `walk - matrix M16 on the candidate installer` | pass (Walked) |
| C4 | `run native / dictate::tests::a_missing_frontend_folder_answers_404_for_overlay_and_static ... ok` | pass |
| C5 | `run native / dictate::tests::the_overlay_page_is_served_without_a_token ... ok` | pass |
| C6 | `run native / dictate::tests::theme_requests_without_the_token_get_403_and_leave_the_theme_file_unchanged ... o` | pass |
| C6 | `run native / dictate::tests::a_theme_update_with_the_token_answers_200_and_an_invalid_one_422 ... ok` | pass |
| C7 | `run native / tests::the_meeting_folder_is_reserved_with_a_create_new_call ... ok` | pass |
| C8 | `run native / tests::a_second_meeting_in_the_same_minute_gets_its_own_folder_and_keeps_the_first ... ok` | pass |
| C8 | `run native / tests::after_99_folders_in_one_minute_the_meeting_refuses_with_no_free_session_folder ... ok` | pass |
| C8 | `walk - matrix M13` | pass (Walked) |
| C9 | `run native / tests::recording_into_a_folder_that_already_holds_a_track_is_refused_and_leaves_it_unchanged ... ` | pass |
| C9 | `run native / tests::recording_into_a_missing_or_empty_folder_is_allowed ... ok` | pass |
| C10 | `run native / session::tests::start_refuses_a_session_folder_that_already_exists ... ok` | pass |
| C11 | `run native / session::tests::recover_fixes_only_the_header_sizes_and_keeps_every_sample_byte ... ok` | pass |
| C11 | `run native / session::tests::recover_leaves_a_valid_wav_byte_identical ... ok` | pass |
| C12 | `run native / session::tests::start_refuses_while_the_newest_active_session_has_a_live_owner ... ok` | pass |
| C13 | `run native / session::tests::a_dead_recorder_is_marked_interrupted_with_owner_died_and_a_new_session_starts ..` | pass |
| C13 | `run native / session::tests::a_pid_reused_by_another_program_is_not_a_live_owner ... ok` | pass |
| C13 | `walk - matrix M7` | pass (Walked) |
| C14 | `run native / session::tests::a_dead_pipeline_puts_its_session_back_to_captured ... ok` | pass |
| C14 | `walk - matrix M8` | pass (Walked) |
| C15 | `run tray / miris::tests::a_live_recorder_under_a_profile_with_spaces_shows_recording ... ok` | pass |
| C15 | `run tray / miris::tests::a_reused_pid_with_a_different_argv_is_not_live ... ok` | pass |
| C15 | `walk - matrix M9` | pass (Walked) |
| C16 | `run native / session::tests::a_recorder_that_exits_before_recording_fails_the_session_with_recorder_error ... ` | pass |
| C17 | `run native / session::tests::stop_ends_a_recorder_that_never_finishes_and_marks_it_interrupted ... ok` | pass |
| C17 | `run native / session::tests::stop_keeps_stopping_when_the_recorder_cannot_be_ended ... ok` | pass |
| C18 | `run native / tests::a_failed_wav_write_finalizes_the_header_of_the_samples_written ... ok` | pass |
| C18 | `run native / session::tests::a_disk_write_error_fails_the_session_and_keeps_both_files ... ok` | pass |
| C19 | `run contract / PASS: no_automatic_retention` | pass |
| C20 | `run native / tests::config_toml_values_are_read_as_toml_like_the_tray ... ok` | pass |
| C21 | `run native / tests::an_invalid_config_toml_is_reported_and_the_defaults_are_used ... ok` | pass |
| C22 | `run tray / commands::tests::windows_offers_exactly_f5_to_f15_for_dictation ... ok` | pass |
| C22 | `run frontend / the key list is the platform's and preselects the key the daemon uses` | pass |
| C22 | `run native / dictate::tests::the_daemon_uses_f13_when_the_saved_key_is_unsupported ... ok` | pass |
| C23 | `run tray / commands::tests::windows_rejects_a_dictate_key_outside_f5_to_f15_and_writes_nothing ... ok` | pass |
| C23 | `run tray / commands::tests::windows_saves_f13 ... ok` | pass |
| C24 | `run frontend / a saved unsupported key shows the F13 notice` | pass |
| C24 | `walk - matrix M5, file hash before and after opening the window` | pass (Walked) |
| C25 | `run frontend / legacy controls are disabled with the Windows notice` | pass |
| C25 | `run frontend / the Windows form leaves STT_MODEL and NOISE_REDUCTION out of the save` | pass |
| C25 | `run tray / commands::tests::saving_f13_from_the_windows_form_keeps_stt_model_and_noise_reduction ... ok` | pass |
| C26 | `run tray / commands::tests::windows_refuses_stt_model_or_noise_reduction_from_any_caller ... ok` | pass |
| C26 | `run tray / commands::tests::windows_refuses_the_meetings_config_save ... ok` | pass |
| C26 | `run tray / commands::tests::windows_accepts_dictate_key_assistant_mode_and_initial_prompt ... ok` | pass |
| C27 | `run native / dictate::tests::the_next_dictate_start_applies_assistant_mode_and_initial_prompt ... ok` | pass |
| C28 | `run native / tests::a_chosen_device_is_used_and_an_absent_one_falls_back_with_a_warning ... ok` | pass |
| C28 | `walk - matrix M6 and M3 with a chosen device` | pass (Walked) |
| C29 | `run native / minutes::tests::every_provider_call_carries_its_deadline ... ok` | pass |
| C29 | `run native / minutes::tests::a_stalled_anthropic_call_ends_at_its_deadline ... ok` | pass |
| C30 | `run native / minutes::tests::a_timeout_names_the_provider_and_the_deadline ... ok` | pass |
| C30 | `run native / session::tests::a_provider_timeout_returns_the_session_to_captured_and_keeps_the_files ... ok` | pass |
| C30 | `run native / session::tests::a_malformed_provider_answer_returns_to_captured_with_the_files ... ok` | pass |
| C30 | `walk - matrix M10` | pass (Walked) |
| C31 | `run native / minutes::tests::a_drive_upload_timeout_says_it_may_have_completed_and_keeps_ata_md ... ok` | pass |
| C32 | `run native / minutes::tests::ata_md_is_written_before_the_first_drive_call ... ok` | pass |
| C33 | `run native / session::tests::without_an_anthropic_key_the_session_completes_with_the_transcript ... ok` | pass |
| C33 | `run native / session::tests::without_google_credentials_the_minutes_stay_in_ata_md ... ok` | pass |
| C34 | `run native / session::tests::a_drive_upload_makes_the_note_a_url_shortcut_to_the_doc ... ok` | pass |
| C35 | `run native / dictate::tests::a_hold_of_250_ms_with_audio_is_transcribed_and_pasted_with_a_space ... ok` | pass |
| C35 | `walk - matrix M3` | pass (Walked) |
| C36 | `run native / dictate::tests::a_short_tap_pastes_nothing_and_two_taps_turn_on_hands_free ... ok` | pass |
| C37 | `run native / dictate::tests::hands_free_closes_an_utterance_after_700_ms_of_silence ... ok` | pass |
| C37 | `run native / dictate::tests::hands_free_closes_an_utterance_at_30_s ... ok` | pass |
| C37 | `walk - matrix M4` | pass (Walked) |
| C38 | `run native / dictate::tests::a_press_during_hands_free_ends_it_and_keeps_the_cut_speech ... ok` | pass |
| C39 | `run native / dictate::tests::the_utterance_log_holds_counts_and_timings_but_no_text ... ok` | pass |
| C39 | `walk - matrix M3, dictate-<date>.log, laris.log and laris.log.1 searched for the utterance text` | pass (Walked) |
| C40 | `run native / dictate::tests::a_paste_puts_the_text_sends_ctrl_v_and_restores_the_previous_text ... ok` | pass |
| C41 | `run native / dictate::tests::a_partial_injection_leaves_the_text_on_the_clipboard ... ok` | pass |
| C42 | `run native / dictate::tests::a_clipboard_failure_is_logged_without_the_text ... ok` | pass |
| C43 | `run native / dictate::tests::an_orb_that_cannot_be_created_leaves_dictation_running ... ok` | pass |
| C44 | `run native / dictate::tests::a_lost_microphone_logs_and_exits_with_code_3 ... ok` | pass |
| C44 | `run tray / tests::a_dictation_that_lost_its_microphone_is_not_respawned ... ok` | pass |
| C44 | `run tray / tests::the_dictation_row_offers_one_restart_after_a_lost_microphone ... ok` | pass |
| C44 | `walk - matrix M11` | pass (Walked) |
| C45 | `run native / dictate::tests::an_unsupported_sample_format_or_model_failure_is_logged_with_exit_1 ... ok` | pass |
| C46 | `run bench / test_the_backend_is_read_from_the_whisper_cpp_stderr_line PASSED` | pass |
| C46 | `walk - matrix M3, the backend line in laris.log` | pass (Walked) |
| C47 | `run native / dictate::tests::a_cpu_fallback_is_logged_as_sem_gpu ... ok` | pass |
| C47 | `run bench / test_a_gpu_init_failure_is_recorded_as_the_cpu_backend PASSED` | pass |
| C48 | `run native / models::tests::verify_replaces_a_file_whose_hash_differs_after_a_verified_download ... ok` | pass |
| C48 | `run native / models::tests::verify_leaves_files_that_match_their_pin ... ok` | pass |
| C48 | `run native / models::tests::fetching_models_that_are_already_there_downloads_nothing ... ok` | pass |
| C48 | `run tray / models::tests::a_model_load_failure_offers_verify_in_the_retry_row ... ok` | pass |
| C48 | `walk - matrix M2, hash duration recorded` | pass (Walked) |
| C49 | `run native / models::tests::a_cut_verify_download_keeps_the_old_file_and_removes_its_part ... ok` | pass |
| C49 | `run native / models::tests::a_cut_download_with_no_old_file_leaves_nothing_under_the_final_name ... ok` | pass |
| C49 | `run native / models::tests::a_full_disk_during_verify_keeps_the_old_file ... ok` | pass |
| C50 | `run contract / PASS: verified_windows_configuration` | pass |
| C51 | `run contract / PASS: no_windows_fork_statement` | pass |
| C52 | `run contract / PASS: readme_download_statement_names_the_draft_tag` | pass |
| C52 | `walk - at ready time, gh release view with gh-dev compared to the README line` | pass (Walked) |
| C53 | `run contract / PASS: installer_sizes_and_labels` | pass |
| C54 | `run contract / PASS: windows_dictation_and_notes_facts` | pass |
| C55 | `run contract / PASS: windows_privacy_statement` | pass |
| C56 | `run contract / PASS: windows_data_locations` | pass |
| C57 | `run contract / PASS: install_folder_equals_data_folder` | pass |
| C58 | `run contract / PASS: msys_argument_rewrite_note` | pass |
| C59 | `run contract / PASS: clipboard_and_elevated_window_limits` | pass |
| C60 | `run contract / PASS: audit_scope_names_the_native_lock` | pass |
| C61 | `walk - verifier reads tag and release with gh-dev; unreadable is reported unverified` | pass (Walked) |
| C62 | `walk - candidate step of the delivery order, after review` | pass (Walked) |
| C63 | `run contract / PASS: pins_cuda_layout_and_menu_wiring_unchanged` | pass |
| C64 | `walk - development.md red/green log against 889d48c, each red named with its exit and first failing line` | pass (Walked) |
| C65 | `walk - review and verification records of the candidate SHA` | pass (Walked) |
| C66 | `run native / test result: ok.` | pass |
| C66 | `run tray / test result: ok.` | pass |
| C66 | `run contract / PASS: no_packages_or_macos_linux_build_changes` | pass |
| C66 | `run contract / PASS: lock_adds_only_toml_edit_sysinfo_and_their_dependencies` | pass |
| C66 | `walk - CI python and linux jobs green at the candidate SHA` | pass (Walked) |
| C67 | `walk - Vera's matrix report` | pass (Walked) |
| C68 | `walk - gh pr view shows draft; PR body names the macOS red` | pass (Walked) |
| C69 | `run bench / test_a_run_without_a_protocol_file_runs_nothing PASSED` | pass |
| C69 | `walk - protocol hash recorded before Vera's first run` | pass (Walked) |
| C70 | `run bench / test_a_corpus_whose_audio_hash_differs_stops_with_corpus_mismatch PASSED` | pass |
| C70 | `run bench / test_the_pinned_corpus_hashes_are_the_ones_the_plan_names PASSED` | pass |
| C71 | `run bench / test_the_shipped_default_prompt_is_extracted_from_dictate_rs PASSED` | pass |
| C71 | `run bench / test_the_promptfile_config_reads_its_prompt_from_stt_prompt_file PASSED` | pass |
| C71 | `walk - Vera's prompt and noprompt tables` | pass (Walked) |
| C72 | `run bench / test_a_variant_that_fails_is_recorded_with_its_exit_code_and_error PASSED` | pass |
| C72 | `walk - Vera's runs` | pass (Walked) |
| C73 | `run bench / test_a_fake_variant_run_writes_raw_json_with_exit_code_backend_and_hashes PASSED` | pass |
| C74 | `run bench / test_process_stats_give_warm_percentiles_over_all_sweeps_and_rtf_per_sweep PASSED` | pass |
| C74 | `run bench / test_runs_are_summarized_with_count_min_median_and_max PASSED` | pass |
| C75 | `run bench / test_a_per_process_gpu_counter_shown_as_na_is_unavailable_not_zero PASSED` | pass |
| C75 | `run bench / test_a_resource_summary_never_turns_a_missing_counter_into_zero PASSED` | pass |
| C76 | `run bench / test_six_blocks_use_every_ordering_of_three_variants_once PASSED` | pass |
| C76 | `walk - Vera's run manifest` | pass (Walked) |
| C77 | `run bench / test_a_difference_is_claimed_only_when_every_block_has_the_same_sign PASSED` | pass |
| C77 | `run bench / test_one_block_with_the_other_sign_means_no_consistent_difference PASSED` | pass |
| C78 | `run bench / test_scoring_reports_wer_cer_terms_empty_outputs_repeats_and_omissions PASSED` | pass |
| C78 | `run bench / test_a_schema_1_file_and_a_schema_2_file_read_the_same_hypotheses PASSED` | pass |
| C79 | `run bench / test_generated_silence_and_noise_clips_are_ten_seconds_of_16_khz_mono PASSED` | pass |
| C79 | `run bench / test_a_control_run_reports_every_non_empty_output PASSED` | pass |
| C80 | `run native / dictate::tests::the_utterance_log_carries_release_and_paste_timestamps ... ok` | pass |
| C80 | `walk - layer 2 on the candidate installer` | pass (Walked) |
| C81 | `run native / session::tests::the_meeting_latency_marks_name_the_stop_and_the_ready_transcript ... ok` | pass |
| C81 | `walk - layer 2 on the candidate installer` | pass (Walked) |
| C82 | `walk - Vera's evidence inventory` | pass (Walked) |
| C83 | `walk - Vera's report` | pass (Walked) |
| C84 | `run contract / PASS: engine_inputs_unchanged_since_h` | pass |
| C84 | `walk - Vera records H in the protocol` | pass (Walked) |
| C85 | `run native / session::tests::an_owner_is_live_only_with_this_image_and_exactly_the_marker_and_folder ... ok` | pass |
| C85 | `run native / session::tests::sysinfo_reads_the_argv_elements_of_a_spawned_process ... ok` | pass |
| C86 | `run native / session::tests::process_and_recover_refuse_a_session_whose_owner_is_live ... ok` | pass |
| C86 | `run native / session::tests::stop_writes_the_stop_file_only_after_checking_the_recorder ... ok` | pass |
| C87 | `run contract / PASS: harness_commit_changes_exactly_the_listed_files` | pass |
| C88 | `run native / session::tests::a_retry_reuses_a_fresh_transcript_and_runs_only_the_minutes ... ok` | pass |
| C88 | `run native / session::tests::a_wav_newer_than_the_transcript_runs_the_full_pipeline ... ok` | pass |
| C88 | `run native / minutes::tests::a_fresh_ata_md_is_reused_without_asking_anthropic ... ok` | pass |
| C89 | `run native / minutes::tests::a_retry_after_an_upload_that_completed_returns_the_existing_doc ... ok` | pass |
| C89 | `run native / minutes::tests::a_new_upload_carries_the_session_app_property ... ok` | pass |
| C90 | `run native / dictate::tests::an_elevated_target_skips_the_paste_and_keeps_the_text ... ok` | pass |
| C90 | `walk - matrix M15` | pass (Walked) |
| C91 | `run native / dictate::tests::dictation_recognizes_portuguese_only ... ok` | pass |
| C91 | `run frontend / the Windows settings show the fixed Portuguese language` | pass |
| C91 | `run contract / PASS: portuguese_only_statement` | pass |
| C92 | `walk - matrix M17 lists the seam tests of C3, C18, C41, C43, C45, C47 and C49 as unit-proven` | pass (Walked) |
| Build | `MSYS2_ARG_CONV_EXCL='*' cmd /c 'scripts\windows\meetrec-test.cmd --no-run'` | pass |
| Build | `cargo build --release --manifest-path apps/tray/src-tauri/Cargo.toml` | pass |
| Build | `npm --prefix apps/tray run build` | pass |

## Output

`Run native` - exit 0

```
test dictate::tests::the_overlay_page_is_served_without_a_token ... ok
test dictate::tests::theme_requests_without_the_token_get_403_and_leave_the_theme_file_unchanged ... ok
test dictate::tests::a_junction_inside_the_root_pointing_outside_answers_404 ... ok
test dictate::tests::a_theme_update_with_the_token_answers_200_and_an_invalid_one_422 ... ok
test session::tests::a_pid_reused_by_another_program_is_not_a_live_owner ... ok
test session::tests::sysinfo_reads_the_argv_elements_of_a_spawned_process ... ok
test dictate::tests::a_file_under_the_frontend_root_is_served ... ok

test result: ok. 95 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.47s

'vswhere.exe' n�o � reconhecido como um comando interno
ou externo, um programa oper�vel ou um arquivo em lotes.
   Compiling meetrec v0.1.0 (D:\voxis-port-work\repo\spikes\windows-rust\meetrec)
    Finished `release` profile [optimized] target(s) in 2.65s
     Running unittests src\main.rs (D:\vpc\release\deps\meetrec-b2f3f96a35562ec8.exe)
```

`Run tray` - exit 0

```
   Compiling tauri-macros v2.6.3
   Compiling tauri-plugin-notification v2.3.3
   Compiling tauri-plugin-opener v2.5.4
   Compiling tray v0.1.0 (D:\voxis-port-work\repo\apps\tray\src-tauri)
   Compiling tao v0.35.3
   Compiling tauri-winrt-notification v0.7.3
   Compiling sysinfo v0.37.2
   Compiling notify-rust v4.18.0
   Compiling webview2-com v0.38.2
   Compiling tauri-plugin-single-instance v2.4.3
    Finished `release` profile [optimized] target(s) in 1m 09s
     Running unittests src\lib.rs (D:\vpc\release\deps\tray_lib-f63f88a659cd12ec.exe)
O sistema n�o pode encontrar o caminho especificado.
     Running unittests src\main.rs (D:\vpc\release\deps\tray-adbbc2f4e625d0b2.exe)
   Doc-tests tray_lib
```

`Run contract` - exit 0

```
PASS: readme_download_statement_names_the_draft_tag
PASS: installer_sizes_and_labels
PASS: windows_dictation_and_notes_facts
PASS: windows_privacy_statement
PASS: windows_data_locations
PASS: install_folder_equals_data_folder
PASS: msys_argument_rewrite_note
PASS: clipboard_and_elevated_window_limits
PASS: audit_scope_names_the_native_lock
PASS: portuguese_only_statement
PASS: pins_cuda_layout_and_menu_wiring_unchanged
PASS: no_packages_or_macos_linux_build_changes
PASS: lock_adds_only_toml_edit_sysinfo_and_their_dependencies
PASS: engine_inputs_unchanged_since_h
PASS: harness_commit_changes_exactly_the_listed_files
```

`Run frontend` - exit 0

```
✔ the key list is the platform's and preselects the key the daemon uses (0.4287ms)
✔ a saved unsupported key shows the F13 notice (0.0762ms)
✔ legacy controls are disabled with the Windows notice (0.3796ms)
✔ the Windows form leaves STT_MODEL and NOISE_REDUCTION out of the save (0.1061ms)
✔ the Windows settings show the fixed Portuguese language (0.0674ms)
ℹ tests 5
ℹ suites 0
ℹ pass 5
ℹ fail 0
ℹ cancelled 0
ℹ skipped 0
ℹ todo 0
ℹ duration_ms 69.2265
```

`Run bench` - exit 0

```
scripts/stt_bench/test_perf_driver.py::test_a_variant_that_fails_is_recorded_with_its_exit_code_and_error PASSED [ 75%]
scripts/stt_bench/test_perf_driver.py::test_a_schema_1_file_and_a_schema_2_file_read_the_same_hypotheses PASSED [ 78%]
scripts/stt_bench/test_perf_driver.py::test_scoring_reports_wer_cer_terms_empty_outputs_repeats_and_omissions PASSED [ 82%]
scripts/stt_bench/test_perf_driver.py::test_identical_text_scores_zero_errors PASSED [ 85%]
scripts/stt_bench/test_perf_driver.py::test_voice_py_writes_schema_2_with_every_sweep_and_prints_ready PASSED [ 89%]
scripts/stt_bench/test_perf_driver.py::test_the_promptfile_config_reads_its_prompt_from_stt_prompt_file PASSED [ 92%]
scripts/stt_bench/test_perf_driver.py::test_bench_flags_are_split_from_the_positionals PASSED [ 96%]
scripts/stt_bench/test_perf_driver.py::test_a_run_without_a_protocol_file_runs_nothing PASSED [100%]

============================= 28 passed in 0.73s ==============================
Downloading rapidfuzz (1.6MiB)
Downloading numpy (12.2MiB)
 Downloaded rapidfuzz
 Downloaded numpy
Installed 16 packages in 136ms
```
