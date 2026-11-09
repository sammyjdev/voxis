# M7 silent-track recovery - checks

Build: `CARGO_TARGET_DIR='D:\dani-m7-c18d6da1-20261109\target' MSYS2_ARG_CONV_EXCL='*' cmd /c 'scripts\windows\meetrec-test.cmd --no-run'`
Run m7: `CARGO_TARGET_DIR='D:\dani-m7-c18d6da1-20261109\target' MSYS2_ARG_CONV_EXCL='*' cmd /c 'scripts\windows\meetrec-test.cmd m7_'`
Run native: `CARGO_TARGET_DIR='D:\dani-m7-c18d6da1-20261109\target' MSYS2_ARG_CONV_EXCL='*' cmd /c 'scripts\windows\meetrec-test.cmd'`
Run tray: `cargo test --release --manifest-path apps/tray/src-tauri/Cargo.toml`
Plan: `.specs/features/m7-silent-track-recovery/plan.md` (approved copy of `plan-c18d6da1.md`, SHA-256 `c18d6da17cce5fd4b71c60c1783ba7711bbb15ccf342bbbf24cf181f0e8d5064`; only the `**Approved**` line differs)

Derived from the approved plan; no criterion is changed. `(AC n)` is the plan's criterion n; `Recipe` names the plan's pre-change failure recipe row and type (U defect red, A feature-absent red, S red at the behavior-neutral S0 commit, I integration on built bytes, C control with no red, H person or process rule). Every `Fail-first` line is a planned recipe: nothing here has been run, and no line in this file is a measured red. Criterion 26 makes the measured failing set, recorded before the production edit, the only evidence.

Test placement: tests that reach `session.rs` live in `#[cfg(test)] mod m7_tests` of `session.rs` and print `session::m7_tests::m7_<AC>_<name>`; tests of `load_16k`, the per-track read and `write_wav` live in `mod m7_tests` of `main.rs` and print `m7_tests::m7_<AC>_<name>`. `m7_s0_*` are S0 supporting tests. Every fixture builder asserts its own L, D, R and oracle class first, with a message beginning `fixture:` (plan, "Fixture assertion first"); the oracle class is written into the builder from the truth table, never taken from `classify`.

Red protocol (criterion 26): the `m7_tests` module text is appended, byte-identical, to a retained worktree pinned at `c2d7b6c6294c9fe67709cc6c2b1c1650fdff5de1` (U, A, C rows) or at the recorded S0 SHA (S rows), by an adapted copy of `scripts/windows/frozen-repro.sh` that removes no worktree; the failing set it measures, the module hash and the base SHA are recorded before the production line. Because later greens can turn a not-yet-built check green at `HEAD`, the frozen red at its base is the evidence; a check already green at `HEAD` when its turn comes is recorded as `green at HEAD before its edit` with no production edit.

87 checks in 7 slices · 3 one-way doors · 2 open, of which 1 blocks one member of C47

## Checks

### S0 - behavior-neutral seams (supporting; no numbered criterion)

**C1** - Each of the 15 fixture builders (E, F(n), Z(n), Hk, Hf, Hd(d), V(n), V0(n), K(n), Kt(n,t), T(d,p), Vt(n,t), Vr(n), X, Y) yields the L, D (bytes 64..68), R (bytes 4..8) and oracle class of the truth table; T(1000,400) gives D 4000, L 1668, W 1600, REPAIRABLE; T(400,400) D 1600, L 1668, VALID; T(400,1000) D 1600, L 4068, W 4000, MISMATCH; K(1000) D 0, L 4068, REPAIRABLE; E and F(n) assert only length and covered prefix (supporting)
Proof: run m7 | session::m7_tests::m7_s0_fixture_builders_match_the_truth_table_oracle ... ok
Recipe: C, test code only; a failure is a defective fixture, never a red or a green.

**C2** - The `RecoverIo` test folder fails a second `try_lock` while the first is held, frees it when the attempt ends or is declared killed, calls `barrier(point)` before and after each of the 11 operations (`try_lock`, `read_manifest`, `write_manifest_tmp`, `rename_manifest`, `open_track`, `create_new_tmp`, `write`, `sync`, `rename`, `read_file`, `exists`), can drop the last unsynced `rename` and can tear a 4-byte write at a chosen byte (supporting)
Proof: run m7 | session::m7_tests::m7_s0_recover_io_double_models_lock_barriers_and_faults ... ok
Recipe: C, test double only; no product claim.

**C3** - The `ChildIo` test double spawns a child with a pid, a piped input and `wait`, `send` delivers one line, `kill_caller` drops the caller's lock and pipe end so the child's read returns end of input, and every manifest, track and transcript writer is recorded (supporting)
Proof: run m7 | session::m7_tests::m7_s0_child_io_double_models_spawn_send_and_caller_death ... ok
Recipe: C, test double only.

**C4** - At the S0 commit the non-test diff against `c2d7b6c` holds only signatures, unused stubs and call sites (`classify` stub returning UNREADABLE, `run_pipeline` taking the spawn as a closure, the per-track read extracted from `transcribe`, `recover_session(dir, io)` forwarding to `std::fs`) and the 124 existing native tests pass (supporting, plan S0)
Proof: walk - S0 record: `git diff --stat c2d7b6c6294c9fe67709cc6c2b1c1650fdff5de1 <S0>` of non-test lines, the S0 SHA and the full native run at S0, kept in the evidence folder before any S row's red
Recipe: C at S0; if the existing suite is not green at S0 the S0 commit is wrong and no S red runs.

**C5** - The five existing controls stay green at S0 and at `HEAD`: a valid WAV stays byte-identical, a repair changes only the header sizes, the CLI repairs a session whose owner is gone, a dead pipeline puts its session back to `captured`, a live owner refuses `recover` and `process` with every byte kept (supporting, plan Feasibility inventory)
Proof: run native | session::tests::recover_leaves_a_valid_wav_byte_identical ... ok
Proof: run native | session::tests::recover_fixes_only_the_header_sizes_and_keeps_every_sample_byte ... ok
Proof: run native | session::tests::recover_through_the_cli_repairs_a_session_whose_owner_is_gone ... ok
Proof: run native | session::tests::a_dead_pipeline_puts_its_session_back_to_captured ... ok
Proof: run native | session::tests::recover_and_process_through_the_cli_refuse_a_live_owner_and_leave_every_byte ... ok
Recipe: C; owner Dani keeps them green; a fixture of these tests is not edited to pass.

### S1 - recover classifies, skips and refuses (AC 1-13; AC 29 and 30 built here first, plan tag S6)

**C6** - `classify` returns the class of each of the 16 rows of the boundary table, including L 4294967303 with D 0 as REPAIRABLE and L 4294967304 as UNREADABLE through a virtual reader, and writes nothing (AC 29)
Proof: run m7 | session::m7_tests::m7_29_every_boundary_row_gives_its_class ... ok
Fail-first: A at S0 - the stub returns UNREADABLE, so every non-UNREADABLE row fails; UNREADABLE rows pass at S0 and are recorded as measured, not as reds.

**C7** - For every class row, R in {0, 60, D + 60} gives the oracle class: R alone never changes the class (AC 29)
Proof: run m7 | session::m7_tests::m7_29_riff_size_alone_never_changes_the_class ... ok
Fail-first: A at S0 - the oracle class differs from the stub's UNREADABLE on every non-UNREADABLE row.

**C8** - The M7 case: mic K(1000) and system E on an `interrupted` session with a dead owner -> exit 0, only bytes 4..8 and 64..68 of `mic.wav` change, mic reads 1000 samples with sample bytes identical, `system.wav` stays 0 bytes with SHA-256 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`, state `captured`, `error` null, stdout holds `system.wav: empty (0 bytes), no audio captured, skipped` (AC 1)
Proof: run m7 | session::m7_tests::m7_1_recover_skips_an_empty_system_track_and_repairs_mic ... ok
Proof: run m7 | session::m7_tests::m7_1_recover_prints_that_the_empty_system_track_is_skipped ... ok
Fail-first: U at source, recipe row 1 - predicted exit 1, state `interrupted`, `mic.wav` already changed.
Control: mic K(1000) + system K(500) passes at source (C12).

**C9** - System F(1), F(43), F(44), F(63), F(67) with mic K(1000) -> exit 0, `system.wav` byte-identical, stdout holds `system.wav: partial header (<n> bytes), no audio captured, skipped` with n the length (AC 2)
Proof: run m7 | session::m7_tests::m7_2_a_partial_header_is_skipped_and_kept ... ok
Fail-first: U at source, recipe row 2 - predicted F(1..43) exit 1 `too short`, F(44..63) exit 1 `no data chunk`, F(64..67) a write into the file.
Control: F(68) is HEADER-ONLY (C11).

**C10** - System Z(1), Z(30), Z(67) (bytes that disagree with the fixed fields) -> exit 1 with `system.wav: unreadable (<reason>)`, both tracks byte-identical, no `recover.journal*` file (AC 2, AC 9)
Proof: run m7 | session::m7_tests::m7_2_a_short_non_header_file_is_unreadable ... ok
Fail-first: A at source, recipe row 2 - exit 1 and identical bytes already hold (control part); only the text `unreadable (` is absent.

**C11** - System Hk, Hf and Hd(4294967295) at 68 bytes, and a 70-byte product header -> stdout `system.wav: no samples, skipped`, each byte-identical, exit 0 (AC 3)
Proof: run m7 | session::m7_tests::m7_3_a_header_only_track_is_skipped_and_kept ... ok
Fail-first: U at source, recipe row 3 - Hk predicted red (bytes 4..8 go from 0 to 60); Hf is the control; Hd and the 70-byte file are recorded as measured, not predicted.

**C12** - Both tracks REPAIRABLE (mic K(1000), system K(500)) -> both repaired, only bytes 4..8 and 64..68 of each differ, state `captured`, exit 0 (AC 4)
Proof: run m7 | session::m7_tests::m7_4_two_repairable_tracks_are_both_repaired ... ok
Recipe: C, recipe row 4 - passes at source; existing `recover_through_the_cli_repairs_a_session_whose_owner_is_gone` (C5) is the second control; owner Dani keeps it green.

**C13** - System V0(48000), a 1000-sample audible V(1000), and Vr(400) (RIFF size 0), each with mic K(1000) -> each byte-identical, no line reports it empty, state `captured` (AC 5)
Proof: run m7 | session::m7_tests::m7_5_a_valid_track_silent_audible_or_stale_riff_is_kept ... ok
Recipe: C, recipe row 5 - true at source by construction, no red exists; Vera repeats it in row 18.

**C14** - System T(1000, 400) -> after `recover` D 1600 and R 1660, no sample byte changes, hound reads 400 samples without error (AC 6)
Proof: run m7 | session::m7_tests::m7_6_a_truncated_track_declares_exactly_the_samples_present ... ok
Fail-first: U at source, recipe row 6 - predicted: D stays 4000 and hound fails on the 401st sample.
Control: T(400, 400) byte-identical, reads 400 samples.

**C15** - System Hd(4000) (declares 1000 samples, 68 bytes, none present) -> HEADER-ONLY, byte-identical, no write (AC 6)
Proof: run m7 | session::m7_tests::m7_6_a_declaration_with_no_samples_is_header_only ... ok
Fail-first: U at source - recorded as measured (the source returns early when D > 0; a pass at source is recorded as a control).

**C16** - System Kt(250, 2) -> D' 1000, R' 1060, length 1070 unchanged, the 2 trailing bytes unchanged, 250 samples read, and a second `recover` writes no byte; Vt(250, 2) and K(250) stay as their controls say (AC 7)
Proof: run m7 | session::m7_tests::m7_7_trailing_bytes_stay_and_a_second_recover_writes_nothing ... ok
Fail-first: U at source, recipe row 7 - predicted D written as 1002 and hound refuses it.
Control: Vt(250, 2) byte-identical; K(250) repaired.

**C17** - System X (`data` at offset 40 of 100 bytes of `x`) with mic K(1000) -> both byte-identical, exit 1, `system.wav: unreadable (<reason>)` (AC 8)
Proof: run m7 | session::m7_tests::m7_8_a_non_header_file_with_data_bytes_is_never_written ... ok
Fail-first: U at source, recipe row 8 - predicted bytes 4..8 and 44..48 overwritten.

**C18** - After `recover`, E, F(1), F(64), Hf, Hk and V0 tracks are byte-identical and a REPAIRABLE track differs only at bytes 4..8 and 64..68 (AC 8)
Proof: run m7 | session::m7_tests::m7_8_only_a_repairable_track_is_written_and_only_eight_bytes ... ok
Fail-first: U at source, recipe row 8 - F(64) and Hk predicted red; E, F(1), Hf, V0 are controls. May be green at HEAD after C9 and C11; the frozen red is the evidence.

**C19** - Mic K(1000) with system Y, and with system Z(30) -> exit 1 naming `system.wav` and `unreadable (<reason>)`, both tracks byte-identical, no `recover.journal*` file, `session.json` byte-identical, state `interrupted` (AC 9)
Proof: run m7 | session::m7_tests::m7_9_an_unreadable_track_refuses_the_whole_recovery ... ok
Fail-first: U at source, recipe row 9 - predicted `mic.wav` rewritten before `no data chunk` stops the loop. The least code for this green refuses UNREADABLE only; MISMATCH is C20's green.
Control: mic V0(1000) + system Y, no write needed, passes on bytes.

**C20** - System T(400, 1000) with mic K(1000) -> exit 1 with `system.wav: declared 1600 bytes but 4000 bytes of samples are present`, both byte-identical, no `recover.journal*` file, state `interrupted`; the declaration is neither shortened nor extended (AC 30, AC 9)
Proof: run m7 | session::m7_tests::m7_30_a_mismatch_track_refuses_the_whole_recovery ... ok
Fail-first: U at source, recipe row 30 - predicted exit 0, state `captured`, only 400 of 1000 samples readable.
Control: T(1000, 400) is REPAIRABLE (C14).

**C21** - `repair_wav_in` over a virtual reader of length 4294967304 returns `larger than a WAV header can describe` and issues no write; at length 4294967303 with D 0 the track follows the ordinary rules (AC 10)
Proof: run m7 | session::m7_tests::m7_10_repair_refuses_a_length_above_u32_and_writes_nothing ... ok
Fail-first: S without S0 (generic `pub(crate)` function), recipe row 10 - predicted `(len - 8) as u32` truncates and the writes happen. No real 5 GiB file is created.

**C22** - `recover_session` over the `RecoverIo` test folder with a track of virtual length 4294967304 -> exit 1 `unreadable (larger than a WAV header can describe)`, no write to any track (AC 10)
Proof: run m7 | session::m7_tests::m7_10_recover_refuses_a_track_longer_than_u32 ... ok
Fail-first: S at S0 - predicted the forwarded `repair_wav` path reads or truncates; measured at S0.

**C23** - `recover` reads at most 4096 bytes of a 64 MiB virtual track through a counting reader and never loads a whole track (AC 11)
Proof: run m7 | session::m7_tests::m7_11_recover_reads_at_most_4096_bytes_of_a_track ... ok
Proof: walk - Cyrus reviews by source that no path reads a whole track (recipe row 11 second proof)
Fail-first: S at S0, recipe row 11 - predicted the whole file read by `std::fs::read`.

**C24** - Mic K(1000) -> stdout holds `mic.wav: riff_size 0 -> 4060, data_size 0 -> 4000` in decimal (AC 12)
Proof: run m7 | session::m7_tests::m7_12_recover_prints_the_old_and_new_size_fields ... ok
Fail-first: A at source, recipe row 12 - nothing is printed.

**C25** - Both tracks E, and mic Hk with system E -> exit 0, state `failed`, `error.code` `no_audio`, message `no track holds audio; the files were kept`, every file byte-identical; mic K(1) with system E -> state `captured` (AC 13)
Proof: run m7 | session::m7_tests::m7_13_no_track_with_audio_ends_failed_no_audio ... ok
Fail-first: U at source, recipe row 13 - predicted exit 1, state `interrupted`, `too short`.

### S2 - process finishes or fails without a panic (AC 14-19)

**C26** - `load_16k` (or the per-track read) on E, F(1) and F(67) returns without a panic under `catch_unwind`; Hf returns without a panic (AC 14)
Proof: run m7 | m7_tests::m7_14_load_16k_does_not_panic_on_an_empty_or_partial_track ... ok
Fail-first: U at source, recipe row 14 - predicted panic at `main.rs:443` with `Failed to read enough bytes`; Hf is the control.

**C27** - The per-track read extracted from `transcribe` prints `<file>: no audio, skipped` for 0, 1, 67 and 68 byte tracks and still reads the other track; a VALID track under 1 s keeps `under 1 s of audio, skipped` (AC 14)
Proof: run m7 | m7_s2_tests::m7_14_the_per_track_read_skips_a_track_with_no_audio ... ok
Fail-first: S at S0, recipe row 14 - predicted panic or no skip line.

**C28** - The per-track read on X and on T(1000, 400) returns an error naming the file and does not panic under `catch_unwind` (AC 15)
Proof: run m7 | m7_tests::m7_15_an_unreadable_track_returns_an_error_naming_the_file ... ok
Fail-first: U at source, recipe row 15 - predicted X panics in `expect`, T in `Result::unwrap` at the 401st sample.

**C29** - A track read error inside the pipeline ends the session `captured` with `error.code` `pipeline_error`, every file byte-identical, exit 1 (AC 15)
Proof: run m7 | session::m7_tests::m7_15_a_track_read_error_settles_captured_with_pipeline_error ... ok
Fail-first: S at S0, recipe row 15 (pipeline half is row 16's seam) - predicted a panic that bypasses `pipeline_failed`. Feasibility to confirm at S0: if the pipeline cannot reach the per-track read without a model, this check falls back to Vera's integration row and the disposition goes to Otto before its red.

**C30** - The spawn closure returns status 101 and writes nothing while `session.json` says `processing` -> after `process` the state is `captured`, `error.code` `pipeline_error`, the message names exit status 101, every file kept, exit 1 (AC 16)
Proof: run m7 | session::m7_s2::m7_16_a_failed_child_settles_captured_with_pipeline_error ... ok
Fail-first: S at S0, recipe row 16 - predicted state stays `processing`.

**C31** - A child that already wrote `captured` with `provider_timeout` and exits non-zero keeps `provider_timeout` (AC 16)
Proof: run m7 | session::m7_s2::m7_16_a_child_written_error_code_is_kept ... ok
Recipe: C at S0 - predicted to pass at S0 (`process` does not touch it); recorded as a control.

**C32** - `process <id>` on an `interrupted` session -> exit 1 with `session <id> is interrupted; run recover first`, `session.json` byte-identical, the spawn closure never called (AC 17)
Proof: run m7 | session::m7_s2::m7_17_process_refuses_an_interrupted_session ... ok
Fail-first: S at S0, recipe row 17 - predicted the child is spawned and sets `processing`.

**C33** - `process <id>` on an `interrupted` session with `recover.journal`, and separately with `recover.journal.tmp`, present -> exit 1 with `session <id> has a recovery pending; run recover first`, nothing changed, the closure never called (AC 17)
Proof: run m7 | session::m7_tests::m7_17_process_refuses_a_pending_recovery ... ok
Fail-first: A at S0 - the message does not exist.

**C34** - `process <id>` on a `captured` session with a leftover `recover.journal` -> runs the pipeline (the closure is called) (AC 17)
Proof: run m7 | session::m7_tests::m7_17_a_leftover_record_does_not_stop_process_outside_interrupted ... ok
Recipe: C at S0 - passes at S0 (process ignores the name); kept green after C33.

**C35** - Mic K(n) from a synthetic Portuguese speech WAV and system E, `recover` then `process` with no Anthropic key and an `APPDATA` without secrets -> exit 0, `transcript.md` holds only `Você` lines and no `Outros` line, state `completed` (AC 18)
Proof: walk - Vera, integration row 18 on built bytes with the pinned model, inside the execution gate; not run by the author
Recipe: I, owner Vera - predicted at source: `recover` exits 1.

**C36** - The captured stdout and stderr of every message row of AC 9, 10, 15, 32 to 37 and 41 to 44 and every printed line of AC 1 to 3, 12 and 14 hold no fixture sample byte, no `header` hex of the record, no transcript text and no path outside the session folder (AC 19)
Proof: run m7 | session::m7_tests::m7_19_no_message_carries_audio_header_hex_transcript_or_an_outside_path ... ok
Proof: walk - Rita checks by source that no message formats audio or transcript text
Recipe: C, recipe row 19 - not red-producible (the lines do not exist at source); written with the new lines.

### S3 - the recorder writes its header through at open (AC 20)

**C37** - `write_wav` over a shared-buffer writer that receives no chunk -> within 2 s the underlying writer holds 68 bytes with `data` at 60..64 (AC 20)
Proof: run m7 | m7_tests::m7_20_the_header_reaches_the_file_before_the_first_chunk ... ok
Fail-first: U at source, recipe row 20 - predicted buffer length 0 (`BufWriter` holds the header).

### S4 - nothing is lost (AC 21-23)

**C38** - For each class pair, the byte map of the session folder before and after `recover` differs only in the 8 recover names (`session.json`, `.session.json.tmp`, `recover.lock`, `recover.journal.tmp`, `recover.journal`, `recover.done.json`, `recover.journal.rejected`, and bytes 4..8 and 64..68 of a REPAIRABLE track) (AC 21)
Proof: run m7 | session::m7_cp4::m7_21_recover_changes_only_its_allowlisted_names_and_eight_bytes ... ok
Fail-first: U at source, recipe row 21 - predicted red for the refused case (row 9) and killed Hk (row 3); the new names are feature-absent. May be green at HEAD after C11 and C19; the frozen red is the evidence.

**C39** - The byte map before and after `process` (spawn closure writing a transcript) differs only in the 8 process names `transcript.md`, `ata.md`, `minutes.json`, `note.path`, the `.url` shortcut, `session.json`, `.session.json.tmp`, `recover.lock` (AC 21)
Proof: run m7 | session::m7_s2::m7_21_process_changes_only_its_allowlisted_names ... ok
Recipe: C at S0 for today's names; `recover.lock` is feature-absent until C64.

**C40** - Across every S6 schedule and fault row, the op log of the `RecoverIo` and `ChildIo` doubles holds no delete and no rename onto an existing name for the 5 retained names (`recover.lock`, `recover.journal.tmp`, `recover.journal`, `recover.done.json`, `recover.journal.rejected`); a leftover `.session.json.tmp` is overwritten and never read as data; a leftover `recover.journal.tmp` is read only as a candidate record (AC 21)
Proof: run m7 | session::m7_tests::m7_21_no_retained_recovery_file_is_deleted_or_replaced ... ok
Proof: walk - Rita checks by source that no `remove_file`, `remove_dir` or replacing rename targets those names
Fail-first: S at S0 - feature-absent for the record names; the leftover-tmp half is red where today's code writes the tmp manifest without reading it (control).

**C41** - `recover` twice on mic K(1000) + system E -> second run exit 0, no track byte changes, `recover.done.json` byte-identical (not rewritten) (AC 22)
Proof: run m7 | session::m7_cp4::m7_22_a_second_recover_changes_nothing ... ok
Fail-first: U at source, recipe row 22 - predicted the first run fails. Control: K(1000) + K(500) twice keeps the bytes today.

**C42** - With a live owner, `recover` and `process` refuse with `session is active: <dir>` and change nothing, and a refused attempt creates at most the 0-byte `recover.lock` (AC 23)
Proof: run native | session::tests::recover_and_process_through_the_cli_refuse_a_live_owner_and_leave_every_byte ... ok
Proof: run m7 | session::m7_tests::m7_23_a_refused_attempt_creates_at_most_an_empty_lock ... ok
Recipe: C, recipe row 23 - true today; the lock half is feature-absent until C64.

### S6 - one owner, a crash-safe repair and an honest settle (AC 31-44)

**C43** - Two `recover` attempts on one session through the `try_lock` model -> the second exits 1 with `session <id> is being recovered or processed by another meetrec; no session file was changed`, the folder's bytes are unchanged except a first-time 0-byte `recover.lock`, no retry and no wait (AC 41)
Proof: run m7 | session::m7_tests::m7_41_a_second_recover_is_refused_by_the_lock ... ok
Fail-first: S at S0, recipe row 41(a) - predicted the second attempt simply runs.

**C44** - `process` holding the lock while its child runs -> a `recover` of the same session exits 1 with the lock message; two `process` of one session -> the second exits 1 with the lock message (AC 41)
Proof: run m7 | session::m7_tests::m7_41_process_holds_the_lock_against_recover_and_process ... ok
Fail-first: S at S0, recipe row 41(a).

**C45** - The lock is taken again after the first attempt ends and after it is declared killed; a `recover.lock` left by an earlier run with no holder is taken at once; the op log shows no read of `recover.lock` content and no pid read for the decision (AC 41)
Proof: run m7 | session::m7_tests::m7_41_a_free_or_stale_lock_is_taken_at_once ... ok
Fail-first: S at S0 - feature-absent (no lock exists).

**C46** - When the lock is held by a running `recover` or `process` and the manifest names a live owner, the lock refusal comes first and carries the AC 41 message (AC 23, AC 41)
Proof: run m7 | session::m7_tests::m7_23_the_lock_refusal_comes_before_the_live_owner_refusal ... ok
Fail-first: S at S0 - predicted `session is active: <dir>` (the order does not exist).

**C47** - `recover.lock` is a directory; `recover.lock` is a read-only file whose write-open fails; `recover.lock` held open by a non-meetrec handle -> exit 1 `cannot lock the session: <error>`, no further step, no retry, no track, record or manifest byte changed (AC 41)
Proof: run m7 | session::m7_tests::m7_41_a_lock_that_cannot_be_opened_stops_every_step ... ok
Fail-first: U at source on the real filesystem (directory and read-only members; the attribute behavior is unverified on this host) - predicted the source ignores the name and runs. The foreign-handle member is open question Q1.

**C48** - Real handles on the Windows host: two opens of `recover.lock` with share mode none in one test process -> the second fails with a sharing violation; after the first handle closes, an open succeeds (AC 41, row 41(b) b1, through the product's lock function)
Proof: run m7 | session::m7_tests::m7_41_a_second_share_none_open_of_the_lock_fails_on_windows ... ok
Fail-first: A at S0 - the lock function is absent at source.

**C49** - The Windows gate b2 to b4 of row 41(b) holds by its recorded status: Vera's isolated run at 14:40:11Z, Dale's assessment, Sammy's corrected b2 via Parker `779a592f-e8ef-4e9a-9c1f-f25cb51a957e`, artifact `VERA-RAW-observed-1442Z.txt` SHA-256 `bb0f4917f5995cdac37e0dfc94d04ecab187076841253bd25dfe54bf6f420855`; no second probe is run (AC 41)
Proof: walk - read the recorded artifacts and hash under `Shared/evidence/otto-m7-windows-feasibility-20261109`; product-level handoff on built bytes is Vera's integration row
Recipe: H - recorded gate status; the plan authorizes no second probe and 7b is not selected.

**C50** - Mic K(1000), system K(500) with the read-only attribute on `system.wav` -> exit 1 naming `system.wav`, both byte-identical, no `recover.journal*` file, state `interrupted`; each repairable track is opened for writing once and closed before the first journal byte (AC 31)
Proof: run m7 | session::m7_tests::m7_31_a_track_that_cannot_be_opened_stops_before_any_write ... ok
Fail-first: U at source, recipe row 31 - predicted `mic.wav` repaired first. Control: both writable (C12). Attribute handling unverified on this host.

**C51** - Mic K(1000), system E, `recover.journal.tmp` created as a directory -> exit 1 with `cannot persist the recovery record: <error>`, `mic.wav` byte-identical, state `interrupted` (AC 32)
Proof: run m7 | session::m7_tests::m7_32_a_record_that_cannot_be_created_stops_before_any_track_write ... ok
Fail-first: U at source, recipe row 32 - predicted the source repairs `mic.wav` and exits 0.

**C52** - A fresh run's op log shows `create_new(recover.journal.tmp)`, write, flush, sync and rename to `recover.journal` before the first track write; the record is one UTF-8 JSON object of at most 2048 bytes holding `schema` 1, the session id, `prior_state` read under the lock, and one entry per REPAIRABLE track with `file`, `length`, `header` (136 lowercase hex characters equal to the original bytes 0..68), `riff_size` R' and `data_size` D'; an EMPTY track has no entry (AC 32, AC 39)
Proof: run m7 | session::m7_tests::m7_32_the_record_is_published_before_any_track_byte ... ok
Fail-first: S at S0 - feature-absent (no record exists).

**C53** - Journal persisted, then the attempt stops before any track write -> both tracks byte-identical and the journal describes the original headers (AC 32)
Proof: run m7 | session::m7_tests::m7_32_a_stop_after_the_record_leaves_tracks_identical ... ok
Fail-first: S at S0 - feature-absent.

**C54** - Each of the 5 temporary-record faults - (i) the write fails after 100 bytes, (ii) the sync of an incomplete tmp fails, (iii) the sync of a fully formed record errors, (iv) the rename fails after a good sync, (v) `recover.journal.rejected` occupied when the next run needs it - gives exit 1 `cannot persist the recovery record: <error>`, no track byte, `session.json` byte-identical, the tmp kept byte-identical; the next run promotes (iii) and (iv), sets (i) and (ii) aside as `recover.journal.rejected` and completes, and in (v) exits 1 naming both files and replaces neither (AC 32)
Proof: run m7 | session::m7_tests::m7_32_every_temporary_record_fault_keeps_the_tmp_and_the_next_run_resolves_it ... ok
Fail-first: S at S0, plan "Temporary-record failure recipes" - all absent at source.

**C55** - A fresh repair writes R' then D' of each REPAIRABLE track with one 4-byte write at offset 4 and one at offset 64, then flushes, syncs and reads the track back as VALID, track by track (AC 33)
Proof: run m7 | session::m7_tests::m7_33_each_track_gets_two_four_byte_writes_then_sync_then_read_back ... ok
Fail-first: S at S0 - predicted the forwarded `repair_wav` reads the whole file and writes without the read-back.

**C56** - A writer failing at write 1, at write 2, a short write of 2 of 4 bytes, a failing sync, and a failure between the two tracks -> exit 1 naming the file, each header byte original or intended, sample bytes identical, state `interrupted`, the journal pending, the other track not changed further (AC 33)
Proof: run m7 | session::m7_tests::m7_33_every_apply_fault_leaves_original_or_intended_bytes_and_a_pending_record ... ok
Fail-first: S at S0, recipe row 33 - the journal and exit parts fail (no journal; no seam at source). Unit-proven only, not forced on the host.

**C57** - With `recover.journal` pending: mic at intended and system at original bytes -> completes, both VALID, samples identical; a torn field (2 of 4 bytes intended) -> completes; a track longer than the record's length -> exit 1 naming the file and the record, nothing written; a record with `file` `..\x` -> refused, nothing written; a 3000-byte record -> refused, nothing written; the values written are recomputed from the current length (AC 34)
Proof: run m7 | session::m7_tests::m7_34_a_pending_record_is_resumed_only_when_every_track_matches ... ok
Fail-first: A at S0, recipe row 34 - predicted the source ignores the journal and repairs by itself.

**C58** - In every outcome of C57 the record's bytes (SHA-256) are unchanged until its rename to `recover.done.json` (AC 34)
Proof: run m7 | session::m7_tests::m7_34_the_record_bytes_never_change_before_the_done_rename ... ok
Fail-first: S at S0 - feature-absent.

**C59** - After a successful recovery there is no `recover.journal` and one `recover.done.json` of at most 2048 bytes; the absence of `recover.done.json` was checked before the first track byte; a second `recover` leaves it byte-identical (AC 35)
Proof: run m7 | session::m7_tests::m7_35_a_committed_recovery_leaves_one_done_record ... ok
Fail-first: A at source, recipe row 35 - no file exists.

**C60** - The done rename fails after the manifest commit -> exit 0 with `recovery record not renamed: <error>`; the next `recover` renames it and leaves `session.json` byte-identical (AC 35)
Proof: run m7 | session::m7_tests::m7_35_a_failed_done_rename_is_finished_by_the_next_recover ... ok
Fail-first: S at S0 - feature-absent.

**C61** - Mic K(1000), system E with `.session.json.tmp` as a directory -> exit 1 `session.json not updated: <error>`, no line says the session is settled, the previous `session.json` in place, tracks VALID with identical samples, the journal pending; after the cause is moved aside by rename (no delete) the retry exits 0, state `captured`, and no track byte already at its intended value is written again (AC 36)
Proof: run m7 | session::m7_tests::m7_36_a_failed_manifest_write_keeps_the_record_pending_and_the_retry_completes ... ok
Fail-first: U/A at source, recipe row 36 - exit 1 and the stale manifest hold at source (control part); the message, the journal and the clean retry are feature-absent.

**C62** - Same as C61 with `session.json` read-only, so the rename over it fails on Windows -> same outcome; after the attribute is cleared the retry completes (AC 36)
Proof: run m7 | session::m7_tests::m7_36_a_read_only_manifest_keeps_the_record_pending_and_the_retry_completes ... ok
Fail-first: U/A at source, recipe row 36 - the read-only rename failure is unverified on this host.

**C63** - The spawn closure returns 101 and `session.json` is read-only -> exit 1 with `session.json still says processing: <error>`, no line reports `captured`, the state on disk unchanged, files identical; a later `process` runs the pipeline again (AC 37)
Proof: run m7 | session::m7_tests::m7_37_a_failed_settle_is_reported_and_the_next_process_reconciles ... ok
Fail-first: S at S0, recipe row 37 - predicted exit 1 with no message about the state (`fail` ignores write errors); the retry half passes today.

**C64** - Each of the 10 barriers of "Scheduled interleavings", with attempt A paused there and attempt B started -> B exits 1 with the AC 41 message, the folder's bytes equal A's at that barrier (a first-time empty `recover.lock` aside), the record's `header` hex equals the original headers, no `recover.journal*` or `recover.done.json` file is removed or replaced, sample bytes identical (AC 38)
Proof: run m7 | session::m7_tests::m7_38_a_second_recover_at_every_barrier_is_refused_and_changes_nothing ... ok
Fail-first: S at S0, recipe row 38 - absent (no lock, no record); deterministic barrier hooks, no threads.

**C65** - Each of the 10 barriers with attempt A killed there and B started -> B ends with the final bytes of a single uninterrupted run (or a refusal naming the cause), tracks VALID with D = W, the `header` hex equals the original headers, the record never rewritten or replaced, sample bytes identical (AC 38)
Proof: run m7 | session::m7_tests::m7_38_a_recover_after_a_kill_at_every_barrier_ends_as_one_run ... ok
Fail-first: S at S0, recipe row 38.

**C66** - Records with an extra field, an unknown file name, a 130-character `header`, a `prior_state` that is not a state name, a `session` that differs from the folder's id, and a `data_size` that differs from the recomputed value are refused with nothing written; a leftover `recover.journal.tmp` whose `session` differs is set aside under AC 42(d) (AC 39)
Proof: run m7 | session::m7_tests::m7_39_a_record_outside_the_fixed_schema_is_refused ... ok
Proof: walk - Cyrus reviews the schema and the 2048-byte limit by source
Fail-first: A at S0, recipe row 39 - no journal is read at source.

**C67** - No value read from a record is used as a path, a length to allocate or a value to write: the op log's paths are only the session folder's fixed names, the read is capped at 2048 bytes, and every written value equals the one recomputed from the file length (AC 39)
Proof: run m7 | session::m7_tests::m7_39_no_record_value_is_used_as_a_path_length_or_written_value ... ok
Fail-first: S at S0 - feature-absent.

**C68** - Each leftover-tmp case of AC 42 -> (i) a synced valid tmp whose rename was lost is promoted and completes with `header` equal to the original headers; (iii) a tmp cut at 100 bytes and (iv) a tmp whose `header` differs outside the two fields are renamed to `recover.journal.rejected` with bytes kept and the ordinary rules follow; (v) `recover.journal.rejected` present -> exit 1 naming both files, nothing written; (vi) tmp is a directory -> exit 1 `cannot persist the recovery record: recover.journal.tmp is not a file`; (vii) `recover.done.json` present with mic K(1000) -> exit 1 `recover.done.json exists; it is not replaced`, file byte-identical; (viii) `recover.journal` and tmp both present -> resumed from `recover.journal`, tmp untouched (AC 42)
Proof: run m7 | session::m7_tests::m7_42_a_leftover_tmp_is_promoted_or_set_aside_never_deleted ... ok
Fail-first: S at S0, recipe row 42 - all absent at source.

**C69** - A record with `prior_state` `interrupted` against a manifest that is `interrupted` -> committed `captured`; against `captured`, `completed`, `processing` with a dead owner, and `failed` with a later `error` -> `session.json` byte-identical, `session <id> is <state>; session.json left as it is`, the record renamed to `recover.done.json` when the tracks are VALID with their intended values, exit 0; `completed` with a track still at original bytes -> exit 1, nothing written, record untouched (AC 43)
Proof: run m7 | session::m7_tests::m7_43_the_manifest_is_committed_only_from_the_prior_state ... ok
Fail-first: S at S0, recipe row 43 - predicted the source resets any state to `captured` and clears `error` (a defect red for the reset).

**C70** - The caller writes to the child's standard input exactly the 6 bytes `6f 77 6e 65 64 0a` (`owned` followed by one LF), once, and only after `settle` persisted `processing`, the child's pid and `error` null; never the 7 bytes of `owned` with a literal backslash and `n`, and never the 5 unterminated bytes `owned` (AC 44 steps 1 and 3)
Proof: run m7 | session::m7_tests::m7_44_the_caller_sends_exactly_six_bytes_after_the_persist ... ok
Fail-first: S at S0, recipe row 44 - absent (no pipe, no handoff at source).

**C71** - The child reads at most 8 bytes of its input to find the LF and accepts the 6-byte line `owned` + LF, whole and when delivered as `own` then `ed` + LF, then reads `session.json`, finds `processing` with its own pid, and only then runs the pipeline (AC 44 steps 2 and 4)
Proof: run m7 | session::m7_tests::m7_44_the_child_accepts_only_owned_and_lf_within_eight_bytes ... ok
Fail-first: A at S0 - the child reads no input at source.

**C72** - Each of the 8 step-2 inputs - a 9-byte line, `owned` with no terminator then end of input, `owned` CR LF, `OWNED` LF, an empty line, end of input with no byte (also the child run by hand without a caller), a read error, and 64 bytes with no LF - makes the child exit 1 with `pipeline not started by process: no ownership handed over`, having read no session file, written nothing, consumed at most 8 input bytes and not entered `pipeline_failed`; the byte map of the whole session folder is identical before and after (AC 44 step 2)
Proof: run m7 | session::m7_tests::m7_44_every_bad_handoff_line_is_refused_before_any_effect ... ok
Fail-first: A at S0 - absent at source (the child publishes its own owner and runs at once).

**C73** - After it read `owned` + LF, a child that finds `session.json` not `processing`, or naming another pid, exits 1 with `pipeline not started by process: session.json does not name this pipeline` and touches nothing (AC 44 step 4)
Proof: run m7 | session::m7_tests::m7_44_a_child_not_named_by_the_manifest_refuses ... ok
Fail-first: A at S0.

**C74** - The persist at step 3 fails -> the caller closes the pipe without writing, waits for the child's refusal and exits 1 with `cannot hand the session to the pipeline: <error>`; no transcript, manifest unchanged (AC 44 step 3)
Proof: run m7 | session::m7_tests::m7_44_a_failed_persist_hands_nothing_to_the_child ... ok
Fail-first: S at S0 - absent.

**C75** - Each cell of the 7 process handoff schedules (B while P and C live: 7; P killed: 6, schedule 6 not applicable) -> at most one writer of `session.json`, the transcript and the note at any instant; C writes nothing before it read `owned` and found its pid; C never opens `recover.lock`; a record, when present, byte-identical; a `recover` by B writes no track byte while C has acted or can still act (AC 44)
Proof: run m7 | session::m7_tests::m7_44_every_handoff_schedule_keeps_one_writer ... ok
Fail-first: S at S0, recipe row 44.

**C76** - `stop`'s own pipeline run, which shares `run_pipeline`, hands off the same way: its child receives the 6 bytes only after the persist and refuses without them (AC 44)
Proof: run m7 | session::m7_tests::m7_44_stop_hands_off_through_the_same_run_pipeline ... ok
Fail-first: S at S0 - absent.

**C77** - `docs/INSTALL.md` section `## Windows` names `recover.lock`, `recover.journal`, `recover.done.json`, `recover.journal.tmp`, `recover.journal.rejected`; says the first is empty and the others hold 68 bytes of header per repaired track and no audio, and that the product never deletes them; states the failure model (samples never written; `recover` finishes after a killed or failed run; nothing promised after a power loss, a header can be left partly written); gives `D = 4 * floor((L - 68) / 4)`, `R = 60 + D`, little-endian u32 at offsets 64..68 and 4..8, and the restore from the record's original values (AC 40)
Proof: run m7 | m7_tests::m7_40_install_md_names_the_recovery_files_and_the_manual_repair ... ok
Proof: walk - Rita checks the wording against "Failure model"
Fail-first: A at source, recipe row 40 - absent. The plan's `INSTALL.md` is the repository's `docs/INSTALL.md` (`## Windows` at line 370 of the source).

### S5 - verification and immutability (AC 24-28)

**C78** - Tag `v0.2.0-rc3` at `889d48c9a2cca7f61a0648b832c3c3ac20953c78` and tag `v0.2.0-rc4` at `c2d7b6c6294c9fe67709cc6c2b1c1650fdff5de1` and their draft assets are unchanged, read with `gh-dev` (AC 24)
Proof: walk - Vera reads `git ls-remote` and the draft releases with `gh-dev`
Recipe: H, owner Vera - a read, not a red.

**C79** - The native suite (124 existing tests by static count at `c2d7b6c`, plus the new `m7_` tests) and the tray suite are green (AC 25)
Proof: run native | test result: ok.
Proof: run tray | test result: ok.
Recipe: C, recipe row 25 - Vera's counts, not re-run here; no red exists.

**C80** - No file changes under `apps/tray`, `packages/`, `.github/workflows` or the packaging scripts `scripts/windows/build-installer.ps1` and `scripts/windows/installer-smoke.ps1`, and the dictation, overlay, `minutes` and `models` tests stay green in the native run (AC 25)
Proof: `git diff --exit-code --stat c2d7b6c6294c9fe67709cc6c2b1c1650fdff5de1 HEAD -- apps/tray packages .github scripts/windows/build-installer.ps1 scripts/windows/installer-smoke.ps1`
Recipe: C.

**C81** - Every check of S1, S2, S4 and S6 that protects a recording has a committed test whose failing run at its base (source SHA or recorded S0) was measured and kept before the production line, and a passing run after it; predicted outcomes are not quoted as results (AC 26)
Proof: walk - the red record per check (base SHA, module hash, command, exit, failing set, UTC) in the evidence folder precedes the production commit; Rita and Vera check it exists
Recipe: H, owner Dani produces; it cannot be rebuilt afterwards.

**C82** - Rita reviews the diff independently and Vera executes the frozen recipes and the integration rows on built bytes; Dani does not review or verify (AC 27)
Proof: walk - process rule, owners Rita and Vera
Recipe: H.

**C83** - A change to shipped executable bytes ships only as a new tag chosen by Sammy on a new reviewed SHA built by CI into a draft release; rc4 evidence stays valid for rc4 bytes only (AC 28)
Proof: walk - tag and CI rule, owner Sammy; Vera verifies bytes
Recipe: H.

### S6 supplement - order and refusal text checks shared by several criteria

**C84** - The `riff_size` and `data_size` printed for a repair are the decimal u32 values written, and the values in the record equal them (AC 12, AC 32)
Proof: run m7 | session::m7_tests::m7_12_the_printed_values_equal_the_written_and_recorded_values ... ok
Fail-first: A at S0 - absent.

**C85** - Every refusal of the lock, the record, the leftover files and the manifest exits 1 and names its file by name only (`recover.lock`, `recover.journal`, `recover.journal.tmp`, `recover.journal.rejected`, `recover.done.json`, `session.json`) (AC 19, AC 41, AC 42)
Proof: run m7 | session::m7_tests::m7_19_every_refusal_names_its_file_by_name_only ... ok
Recipe: C - written with the new lines; not red-producible.

**C86** - The 8-byte bound is a bound on what the child reads, not on the line: the counting input of C72 shows at most 8 bytes consumed for each of its 8 inputs (AC 44 step 2)
Proof: run m7 | session::m7_tests::m7_44_the_child_reads_at_most_eight_bytes ... ok
Fail-first: A at S0 - absent.

**C87** - A refused `recover` or `process` (lock held, live owner, pending record, unreadable or mismatch track) leaves `session.json` byte-identical and creates no file other than a first-time 0-byte `recover.lock` (AC 9, AC 23, AC 41)
Proof: run m7 | session::m7_tests::m7_41_every_refusal_leaves_the_manifest_and_creates_only_the_lock ... ok
Fail-first: S at S0 - the lock part is absent; the manifest part holds at source for the live owner (control).

## Coverage

| Set (size) | Member -> proof | Unproven |
| --- | --- | --- |
| plan criteria (44) | AC1 C8 · AC2 C9 C10 · AC3 C11 · AC4 C12 · AC5 C13 · AC6 C14 C15 · AC7 C16 · AC8 C17 C18 · AC9 C19 C20 C87 · AC10 C21 C22 · AC11 C23 · AC12 C24 C84 · AC13 C25 · AC14 C26 C27 · AC15 C28 C29 · AC16 C30 C31 · AC17 C32 C33 C34 · AC18 C35 · AC19 C36 C85 · AC20 C37 · AC21 C38 C39 C40 · AC22 C41 · AC23 C42 C46 C87 · AC24 C78 · AC25 C79 C80 · AC26 C81 · AC27 C82 · AC28 C83 · AC29 C6 C7 · AC30 C20 · AC31 C50 · AC32 C51 C52 C53 C54 C84 · AC33 C55 C56 · AC34 C57 C58 · AC35 C59 C60 · AC36 C61 C62 · AC37 C63 · AC38 C64 C65 · AC39 C52 C66 C67 · AC40 C77 · AC41 C43 C44 C45 C46 C47 C48 C49 C85 C87 · AC42 C68 C85 · AC43 C69 · AC44 C70 C71 C72 C73 C74 C75 C76 C86 | - |
| pre-change failure recipe rows (44) | row 1 C8 · row 2 C9 C10 · row 3 C11 · row 4 C12 · row 5 C13 · row 6 C14 · row 7 C16 · row 8 C17 C18 · row 9 C19 · row 10 C21 · row 11 C23 · row 12 C24 · row 13 C25 · row 14 C26 C27 · row 15 C28 C29 · row 16 C30 · row 17 C32 · row 18 C35 · row 19 C36 · row 20 C37 · row 21 C38 · row 22 C41 · row 23 C42 · row 24 C78 · row 25 C79 · row 26 C81 · row 27 C82 · row 28 C83 · row 29 C6 · row 30 C20 · row 31 C50 · row 32 C51 C54 · row 33 C56 · row 34 C57 · row 35 C59 · row 36 C61 C62 · row 37 C63 · row 38 C64 C65 · row 39 C66 · row 40 C77 · row 41(a) C43 C44 C45 · row 41(b) C48 C49 · row 42 C68 · row 43 C69 · row 44 C70 C75 | - |
| track classes (7) | EMPTY C8 · FRAGMENT C9 · UNREADABLE C10 C17 C19 · HEADER-ONLY C11 C15 · VALID C13 · REPAIRABLE C14 C16 · MISMATCH C20 | - |
| boundary table rows (16) | C6, table-driven over all 16 rows; R variation C7 | - |
| fixture builders (15) | C1, table-driven over all 15 | - |
| S0 seam parts (5) | `classify` stub C4 C6 · spawn closure C4 C30 · per-track read C4 C27 · `RecoverIo` forwarding and double C2 C4 · `ChildIo` double C3 | - |
| scheduled interleavings of row 38 (20 cells) | C64 and C65, table-driven over all 20 (10 barriers paused, 10 barriers killed) | - |
| process handoff schedules of row 44 (13 cells) | C75, table-driven over all 13 (7 while live, 6 killed; schedule 6 killed not applicable) | - |
| step-2 inputs of AC 44 (9) | `owned` LF accepted C70 C71 · 9-byte line C72 · `owned` unterminated C72 · `owned` CR LF C72 · `OWNED` LF C72 · empty line C72 · end of input C72 · read error C72 · 64 bytes with no LF C72 C86 | - |
| temporary-record faults of AC 32 (5) | (i) write after 100 bytes C54 · (ii) incomplete sync C54 · (iii) formed record sync error C54 · (iv) rename after sync C54 · (v) rejected occupied C54 | - |
| apply faults of AC 33 (5) | write 1 C56 · write 2 C56 · short 2 of 4 C56 · sync C56 · between tracks C56 | - |
| resume cases of AC 34 (5) | intended plus original C57 · torn field C57 · longer track C57 · `..\x` name C57 · 3000-byte record C57 | - |
| leftover-tmp cases of AC 42 (7) | (i) promote C68 · (iii) cut C68 · (iv) header differs C68 · (v) rejected present C68 · (vi) directory C68 C51 · (vii) done present C68 · (viii) journal and tmp C68 | - |
| manifest states of AC 43 (6) | `interrupted` C69 · `captured` C69 C60 · `completed` C69 · `processing` dead owner C69 · `failed` later error C69 · `completed` with original track C69 | - |
| record rejections of AC 39 (6) | extra field C66 · unknown file C66 · 130-character hex C66 · bad `prior_state` C66 · other `session` C66 · value differs C66 C67 | - |
| lock outcomes of AC 41 (9) | recover vs recover C43 · process vs recover C44 · process vs process C44 · released at end C45 · released at kill C45 · stale lock C45 · directory C47 · read-only C47 · foreign handle C47 (Q1) | - |
| Windows gate of row 41(b) (4) | b1 C48 · b2 C49 · b3 C49 · b4 C49 | - |
| manifest faults of AC 36 (2) | `.session.json.tmp` directory C61 · `session.json` read-only C62 | - |
| recover allowlist of AC 21 (8) | C38, table-driven over all 8 | - |
| process allowlist of AC 21 (8) | C39, table-driven over all 8 | - |
| retained names never deleted (5) | C40, table-driven over all 5 | - |
| forbidden change areas of AC 25 (4) | `apps/tray` C80 · `packages/` C80 · workflows C80 · packaging scripts C80 | - |
| pipeline callers of `run_pipeline` (2) | `process` C70 · `stop` C76 | - |

## Swept

- validation: C6, C10, C17, C66, C72
- failure modes: C54, C56, C61, C62, C63, C74
- idempotency: C16, C41, C59, C60
- authorization: C42, C43, C72, C73
- concurrency: C44, C64, C65, C75
- data lifecycle: C40, C59, C68
- dependency failure: C47, C51, C61
- state transitions: C25, C30, C32, C69
- observability: C24, C36, C85

## Open questions

| # | Question | Effect |
| --- | --- | --- |
| Q1 | AC 41 asks for `cannot lock the session: <error>` when `recover.lock` is held by a program that is not meetrec, and for the "another meetrec" message when another attempt holds it, while forbidding any pid or file-content read. On Windows both holders give the same sharing-violation error to a share-mode-none open, so the standard library cannot tell them apart. The criterion is not changed here; the disposition is Sammy's | blocks one member of C47 (foreign handle) |
| Q2 | The native suite's existing tests drop `tempfile::TempDir` fixtures, which deletes their own synthetic folders under the isolated TEMP; this phase's dispatch authorizes no deletion. New `m7_` tests can keep their folders; the existing 124 cannot without edits the plan forbids | does not block checks; must be settled before the first test run (CP2) |

## Handoff

- **Settled mid-build:** none
- **Dispatched:** none
- **Boundary:** none - CP1 only: plan.md and checks.md written, structural validators only, no commit, no test run, no product or test source
- **Abandoned:** none
- **Isolation for any later run:** `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, `HOME`, `TEMP`, `TMP` and caches under `D:\dani-m7-c18d6da1-20261109\env`; `CARGO_TARGET_DIR` on D: (the wrapper's default `C:\vxc` is overridden); `RUSTUP_HOME` and `CARGO_HOME` must be named explicitly because a moved `USERPROFILE` hides the installed toolchain - to settle at the CP2 preflight, not decided here
- **Frozen red:** the adapted `frozen-repro.sh` copy pins `c2d7b6c` (and the recorded S0 SHA) and must not run `git worktree remove`; worktrees are retained
