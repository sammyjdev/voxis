# Windows port completion - development evidence

Author development evidence for plan `windows-port` (approved rev3, block
bea16b12-f0f2-4b5b-9450-f299bc7c0350, "Aprovar com PR"). It is not review,
verification or acceptance: Rita reviews the diff and Vera runs the matrix
and S10 independently (AC 65). Logs, exit files and hashes:
`Shared/evidence/dani-voxis-windows-port/` (copied from the build host's
`D:\voxis-port-work\evidence`).

## Identities

| Item | Value |
| --- | --- |
| Frozen PR #4 head and base of this branch | `889d48c9a2cca7f61a0648b832c3c3ac20953c78` (PR #4 draft, base `master` `e2971bf21bfc579bf3cacb207cc97063ecd6fd7a`) |
| Layer 1 baseline `H` (criterion 84, 87) | `77a4ba37d7ee72c99f3831c737fce1c34904bc89`, parent `889d48c`, exactly the 5 harness files |
| Branch | `dev/windows-port-fixes`, draft PR into `spike/windows-rust-stt` |
| Build host | Windows 11 x64 (10.0.26200), AMD Ryzen 9 9900X, 32 GB, NVIDIA GeForce RTX 4070 Ti (driver 617.42), rustc 1.99.1, CUDA toolkit 13.5 with `CUDAARCHS=89` for local native tests (the shipped build is 13.3.1 for `75;86;89;120`, built only by CI) |

## Baseline at `889d48c`

Native suite 25 passed, Windows tray suite 61 passed (`baseline-889d48c/`,
exit 0 each). The generated tray sidecar was copied from the owner's checkout
for the tray build only (SHA-256 `fa9bff6e...0ea0e0` at both ends,
`sidecar-copy.sha256`); it is not evidence of any installer.

## Red before green

Each production change was preceded by tests written from the criteria. The
old rule was first extracted, unchanged, into a function the tests call (the
seam), the tests ran red against it, then the rule changed in the same commit
as its tests. At `889d48c` itself these tests do not compile (the seams did not
exist), so the red runs are against the old rule, not the old commit; F17 and
F23 also reproduce through the real path (HTTP on loopback, a real child
process with a spaced folder).

| Slice | Red (old rule) | Green | Commit |
| --- | --- | --- | --- |
| S1 F17 overlay | 3 failed: `C:/.../sentinel.txt` accepted, the sentinel served 200 over HTTP, a junction served 200 (`red-S1-F17.log`) | 9/9 (`green-S1-F17.log`) | `bc92673` |
| S2 F18 folders, write failure | 6 failed, incl. the write panic at `main.rs:261`; the two `recover` tests already passed (characterization) | native 44/44 | `49c51f6` |
| S3 F19 F23 owners | 11 native failed (a dead recorder blocked `start`, `stop`/`process` ignored the owner); tray: a live recorder under `...\Sam Doe\...` showed Interrupted | native 58/58, tray 63/63 | `c3204a8` |
| S4 F20 F22 settings | frontend 5, tray 4, native 3 failed (`USB Microphone" # desk`, f13 not preselected, ignored keys saved) | frontend 5/5, tray 70, native 63 | `062d1a4` |
| S5 F21 deadlines | 8 failed: a stalled call ended only when the server dropped it (3.0 s, no deadline), a retry created a second Doc | native 74/74 | `c3a1381` |
| S6 dictation | 10 native failed (text in the log, silent paste failures, panics, 400 ms of speech measured 390 ms); tray 3 | native 90/90, tray 73 | `f9e4421` |
| S7 `--verify` | 4 failed (no re-check, corrupt file kept) | native 95/95 | `69fdb35` |
| S8 S9 documents and contract | 13 of 18 contract checks failed on the old docs; the 5 preservation checks passed | 18/18 | `691a6e7` |

Characterization tests (behaviour already right at `889d48c`, no production
change): `recover` header repair (AC 11), live-owner refusal (AC 12 part),
`ata.md` before Drive (AC 32), gesture timings (AC 35-38 except the 400 ms
boundary), the protocol guard of the driver (AC 69).

## Decisions taken while building

- `toml_edit` 0.25 (already in the native lock) instead of 0.22: 0.22 would
  have moved `proc-macro-crate` 2.0.2 to 2.0.0 and `toml_datetime` 0.6.3 to
  0.6.11, which criterion 66 forbids. The lock gains only `sysinfo` 0.37.2,
  `ntapi` 0.4.3 and `objc2-io-kit` 0.3.2.
- `sem GPU: usando CPU` reads ggml's device registry through three
  `extern "C"` declarations: whisper-rs re-exports it only with `raw-api`,
  a feature change that would alter the engine inputs criterion 84 compares.
- dictate's backend is not printed by new code: whisper-rs 0.16 routes
  whisper.cpp's log only with `log_backend`, also a feature change; the
  `using <name> backend` line reaches `laris.log` through stderr (AC 46).
- The tray keeps the "microphone lost" flag on its daemon handle, so the
  existing test that builds `AppState` field by field is untouched.
- Tests that open loopback sockets or start processes share one lock: a
  child started while another test's socket was open reset that connection.
- The harness self-tests land in their own commit after `H`, so `H` holds
  exactly the files criterion 87 lists.
- AC 60 takes its fallback branch: pushing a change to
  `.github/workflows/security-audit.yml` was refused (the `openbot-dev` token
  has no `workflow` scope), so the native lock is not added to the audit
  workflow and the README says the `meetrec` binary is not covered. Adding
  the job needs a push with a workflow-scope token: Sammy's decision.
- The native Run line of `checks.md` disables MSYS conversion, because
  `check_proofs.py` runs proofs in Git Bash.

## Development commands and observed results at the branch head

`check_proofs.py windows-port` at `1ff3403`: 131 run proofs and the build pass, 0 fail, tree clean (`proofs.md`, `check_proofs.log`); its 34 `walk` rows are pending, not walked, and
`build-vulkan.log`, `build-tray.log` for the Vulkan edition and tray release
builds (exit 0, no warnings). The harness smoke (`harness-smoke/`, one block,
two sweeps, native CUDA only, local 13.5 build) only shows the driver works
end to end; it is not an S10 measurement.

## Not done here, by the plan

- Matrix rows M1-M17 on candidate bytes, S10 layer 1 runs and layer 2
  (dictation and meeting latency): Vera, inside the Execution gate, after a
  reviewed SHA, Sammy's tag choice and the CI draft release.
- No installer, keyboard hook, recording, clipboard or real provider call ran
  in this session; no tag, release, merge or ready transition.
- Python suites (`packages/laris`, `packages/miris`) and the Linux tray suite
  run in CI; `packages/` is unchanged.

## Review round 1 (Felix): fixes for Rita's findings on `581fefc`

Source of the findings: Rita's local review of `581fefc` (`Shared/evidence/rita-voxis-pr5-581fefc/`,
`REPORT.md` controls, with `findings.json` and `CRITERIA-INDEPENDENT.md`). Authorization: Sammy's
"Aprovar com PR" (block `bea16b12-f0f2-4b5b-9450-f299bc7c0350`). This round is development, not
review or verification: Rita re-reviews the new head and Vera runs the matrix and S10 later. Logs, exit
codes and hashes: `Shared/evidence/felix-voxis-pr5-round1/` (`MANIFEST.sha256`, `STATUS.md`). No
criterion in `plan.md` was changed, and no existing test was edited or weakened.

| Finding | Fix | Tests (new) | Red evidence |
| --- | --- | --- | --- |
| P5-1 | the tray decides a respawn from the flags the exit just set (`supervise_tick`) | two tray tests | old rule (stale snapshot) put back in the seam: 1 failed, `red-P5-1-tray.log` |
| P5-2 | no input device at start exits 3 with `microfone desconectado: ...` | `a_dictate_that_starts_with_no_microphone_exits_3_like_a_lost_one` | old exit 1 put back: failed, `red-P5-2.log` |
| P5-3 | the production writer is `BufWriter`; after a failed flush the header is repaired from the bytes on disk (`repair_wav_in`, overwrites only existing bytes) | 4 tests, 3 on a buffered `PartialDisk` | repair skipped: header not updated, `red-P5-3.log`, `red-P5-3b.log` |
| P5-4 | `PasteOutcome`; `paste_ms` (Ctrl+V sent) and `restore_ms` only for a paste that happened, else `paste=<why>` | 3 tests | old always-`paste_ms` line put back: failed, `red-P5-4.log` |
| P5-6 | frozen-source reproductions, see below | `frozen-repro/` | `evidence/frozen/` |
| P5-8 | `claim` needs 6 distinct complete blocks; a shorter run is a smoke run with no claim | 2 tests | new tests failed on `581fefc`: `red-py.log` |
| P5-9 | README and INSTALL name `dictate-*.log`, `laris.log` and `laris.log.1`; AC 19 reconciled in code, see below | 3 tray tests | old rule put back: failed, `red-P5-9-AC19-tray.log` |
| P5-19 | a variant that cannot launch is an `unavailable: <exact error>` run and the manifest is still written | 2 tests | `red-py.log` |
| P5-20 | `stop` returns a recorder's own `failed`/`interrupted` state at once | `stop_keeps_the_terminal_failure_the_recorder_wrote_after_the_stop_request` | old loop put back: failed after 3 s, `red-P5-20.log` |
| P5-21 | a clipboard failure hides the orb in both modes (`hide_orb_after`) | `a_clipboard_failure_hides_the_orb_in_both_modes` | old rule put back: failed, `red-P5-21.log` |
| P5-22 | the source SHA keeps its reason; Python records package versions, model identity and weights hash, prompt hash and interpreter; native records the model file hash | 5 tests | `red-py.log` |
| AC 45 | state creation, warm-up and microphone open failures are logged (`modelo não carregou: ...`, exit 1; a mic that will not open is exit 3), no panic | `a_model_that_fails_to_make_its_state_or_to_warm_up_is_logged_not_a_panic` | new seam; at `581fefc` these were `unwrap`/`expect` |
| AC 47, AC 92 | `dictate` reads whisper.cpp's own `using <name> backend` / `no GPU found` lines through its log callback (`whisper_log_set`, declared like the ggml registry calls, no feature change); the registry is the fallback | 4 tests; one loads a missing model through the real library to prove the hook is wired | new seam |

P5-10 and P5-11 (nits) are left out. The `check-windows-contract.sh` retention scan was not touched.

How the reds were made: the same as the first round. Each red puts the old rule back into the corrected
code for one run (a mutation, script `reds.py`, not committed) and keeps the log. They are not runs at
`889d48c` and not a claim that a red was seen before the first implementation. The new tests for P5-2,
P5-4, AC 45 and AC 47 call functions that did not exist at `581fefc`, so their first failure on that head is
a compile error; their behavioural reds are the mutations above where one exists.

### AC 64: frozen-source reproductions (P5-6)

`scripts/windows/frozen-repro.sh <worktree> <evidence dir>` checks out `889d48c` in a new worktree,
verifies the blobs of the three files it touches (`dictate.rs` `454881e1...`, `session.rs` `428327c5...`,
`main.rs` `772a5f51...`), appends the test-only modules in `frozen-repro/` and runs them with
`scripts/windows/frozen-repro.cmd`. The modules call the frozen functions themselves (`serve_http`, `start`,
`stop`, `writer`), no extracted seam. Two `control_` tests pass there (the harness reaches the code); five
tests fail there:

| Obligation | Frozen reproduction (fails at `889d48c`) | Candidate-path test (passes) |
| --- | --- | --- |
| AC 2-3 / F17 disclosure | `an_absolute_windows_path_under_static_is_not_served`, `a_junction_inside_the_frontend_does_not_lead_outside_it`: `SECRET` served with 200 over loopback | `a_sentinel_outside_the_root_is_never_served_over_http`, `a_junction_inside_the_root_pointing_outside_answers_404` |
| AC 18 / F18 data loss | `a_track_that_cannot_be_created_ends_in_an_error_not_a_panic`: the writer thread panics (`create wav`) | `a_track_that_cannot_be_created_ends_in_an_error_not_a_panic` through `writer`, plus the buffered full-disk tests |
| AC 12-13 / F19 stuck session | `a_session_whose_recorder_is_dead_does_not_block_a_new_start`: `start` refuses with `a session is already active` | `a_session_whose_recorder_exited_does_not_block_a_new_start_under_the_system_owner_check` (real owner check, real exited pid) |
| AC 86 | `stop_settles_a_session_whose_recorder_is_dead_without_waiting_60_seconds`: `stop` still waits after 5 s | `stop_settles_a_session_whose_recorder_exited_at_once_under_the_system_owner_check` |

What this does not cover, so the AC 64 certificate is incomplete and is not claimed:

- a full-disk write at the frozen SHA (an unwritable path reproduces the same panic class, not a full disk; the
  full-disk case needs an injected writer and is proven only at the candidate's seam);
- F18 folder reuse, `models fetch --verify` (AC 49; the feature did not exist at `889d48c`), the provider deadlines
  (AC 29-31; the frozen provider URLs are fixed, so a loopback stall needs a seam or a real call), F23 (tray) and
  `process`/`recover` under a live owner: the first-round seam reds remain their only red evidence;
- any red seen before the first implementation. These are independent, retrospective reproductions on the base,
  written to close the gap Rita named. Whether the seam-based reds satisfy AC 64 for the rows above is the owner's
  decision, not this document's.

### AC 19 and the dictation log

`laris.log` was renamed to `laris.log.1` (replacing the older one) and then re-created on every dictation start:
a move and a replacement in a log folder, which AC 19 forbids. On Windows the tray now opens the log for append
and writes a `--- voxis run <time> ---` line; an exit is classified from the latest run only (`laris::current_run`),
so an old `modelo não carregou` cannot be mistaken for the current one. macOS and Linux keep the single
generation (AC 66), pinned by a test that runs on every OS. The existing rotation test is `#[cfg(unix)]` and was
not touched. The log now grows without bound on Windows: AC 19 says no automatic retention in this phase.

### Harness provenance

The layer 1 baseline `H` is still `77a4ba37d7ee72c99f3831c737fce1c34904bc89`, exactly the five files of
criterion 87. This round changes `scripts/stt_bench/perf_driver.py` and `voice.py` after `H`; the engine inputs
(features, lock entries, prompt, model pins) are unchanged since `H` (`check-windows-contract.sh`
`engine_inputs_unchanged_since_h`). A layer 1 run must record the repository HEAD it ran from: the earlier harness
smoke was run before this round and does not cover the changed driver.

### Observed results at `b2d6b5c` (code head of this round)

Run by the author in this session, on the build host of the first round (Windows 11 x64, RTX 4070 Ti, CUDA 13.5
with `CUDAARCHS=89`, local only; builds in the fixer's own `D:` folders, the CI toolchain is unchanged):

| Run | Result |
| --- | --- |
| native (`meetrec-test.cmd`, CUDA) | 112 passed, 0 failed (was 95 at `581fefc`, 25 at `889d48c`), no warnings |
| Windows tray | 79 passed, 0 failed, no warnings (was 73) |
| frontend (`node --test`) | 5 passed |
| bench (`pytest scripts/stt_bench`) | 37 passed (was 28) |
| contract (`check-windows-contract.sh`) | 18 of 18 PASS |
| Vulkan edition build of `meetrec` | exit 0, no warnings |
| `check_proofs.py windows-port` | 160 run proofs and 3 builds pass, 0 fail, tree clean; 34 `walk` rows listed as `Walked`, which is a handoff marker, not an execution |
| `frozen-repro.sh` on `889d48c` (round 1 run; 3 controls, not 2) | 3 controls ok, 5 reproductions fail (`evidence/frozen/`) |

The Python suites of `packages/` and the Linux tray run in CI only; the PR's CI result for this head is not
known to this document.

### Pending, counted honestly

Nothing below was executed by this round:

- the 34 `Proof: walk` rows, the acceptance matrix M1-M17, the installer proofs, S10 layers 1 and 2 and the
  independent re-review; `check_proofs.py` prints the walk rows as "Walked" because this file has a handoff
  section, which does not mean they ran;
- no installer, keyboard hook, microphone or loopback recording, real clipboard, real provider or model benchmark
  ran in this round; the clipboard paths ran only against fakes and the backend hook only against a missing model.
- `paste_ms` is the moment Ctrl+V was sent. When the target application finished pasting is not observable from
  here; the report of release-to-paste latency must say which boundary it used.

## Review round 2 (Felix): carryover of Rita's review of `361a965`

Controller: `Shared/evidence/rita-voxis-pr5-361a965/REPORT.md` (final, with `VERIFIER.md`, `JUDGE.md`,
`findings.json`). Eight IDs closed there by inspection and are not reopened. This round covers the two
carryovers: the clipboard restore (P5-4 and P5-21, one cause) and the behavioural certificates for P5-6 / AC 64.
Still development evidence, not review or verification. Logs, exit codes, hashes:
`Shared/evidence/felix-voxis-pr5-round2/`. No criterion and no existing test was changed.

### Clipboard restore (P5-4, P5-21)

`paste_outcome_with` ignored a failing `cb.set(previous)` and stamped `restore_ms` anyway, including when there was
no previous text. `PasteOutcome::Pasted` now carries `Restore::{Done(ms), NoPrevious, Failed(error)}`:
`restore_ms` only for `Done`; `Failed` logs `paste restore failed: clipboard <error>` (no dictated text) and the
line says `restore=failed`; `NoPrevious` says `restore=none`; the injection timestamp (`paste_ms`) is unchanged and
real; `hide_orb_after` hides the orb on a restore failure in both modes (AC 42, wording unqualified).
Tests (fake clipboard, first `set` passes and the next fails; no previous text; the hide decision in both modes):
red against the old restore rule in `red-clipboard-restore.log` (3 failed), green in the native run. Nothing real was
touched, and the end of the paste inside the target application is still not observed.

### AC 64: separated by kind

Everything below the line "red" is a reproduction on `889d48c` written and run now: a retrospective reproduction,
not an observation made before the first implementation. Run `scripts/windows/frozen-repro.sh <worktree>
<evidence dir>` (needs `CARGO_TARGET_DIR` and `TRAY_TARGET_DIR` outside the worktree and the tray sidecar). It
verifies six frozen blobs (`frozen-blobs.txt`), stores the hashes of the sources and of the test modules, and the
exact diff against the frozen tree (`frozen-test-patch.diff`: appended lines only, no frozen line removed or
changed, so the frozen functions are the byte-identical production code), the raw output and the exit codes. The
tests call the frozen `serve_http`, `start`, `stop`, `cli`, `writer`, `agent` and the tray's `snapshot`. Result of
the last run: 12 red, 5 controls ok, 1 characterization ok, script exit 0.

| Obligation | Frozen test (`889d48c`) | Kind | Candidate-path test (`361a965` + this round) |
| --- | --- | --- | --- |
| AC 2, disclosure | `an_absolute_windows_path_under_static_is_not_served` | red | `a_sentinel_outside_the_root_is_never_served_over_http` |
| AC 3, disclosure | `a_junction_inside_the_frontend_does_not_lead_outside_it` | red | `a_junction_inside_the_root_pointing_outside_answers_404` |
| AC 13 / F19, stuck session | `a_session_whose_recorder_is_dead_does_not_block_a_new_start` | red | `a_session_whose_recorder_exited_does_not_block_a_new_start_under_the_system_owner_check` |
| AC 14, dead processing owner | `a_processing_session_whose_pipeline_is_dead_does_not_block_a_new_start` | red | `a_dead_pipeline_puts_its_session_back_to_captured` (fake owner) |
| AC 16, recorder exits early | `a_recorder_that_exits_before_recording_leaves_no_session_in_starting` | red | `a_recorder_that_exits_before_recording_fails_the_session_with_recorder_error` |
| AC 86, `recover` with a live owner, bytes kept | `recover_refuses_a_session_whose_owner_is_live_and_leaves_every_byte` | red | `recover_and_process_through_the_cli_refuse_a_live_owner_and_leave_every_byte` (through `cli_with`) |
| AC 86, `process` with a live owner | `process_refuses_a_session_whose_owner_is_live_and_leaves_every_byte` (controlled boundary: the frozen `spawn_child` starts the test binary, which runs no test) | red | same test as above |
| AC 86, identity before the stop file | `stop_does_not_signal_a_process_it_has_not_identified_as_the_recorder` | red | `stop_writes_the_stop_file_only_after_checking_the_recorder` |
| dead recorder, `stop` waiting | `stop_settles_a_session_whose_recorder_is_dead_without_waiting_60_seconds` | red (wait, not an AC 86 row) | `stop_settles_a_session_whose_recorder_exited_at_once_under_the_system_owner_check` |
| AC 15 / F23, spaced profile path (tray reader) | `a_live_recorder_under_a_profile_path_with_a_space_is_recording` | red | `a_live_recorder_under_a_profile_with_spaces_shows_recording` |
| AC 29-31, no finite deadline | `the_provider_client_carries_a_finite_connect_and_overall_deadline` (reads the frozen client; no host is called) | red | `every_provider_call_carries_its_deadline`, `a_stalled_anthropic_call_ends_at_its_deadline` |
| AC 18, create failure only | `a_track_that_cannot_be_created_ends_in_an_error_not_a_panic` | red, create failure (not AC 18) | `a_track_that_cannot_be_created_ends_in_an_error_not_a_panic` |
| AC 49 | `characterization_a_cut_plain_download_leaves_only_the_part_file` | characterization, green on the base | `a_cut_verify_download_keeps_the_old_file_and_removes_its_part` |
| harness reach | `control_*` (5) | control | - |

Behavioural properties with no frozen red, and why (not reproducible, not waived):

- AC 18, an error during writing (not at creation): the frozen `writer` opens its own `File` and takes no injected
  writer, and a physical full disk is not authorized. Candidate: `write_wav` over the real `BufWriter` with an
  injected failing disk (`a_full_disk_under_the_buffered_writer_...`), which is what AC 92 allows.
- AC 7-10 / F18 folder reuse: at the frozen SHA the only route is `record()`, which opens the microphone and the
  loopback. Candidate: `reserve_meeting_dir` tests. A two-meetings-in-a-minute run with devices belongs to the
  installed phase (Vera).
- AC 49, verification and preservation: `models fetch --verify` does not exist at `889d48c`; there is no frozen
  entry that can fail. The characterization above is explicitly not a red.
- AC 17, a live recorder that never finishes: the candidate's owner identity needs this binary run with exactly
  the marker and the folder, and a lookalike that survives 60 s cannot be built from the frozen test binary. The
  frozen `stop` writes `recorder_timeout` after 60 s and ends nothing; not reproduced. Candidate:
  `stop_ends_a_recorder_that_never_finishes_and_marks_it_interrupted`.
- AC 29-32, bounded failure over a transport and preservation: the frozen provider URLs are constants and no real
  host may be called, so a stall cannot be driven; only the missing deadline is shown (row above).

Temporal non-conformance (only Sammy decides; nothing is waived or reinterpreted here): criterion 64 says the
failing run comes before the production change. No run on `889d48c` was made before the first implementation, and
none can be made now. Historical artifacts that exist: Dani's per-slice red logs (`Shared/evidence/dani-voxis-
windows-port/red-S*.log`), written before each production change but against the old rule extracted into a seam,
not against `889d48c`; and the round-1 mutation reds (`felix-voxis-pr5-round1/red-*.log`), made after the change.
The reproductions above add behavioural coverage at the frozen SHA, after the fact.

### Observed results at `2d70f90` (code head of round 2)

Same build host and local-only settings as round 1. Native 118 passed (CUDA), Windows tray 79, frontend 5, bench 37,
contract 18 of 18, Vulkan build of `meetrec` exit 0 with no warnings. `check_proofs.py windows-port`: 166 run proofs
and 3 builds pass, 0 fail, tree clean; the 34 `walk` rows are still only listed as `Walked`. Frozen reproduction: 12
red, 5 controls ok, 1 characterization ok, script exit 0. The PR's CI for the new head is not known to this document.

### Pending, counted honestly

The 34 `Proof: walk` rows, the matrix M1-M17, installer proofs, S10 layers 1 and 2 and the independent re-review
did not run; `check_proofs.py` still prints the walk rows as `Walked` only because this file has a handoff
section. No installer, keyboard hook, microphone, loopback, real clipboard, real provider or model benchmark ran;
the frozen tests started only `ping`, `cmd /C exit`, an inline `powershell -Command Start-Sleep` stand-in and the test
binary itself, all ended by the tests.

## Review round 3 (Felix): the certificates Vera's run left open

Source: `Shared/evidence/vera-voxis-pr5-6eee125-execution/AC64-GAPS.md` and `EXEC-REPORT.md` (Vera's own runs on
`6eee125`), with `rita-voxis-pr5-6eee125/VERA-NEXT.md`. Still development evidence: Vera runs this delta
independently. No criterion, no existing test and no production behaviour changed in this round; the only
production-side change is in the benchmark driver's records (V1 below). Logs, hashes and the recipe:
`Shared/evidence/felix-voxis-pr5-certificates/`.

### AC64 rows closed in this round

All frozen tests call the unmodified frozen functions (the patch only appends text, `frozen-test-patch.diff`;
no frozen line is changed). They are retrospective reproductions made now. Run: `scripts/windows/frozen-repro.sh`.

| Gap row | Frozen result on `889d48c` | Candidate counterpart |
| --- | --- | --- |
| AC 10, collision at the generated second | `start_does_not_reuse_a_session_folder_at_the_generated_second`: red. Folders exist for this second and the next two, each with manifest, recording and sentinel; `start` rewrote `session.json` and created `recorder.log` in an existing session. A free-second rollover would add a folder and fail the test too; control `control_finished_sessions_do_not_block_admission` ok | `start_through_the_real_entry_refuses_a_session_folder_at_the_generated_second_and_keeps_every_byte` (real `start`, whole tree compared byte for byte) |
| AC 29, OAuth over a stalled transport | `the_oauth_call_ends_at_its_thirty_second_deadline_when_the_endpoint_stalls_for_forty`: red. Loopback `token_uri` in a temporary APPDATA with made-up files; the call ended after 40.006 s with `google token: io: Peer disconnected`, when the server dropped it. No real host or secret | `the_oauth_call_ends_at_its_real_thirty_second_deadline_when_the_endpoint_stalls_for_forty` (real `DEADLINES`, about 30 s) |
| AC 18, failure during writing, not at creation | `a_write_that_fails_in_the_middle_of_a_track_ends_in_an_error_not_a_panic`: red. No adapter: the frozen `writer` runs unmodified on a real file; a second handle locks the byte range from 1 MiB (`LockFileEx`, declared in the test), so the first flush reaching it fails by itself with a lock violation. Panic at `main.rs:234` (`write_sample(..).unwrap()`) with 1048576 bytes already on disk. Control `control_a_track_that_stays_below_the_locked_range_is_written_and_finalized` ok | `a_write_that_fails_in_the_middle_of_a_track_on_a_real_file_is_an_error_and_the_data_on_disk_is_recoverable`: the production `writer` (`File`, `BufWriter`, repair through `reopen`) on the same locked file returns `disk_write_error`, sets the failed flag, and the header counts every whole sample on disk (a `WavReader` reads them). Nothing fills a disk |
| AC 49, old admission rule | `characterization_the_name_based_ready_admits_corrupt_models_and_never_asks_the_endpoint`: green on the base, not a red. MODEL and VAD corrupted, a loopback endpoint that cuts every download, the frozen `fetch_with` says `ready`, exit 0, hashes nothing, makes no request. `--verify` does not exist at `889d48c`, so no verify behaviour was run | `verify_replaces_a_file_whose_hash_differs_after_a_verified_download`, `a_cut_verify_download_keeps_the_old_file_and_removes_its_part`, `a_full_disk_during_verify_keeps_the_old_file` |

Controls (7 in all, 2 new) and the 2 characterizations (the cut plain download of round 2 and this admission rule) are
named `control_` and `characterization_`, and the script expects them green. Result of the last run: 24 tests, 15 red,
7 controls ok, 2 characterizations ok, script exit 0.

### AC 29-32: the provider branches, with real transport errors and bytes

`a_real_transport_stall_at_each_provider_ends_at_its_deadline_and_the_pipeline_keeps_every_byte` (candidate): a
loopback stand-in for Anthropic, the Google token endpoint and Drive stalls one call at a time. For each of the five
branches (anthropic, google token, drive list, drive mkdir with an empty folder list, drive upload) it runs the real
`summarize` or `upload_doc` through `run_with_drive`, takes the error the transport really produced, hands it to
`pipeline_failed`, and checks: the call ended within 2.5 s; the error names the provider and carries the deadline mark;
the manifest says `captured` / `provider_timeout` and its message is that same error; `mic.wav`, `system.wav` and
`transcript.md` are byte-identical before and after; `ata.md` is absent for the anthropic branch and, for the others,
exists before the first Drive call and is byte-identical after the failure. Added at the minutes level:
`a_stalled_drive_folder_create_ends_at_its_deadline` (list returns no folder, the create stalls).

### Python worker sampling (V1) and what the harness records

A venv's `python.exe` is a launcher stub whose child does the work; the driver samples the pid it spawns, so a variant
launched through it reads near-zero CPU and a few MiB. The rule, now in the `perf_driver.py` docstring: a Python variant
names the interpreter that runs the model, with the venv's `site-packages` on `PYTHONPATH` (what Vera's protocol v2
does). The run record now carries `sampled_process` (pid, image, and the children seen while sampling), so a stub is
visible as a sampled image that has a child; tests `test_the_record_names_the_sampled_process_and_the_children_it_had`
and `test_a_variant_without_children_records_none`. This does not follow children and builds no process monitor: with the
documented launch rule AC 73 (the record names what was sampled) and AC 75 (a counter that cannot be read is null with a
reason) hold for the worker; a variant launched through a stub still gets the stub's counters and says so in the record.
It changes the driver after `H` again: a layer 1 run must record the HEAD it ran from, and a run already armed on `6eee125`
keeps that SHA.

### Running the proofs safely (V3)

`check_proofs.py` with no overrides builds under `C:\vxc` and runs the tray test
`enabling_autostart_writes_the_run_value_and_disabling_removes_it`, which writes and removes one value named
`VOXIS-test-<pid>` under `HKCU\...\Run`. Run it only inside a gate that allows that, or with `CARGO_TARGET_DIR`
and `TEMP` on a volume with space and the test skipped (`cargo test ... -- --skip enabling_autostart_writes_the_run_value`).
Round 1 and 2 of this author ran that test inside the full tray suite; a read-only check of `HKCU\...\Run` afterwards
found no `VOXIS-test` value left. This round skipped it and did not re-run `check_proofs.py`, so `proofs.md` still records
`2d70f90`.

### Still without a certificate, by reason

- AC 17, a genuine stuck recorder (executable plus exact argv, then the 60 s path, termination and WAV recoverability) and
  the genuine-owner antecedent of AC 86: no process of this image with exactly `miris.recorder <folder>` that lives long
  enough can be built from a test binary; Preflight B.
- AC 7-9 / F18 with the frozen `meeting` and `record`: the frozen route opens the microphone; Preflight B.
- AC 49 `--verify` at the frozen SHA: no entry point exists.
- AC 64's temporal wording: unchanged. No run on `889d48c` was made before the first implementation and none can be made
  now; nothing is waived. Historical artifacts: Dani's per-slice reds (extracted old rule, before each change) and the
  round-1 mutation reds (after the change).
