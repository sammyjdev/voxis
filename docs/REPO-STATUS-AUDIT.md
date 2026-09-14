# VOXIS Repository Status Audit

**Audit date:** 2026-09-14  
**Branch:** `master`  
**HEAD:** `917e9b8`  
**Working tree:** clean and aligned with `origin/master`

## Executive summary

VOXIS is an active, functional macOS voice-tools repository. The roadmap and design documents have clearly influenced implementation, especially around daemon lifecycle, dictation readiness, hands-free mode, WebSocket authentication, and tray integration.

The main weakness is documentation state management. The repository contains useful roadmap material, but several specs and implementation plans still describe work as pending even when the corresponding code and commits exist. The backlog is more current, but it still mixes open work with historical context.

In short: execution is ahead of documentation. The immediate risk is loss of traceability between plans, backlog items, commits, and the actual code state.

## Repository facts

| Area | Current state |
|---|---|
| Product scope | Native macOS voice tools on Apple Silicon |
| Components | LARIS dictation, MIRIS meeting notes, Tauri tray app |
| Backend/runtime | Python 3.11, FastAPI, uvicorn |
| Desktop app | Rust and Tauri v2 |
| Frontend | TypeScript, Vite |
| Recent activity | 12 recent commits from 2026-07-28 through 2026-08-04 |
| Python modules | 67 reported modules |
| Rust files | 7 reported source files |
| Test files | 29 reported test files |
| Test functions | 205 reported test functions |
| CI | Tray build and security audit workflows are configured |

## Roadmap and documentation inventory

The repository has nine primary planning documents:

- `docs/BACKLOG.md`
- Four design specs under `docs/superpowers/specs/`
- Four implementation plans under `docs/superpowers/plans/`

### Backlog state

`docs/BACKLOG.md` was last updated on 2026-07-28. It contains seven open items:

- Four P1 macOS UX and operational gaps:
  - Settle versioning.
  - Document tray-path permissions in `INSTALL.md`.
  - Finish the tray smoke-test matrix.
  - Confirm that `Sair` reaps the LARIS daemon.
- Three P2 items explicitly deferred by design:
  - Code signing and notarization.
  - An in-app autostart toggle.
  - A word-error-rate harness for transcription quality.

The backlog also contains a `Done recently` section that records completed work and historical triage decisions. This is useful context, but it should be clearly separated from active roadmap state.

### Specs and plans

The documentation shows several stale statuses:

- 2026-07-16 specs are still marked as drafts awaiting review.
- The 2026-07-27 initial-state spec says review is pending and implementation has not started, despite related implementation evidence in the codebase.
- Implementation plans still contain unchecked task boxes even where commits and source code indicate that the work was completed or partially completed.

## Evidence that the roadmap is being used

The roadmap is not merely aspirational. The following documented or planned areas have corresponding implementation evidence:

- Tray readiness is based on a `ready` flag instead of port liveness.
- Dictation is always available while the tray is running, with `cmd_r` as the control.
- The daemon persists for the session instead of reloading the model on every toggle.
- `ASSISTANT_MODE=always_on` is consumed at boot.
- The `/ws/voice` endpoint rejects unauthenticated connections before the handshake.
- The tray and LARIS theme endpoint are integrated.
- The daemon lifecycle has received multiple fixes for respawning, duplicate processes, stale listeners, and process ownership.
- PortAudio recovery after USB device replug was implemented.

The commit history confirms continued execution after the backlog update, including daemon lifecycle and resilience fixes through 2026-08-04.

## Verification results

| Check | Result | Limitation |
|---|---|---|
| MIRIS tests | 34 passed | None reported |
| LARIS tests | 143 passed, 1 failed | Failure: `packages/laris/tests/test_audio_capture.py::test_denoise_on_changes_audio` |
| LARIS server tests | Incomplete | Run interrupted by a local bind restriction |
| Tray frontend build | Passed | `npm run build` |
| Rust library tests | Not completed | Cargo could not write to the global cache because of permissions |

The repository therefore has meaningful automated coverage, but the overall state cannot be called fully green based on this audit.

## Assessment

### What is working

- Core product structure is present and documented.
- Recent commits target concrete operational issues rather than speculative features.
- The backlog prioritizes real macOS UX and reliability gaps.
- CI includes build and dependency-security checks.
- Most Python tests pass, and the tray frontend builds successfully.

### What is out of sync

- Plan checkboxes do not reflect implementation progress.
- Spec status fields do not reflect the current codebase.
- The backlog does not have a clear last-reviewed date after the 2026-08-04 commits.
- Versioning remains unresolved, with manifests declaring `0.1.0` and no repository tag.
- The test baseline has at least one known failure and two environment-related verification gaps.

## Recommended documentation action

Perform a single reconciliation pass across the planning documents:

1. Mark completed plan tasks based on the commit history and current source.
2. Change spec status fields to `implemented`, `partially implemented`, or `deferred` where appropriate.
3. Keep only actionable work in the open backlog and move historical context into a separate section.
4. Add a current verification snapshot, including the known test failure and blocked Rust test command.
5. Decide the versioning scheme before creating a release tag.

This audit intentionally does not modify the backlog, specs, plans, source code, or tests.

## Update 2026-09-14: reliable recording implementation

After this audit was written, the MIRIS reliable-recording plan (`docs/superpowers/plans/2026-09-14-miris-reliable-recording.md`) executed Tasks 1-4 and 6. The implementation replaced the PID-based state described above with a manifest-based session lifecycle (`packages/miris/miris/session.py`), and the numbers in this section supersede the verification table.

Automated gates at commit `74f9611`:

| Command | Result |
|---|---|
| `PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 python3 -m pytest -q` (packages/miris) | 76 passed |
| `cargo test --lib` (apps/tray/src-tauri) | 34 passed |
| `npm run build` (apps/tray) | passed |

The MIRIS suite grew from 34 to 76 tests. The cargo-cache blocker reported above did not reproduce in this environment.

Fault-probe evidence (plan Task 2, Step 8), run on this Mac against a live two-channel capture:

- SIGKILL to the recorder left `mic.wav` valid and `system.wav` with stale RIFF/data lengths (ExtAudioFile finalization never ran). This triggered the design's repair branch: deterministic 16-bit PCM container repair is implemented (`session.repair_wav`, used by `miris recover`) and was verified against the real damaged file (`interrupted` -> `captured`, 3.5 s mic recovered).
- The same probe revealed the Swift helper outlived the killed recorder and kept capturing system audio (69 MB orphan). Fixed: the helper now watches `getppid()` and runs its normal teardown when the parent dies, which also finalizes the WAV header. Re-probe: helper exits on its own within seconds; both WAVs reopen valid with a 0.3 s duration delta.

Remaining work: plan Task 5 (manifest-driven tray UI), Task 7 (owner-Mac failure matrix, four-hour soak, documentation reconciliation, versioning, `v1.0.0`).
