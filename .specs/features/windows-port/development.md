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
| `frozen-repro.sh` on `889d48c` | 2 controls ok, 5 reproductions fail (`evidence/frozen/`) |

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
