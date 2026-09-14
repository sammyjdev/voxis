# MIRIS Reliable Recording Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the VOXIS tray reliably record mandatory microphone and system-audio channels, preserve interrupted audio, recover it manually, and ship a verified personal macOS `v1.0.0` release.

**Architecture:** Keep the existing detached Python recorder, `sounddevice` microphone stream, and Swift Core Audio helper. Add one atomic `session.json` lifecycle contract consumed by Python and read by the Rust tray, then make capture, stop, recovery, processing, and UI behavior follow that contract. Keep raw files as the durable source of truth and add no database, service, job framework, or capture dependency.

**Tech Stack:** Python 3.11, stdlib `dataclasses`, `enum`, `json`, `os`, `queue`, `subprocess`, `wave`, NumPy, sounddevice, Swift, Core Audio, Rust 2021, Tauri v2, serde, serde_json, pytest, Cargo test, Vite.

**Spec:** `docs/superpowers/specs/2026-09-14-miris-reliable-recording-design.md`

**Execution status (2026-09-14):** Tasks 1-4 and 6 are implemented and committed
(single-session execution, not the luna subagent fleet). Automated gates at
commit `74f9611`: MIRIS pytest 76 passed, tray `cargo test --lib` 34 passed,
tray `npm run build` passed. The Task 2 Step 8 fault probe found that a
SIGKILLed recorder leaves a truncated `system.wav` (stale RIFF/data lengths)
and an orphaned capture helper; the deterministic PCM repair is implemented
(`session.repair_wav`) and the helper now exits with its parent. Task 5 (tray)
and Task 7 (owner-Mac validation, v1.0.0) remain open.

## Global Constraints

- macOS 14.2 or newer on Apple Silicon only.
- Personal deployment on the owner's Mac. Do not add public onboarding, notarization, or other-platform code.
- The tray is the official UI. The CLI is an internal diagnostic, testing, and recovery interface.
- Microphone and system audio are both mandatory. Failure of either stops both.
- Interrupted audio is preserved and processed only after a manual recovery action.
- A processing failure must leave valid capture audio in `captured`.
- The release must pass one continuous four-hour recording on the owner's Mac.
- The system WAV output is capped at 48 kHz, 16-bit stereo so four hours remain below classic RIFF's 4 GiB limit.
- Use only installed dependencies and platform APIs. Add no database, service, queue framework, or capture library.
- All production changes use red-green-refactor. Never change an existing test merely to force it to pass.
- Preserve the user's untracked `docs/REPO-STATUS-AUDIT.md` unless the final documentation task explicitly reconciles it from verified results.
- Do not create `v1.0.0` until every automated gate and the owner-Mac validation pass.

## File Map

- Create `packages/miris/miris/session.py`: session schema, atomic persistence, transitions, process identity, discovery, WAV inspection, and recovery validation.
- Create `packages/miris/tests/test_session.py`: pure lifecycle and recovery tests.
- Modify `packages/miris/miris/recorder.py`: two-channel readiness, bounded queue, watchdog, joint failure, and manifest updates.
- Modify `packages/miris/miris/systemtap.py`: observable helper progress and bounded lifecycle.
- Modify `packages/miris/helper/audiotap.swift`: cap output sample rate at 48 kHz.
- Modify `packages/miris/tests/test_recorder.py` and `test_systemtap.py`: capture fault tests.
- Modify `packages/miris/miris/cli.py` and `tests/test_cli.py`: manifest-based start, bounded stop, manual recover, and process commands.
- Modify `packages/miris/miris/pipeline.py`, `vault.py`, `tests/test_pipeline.py`, and `tests/test_vault.py`: retry-safe processing and idempotent notes.
- Modify `apps/tray/src-tauri/src/miris.rs` and `lib.rs`: manifest state, pending actions, stale-state derivation, notifications, and safe quit.
- Update Rust tests in those same modules.
- Modify `packages/miris/miris/cli.py` and `tests/test_cli.py` again only in the final sequential task for the two-channel doctor check.
- Update release documentation and version files only after verification passes.

## Parallel Execution Topology

Use `gpt-5.6-luna` with `reasoning_effort: high` for every implementation and review subagent.

```text
Task 1: manifest contract, sequential foundation
  -> Wave A, three Luna agents in parallel
     - Task 2: recorder, SystemTap, Swift helper
     - Task 3: CLI lifecycle and manual recovery
     - Task 4: pipeline and vault idempotency
  -> Wave B
     - Task 5: Rust tray integration
     - Task 6: two-channel doctor and automated gate preparation
  -> Task 7: integrated verification, owner-Mac validation, docs, versions
```

Before each parallel wave, use `parallel-agent-harness` and create its manifest.
Each worker receives exclusive ownership of the files named by its task, must
not revert other workers, and must adapt to already merged contract changes.
Run an independent Luna review after each task. The reviewer must not be the
worker that implemented the task.

---

### Task 1: Atomic Session Contract

**Files:**
- Create: `packages/miris/miris/session.py`
- Create: `packages/miris/tests/test_session.py`

**Interfaces:**
- Produces: `SessionState(StrEnum)` with `STARTING`, `RECORDING`, `STOPPING`, `CAPTURED`, `PROCESSING`, `COMPLETED`, `INTERRUPTED`, and `FAILED`.
- Produces: frozen dataclasses `SessionError(code: str, message: str, occurred_at: str)`, `ChannelInfo(path: str, frames: int, sample_rate: int, channels: int)`, and `SessionManifest(schema_version: int, session_id: str, state: SessionState, created_at: str, updated_at: str, recorder_pid: int | None, processing_pid: int | None, channels: dict[str, ChannelInfo | None], error: SessionError | None, note_path: str | None, transcript_ready: bool)`.
- Produces: `create(session_dir: Path, now: datetime | None = None) -> SessionManifest`.
- Produces: `load(session_dir: Path) -> SessionManifest` and `write(session_dir: Path, manifest: SessionManifest) -> None`.
- Produces: `transition(session_dir: Path, state: SessionState, **changes: object) -> SessionManifest`, rejecting transitions outside the spec.
- Produces: `process_matches(pid: int, marker: str, session_dir: Path, command_reader: Callable[[int], str] | None = None) -> bool`.
- Produces: `latest_active(sessions_dir: Path) -> tuple[Path, SessionManifest] | None` and `latest_pending(sessions_dir: Path) -> tuple[Path, SessionManifest] | None`.
- Produces: `reconcile_stale(sessions_dir: Path, command_reader: Callable[[int], str] | None = None) -> list[SessionManifest]`.
- Produces: `inspect_wav(path: Path) -> ChannelInfo` and `validate_capture(session_dir: Path, max_duration_delta_s: float | None) -> dict[str, ChannelInfo]`.
- Produces: `ensure_legacy_manifest(session_dir: Path) -> SessionManifest` for existing pre-manifest sessions.

- [x] **Step 1: Create the project environment and record the baseline**

Run:

```bash
cd packages/miris
python3.11 -m venv .venv
.venv/bin/pip install -r requirements.txt -r ../laris/requirements.txt
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 .venv/bin/python -m pytest -q
```

Expected: either 34 PASS, or a real dependency/platform failure recorded
verbatim before changes. Do not use the system interpreter's missing
`torchaudio` result as the project baseline.

- [x] **Step 2: Write failing manifest serialization and transition tests**

Add:

```python
import datetime as dt
import wave

import numpy as np
import pytest

from miris import session

FIXED_NOW = dt.datetime(2026, 9, 14, 14, 30, tzinfo=dt.timezone(dt.timedelta(hours=-3)))
FIXED_ISO = FIXED_NOW.isoformat()


def make_session(tmp_path):
    session_dir = tmp_path / "20260914-143000"
    session_dir.mkdir(exist_ok=True)
    if not (session_dir / "session.json").exists():
        session.create(session_dir, now=FIXED_NOW)
    return session_dir


def advance_to_captured(session_dir):
    session.transition(session_dir, session.SessionState.RECORDING,
                       recorder_pid=42)
    session.transition(session_dir, session.SessionState.STOPPING)
    session.transition(session_dir, session.SessionState.CAPTURED,
                       recorder_pid=None)


def test_create_roundtrips_atomic_manifest(tmp_path):
    session_dir = tmp_path / "20260914-143000"
    session_dir.mkdir()
    manifest = session.create(session_dir, now=FIXED_NOW)
    assert manifest.state is session.SessionState.STARTING
    assert session.load(session_dir) == manifest
    assert not list(session_dir.glob(".session.json.*"))


def test_transition_rejects_skipping_recording(tmp_path):
    session_dir = make_session(tmp_path)
    with pytest.raises(ValueError, match="starting -> captured"):
        session.transition(session_dir, session.SessionState.CAPTURED)


def test_processing_failure_returns_to_captured(tmp_path):
    session_dir = make_session(tmp_path)
    advance_to_captured(session_dir)
    session.transition(session_dir, session.SessionState.PROCESSING,
                       processing_pid=321)
    result = session.transition(
        session_dir,
        session.SessionState.CAPTURED,
        processing_pid=None,
        error=session.SessionError("processing_failed", "boom", FIXED_ISO),
    )
    assert result.state is session.SessionState.CAPTURED
```

- [x] **Step 3: Run the focused tests and verify red**

Run:

```bash
cd packages/miris
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 .venv/bin/python -m pytest tests/test_session.py -v
```

Expected: FAIL because `miris.session` does not exist.

- [x] **Step 4: Implement the schema and atomic transition core**

Use `StrEnum`, frozen dataclasses, `dataclasses.asdict`, and stdlib JSON. Keep
the transition table literal and exhaustive:

```python
ALLOWED = {
    SessionState.STARTING: {SessionState.RECORDING, SessionState.FAILED,
                            SessionState.INTERRUPTED},
    SessionState.RECORDING: {SessionState.STOPPING, SessionState.INTERRUPTED},
    SessionState.STOPPING: {SessionState.CAPTURED, SessionState.INTERRUPTED},
    SessionState.CAPTURED: {SessionState.PROCESSING},
    SessionState.PROCESSING: {SessionState.COMPLETED, SessionState.CAPTURED},
    SessionState.INTERRUPTED: {SessionState.CAPTURED},
    SessionState.COMPLETED: set(),
    SessionState.FAILED: set(),
}


def write(session_dir: Path, manifest: SessionManifest) -> None:
    path = session_dir / "session.json"
    tmp = session_dir / f".session.json.{os.getpid()}.tmp"
    tmp.write_text(json.dumps(to_dict(manifest), ensure_ascii=False, indent=2),
                   encoding="utf-8")
    os.replace(tmp, path)
```

`transition` loads, checks `ALLOWED[current.state]`, applies only named
dataclass fields with `dataclasses.replace`, refreshes `updated_at`, writes,
and returns the new value. Always clear `recorder_pid` when leaving capture
states and `processing_pid` when leaving `PROCESSING`.

- [x] **Step 5: Write failing process identity and stale discovery tests**

Add:

```python
def test_process_matches_requires_module_and_session(tmp_path):
    session_dir = tmp_path / "s1"
    reader = lambda pid: f"python -m miris.recorder {session_dir}"
    assert session.process_matches(42, "miris.recorder", session_dir, reader)
    assert not session.process_matches(42, "miris.recorder", tmp_path / "s2", reader)
    assert not session.process_matches(42, "miris.recorder", session_dir,
                                       lambda pid: "sleep 999")


def test_reconcile_dead_recording_as_interrupted(tmp_path):
    session_dir = make_session(tmp_path)
    session.transition(session_dir, session.SessionState.RECORDING,
                       recorder_pid=42)
    changed = session.reconcile_stale(tmp_path, command_reader=lambda pid: "")
    assert changed[0].state is session.SessionState.INTERRUPTED
    assert changed[0].error.code == "recorder_gone"


def test_reconcile_dead_processing_as_captured(tmp_path):
    session_dir = make_session(tmp_path)
    advance_to_captured(session_dir)
    session.transition(session_dir, session.SessionState.PROCESSING,
                       processing_pid=43)
    changed = session.reconcile_stale(tmp_path, command_reader=lambda pid: "")
    assert changed[0].state is session.SessionState.CAPTURED
    assert changed[0].error.code == "processor_gone"
```

- [x] **Step 6: Implement process identity, discovery, and stale reconciliation**

The default command reader runs exactly:

```python
subprocess.run(
    ["ps", "-p", str(pid), "-o", "command="],
    capture_output=True,
    text=True,
    timeout=2,
    check=False,
).stdout.strip()
```

Return false for nonpositive PIDs, command errors, a missing marker, or a
missing exact session path. `latest_active` considers `starting`, `recording`,
`stopping`, and `processing`. `latest_pending` considers `captured` and
`interrupted`. Sort timestamp-named directories newest first and skip malformed
manifests without deleting them.

- [x] **Step 7: Write failing WAV and legacy-session tests**

Add tests that create real stdlib WAV files:

```python
def write_wav(path, seconds, sample_rate, channels):
    frames = int(seconds * sample_rate)
    audio = np.zeros(frames * channels, dtype=np.int16)
    with wave.open(str(path), "wb") as wav:
        wav.setnchannels(channels)
        wav.setsampwidth(2)
        wav.setframerate(sample_rate)
        wav.writeframes(audio.tobytes())


def make_wavs(tmp_path, mic_seconds, system_seconds):
    session_dir = make_session(tmp_path)
    write_wav(session_dir / "mic.wav", mic_seconds, 16_000, 1)
    write_wav(session_dir / "system.wav", system_seconds, 48_000, 2)
    return session_dir


def test_validate_capture_enforces_normal_duration_delta(tmp_path):
    session_dir = make_wavs(tmp_path, mic_seconds=10, system_seconds=13)
    with pytest.raises(session.CaptureInvalid, match="duration delta"):
        session.validate_capture(session_dir, max_duration_delta_s=2.0)
    channels = session.validate_capture(session_dir, max_duration_delta_s=None)
    assert channels["mic"].frames > 0
    assert channels["system"].frames > 0


def test_legacy_note_session_becomes_completed(tmp_path):
    session_dir = make_wavs(tmp_path, mic_seconds=1, system_seconds=1)
    note = tmp_path / "note.md"
    note.write_text("x", encoding="utf-8")
    (session_dir / "note.path").write_text(str(note), encoding="utf-8")
    assert session.ensure_legacy_manifest(session_dir).state is session.SessionState.COMPLETED
```

- [x] **Step 8: Implement WAV inspection and legacy migration**

`inspect_wav` opens with `wave.open`, requires PCM sample width 2, positive
sample rate, one or two channels, and positive frame count. It returns a
relative filename in `ChannelInfo`. `validate_capture` requires both
`mic.wav` and `system.wav`; when a delta is supplied, compare
`frames / sample_rate` and raise `CaptureInvalid` beyond it.

`ensure_legacy_manifest` returns an existing manifest unchanged. Otherwise it
creates and writes exactly one inferred manifest:

- `completed` when `note.path` names an existing file.
- `captured` when both WAVs validate with no duration limit.
- `interrupted` when files exist but do not both validate.
- `failed` when no capture files exist.

- [x] **Step 9: Run Task 1 tests and the MIRIS suite**

Run:

```bash
cd packages/miris
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 .venv/bin/python -m pytest tests/test_session.py -v
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 .venv/bin/python -m pytest -q
```

Expected: Task 1 tests PASS and the full suite matches or improves the recorded
baseline.

- [x] **Step 10: Commit**

```bash
git add packages/miris/miris/session.py packages/miris/tests/test_session.py
git commit -m "feat(miris): add durable session lifecycle"
```

---

### Task 2: Two-Channel Capture Readiness and Watchdog

**Files:**
- Modify: `packages/miris/miris/recorder.py`
- Modify: `packages/miris/miris/systemtap.py`
- Modify: `packages/miris/helper/audiotap.swift`
- Modify: `packages/miris/tests/test_recorder.py`
- Modify: `packages/miris/tests/test_systemtap.py`

**Interfaces:**
- Consumes: Task 1 `create`, `transition`, `validate_capture`, `SessionError`, `ChannelInfo`, and `SessionState`.
- Produces: `SystemTap.bytes_written() -> int` and existing `alive()`/`stop()` behavior.
- Produces: `record(session_dir: Path, stop: threading.Event, tap_factory=SystemTap, open_stream=None, now: Callable[[], float] = time.monotonic) -> SessionState`.
- Produces constants `START_TIMEOUT_S = 5.0`, `STALL_TIMEOUT_S = 2.0`, `WATCHDOG_INTERVAL_S = 0.25`, `QUEUE_CHUNKS = 50`, and `MANIFEST_INTERVAL_S = 1.0`.

- [x] **Step 1: Write failing SystemTap progress tests**

Add:

```python
def test_bytes_written_reports_owned_output(tmp_path):
    out = tmp_path / "out.wav"
    tap = SystemTap(out, binary=_fake_binary(tmp_path))
    assert tap.bytes_written() == 0
    out.write_bytes(b"RIFF" + b"x" * 100)
    assert tap.bytes_written() == 104


def test_stop_kills_helper_after_grace(tmp_path, monkeypatch):
    proc = FakeProcess(wait_raises_timeout=True)
    tap = SystemTap(tmp_path / "out.wav", binary=tmp_path / "helper")
    tap.proc = proc
    tap.stop(grace_s=0.01)
    assert proc.killed and proc.waited_after_kill
```

- [x] **Step 2: Run SystemTap tests and verify red**

Run:

```bash
cd packages/miris
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 .venv/bin/python -m pytest tests/test_systemtap.py -v
```

Expected: FAIL because `bytes_written` does not exist and the injected timeout
case is not covered.

- [x] **Step 3: Implement minimal progress observation**

Add only:

```python
def bytes_written(self) -> int:
    try:
        return self.out_wav.stat().st_size
    except FileNotFoundError:
        return 0
```

Keep `stop` as SIGTERM, bounded wait, SIGKILL, final wait. Do not add a helper
protocol unless the real fault probe later proves file size does not progress
while recording.

- [x] **Step 4: Write failing recorder readiness and failure tests**

Use deterministic fakes with `bytes_written`, `alive`, callback delivery, and a
fake monotonic clock. Add these cases:

```python
class GrowingFakeTap:
    def __init__(self, out_wav, die_after_checks=None):
        self.out_wav = out_wav
        self.die_after_checks = die_after_checks
        self.checks = 0
        self.stopped = False

    def start(self):
        with wave.open(str(self.out_wav), "wb") as wav:
            wav.setnchannels(2)
            wav.setsampwidth(2)
            wav.setframerate(48_000)
            wav.writeframes(np.zeros(4_800 * 2, dtype=np.int16).tobytes())

    def bytes_written(self):
        return self.out_wav.stat().st_size

    def alive(self):
        self.checks += 1
        return self.die_after_checks is None or self.checks < self.die_after_checks

    def stop(self, grace_s=5.0):
        self.stopped = True


def fake_stream(chunks, stop=None, status=None):
    class Stream:
        def __init__(self, callback):
            self.callback = callback

        def __enter__(self):
            for chunk in chunks:
                self.callback(chunk.reshape(-1, 1), len(chunk), None, status)
            if stop is not None:
                stop.set()
            return self

        def __exit__(self, *args):
            return False

    return Stream


def test_recording_state_requires_frames_from_both_channels(tmp_path):
    session.create(tmp_path)
    tap = GrowingFakeTap(tmp_path / "system.wav")
    stop = threading.Event()
    state = recorder.record(tmp_path, stop, tap_factory=lambda p: tap,
                            open_stream=fake_stream(
                                [np.zeros(1_600, dtype=np.float32)], stop=stop))
    assert state is SessionState.CAPTURED


def test_dead_system_tap_interrupts_capture(tmp_path):
    session.create(tmp_path)
    tap = GrowingFakeTap(tmp_path / "system.wav", die_after_checks=2)
    result = recorder.record(tmp_path, threading.Event(),
                             tap_factory=lambda p: tap,
                             open_stream=fake_stream(
                                 [np.zeros(1_600, dtype=np.float32)]))
    assert result is SessionState.INTERRUPTED
    assert session.load(tmp_path).error.code == "system_tap_died"


def test_microphone_callback_error_interrupts_capture(tmp_path):
    session.create(tmp_path)
    result = recorder.record(tmp_path, threading.Event(),
                             tap_factory=lambda p: GrowingFakeTap(p),
                             open_stream=fake_stream(
                                 [np.zeros(1_600, dtype=np.float32)],
                                 status="input overflow"))
    assert result is SessionState.INTERRUPTED
    assert session.load(tmp_path).error.code == "microphone_error"


def test_queue_overflow_is_not_silent(tmp_path):
    session.create(tmp_path)
    chunks = [np.zeros(1_600, dtype=np.float32)] * (recorder.QUEUE_CHUNKS + 1)
    result = recorder.record(tmp_path, threading.Event(),
                             tap_factory=lambda p: GrowingFakeTap(p),
                             open_stream=fake_stream(chunks))
    assert result is SessionState.INTERRUPTED
    assert session.load(tmp_path).error.code == "microphone_queue_overflow"
```

Also test a missing first microphone frame, missing first system frame, a
two-second stall after readiness, writer exception, and normal duration delta
beyond two seconds.

- [x] **Step 5: Run recorder tests and verify red**

Run:

```bash
cd packages/miris
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 .venv/bin/python -m pytest tests/test_recorder.py -v
```

Expected: FAIL because the current recorder publishes only a PID, uses an
unbounded queue, ignores callback status, and never watches the helper.

- [x] **Step 6: Implement the minimal watchdog**

Use `queue.Queue(maxsize=QUEUE_CHUNKS)`. In the callback, call `put_nowait` and
set one shared first-error value on `queue.Full` or a truthy status. The main
loop owns all manifest writes and checks the tap, frame counters, deadlines,
and stop event every 250 milliseconds.

The lifecycle must be:

```python
starting -> recording -> stopping -> captured
starting -> failed
starting | recording | stopping -> interrupted
```

Do not write `recording` until the microphone accepted frames and
`tap.bytes_written()` advanced beyond its initial header size. On failure,
preserve the first error, close the mic writer, stop the tap, validate without
a duration threshold, write `interrupted`, and return it. On normal stop,
validate with `max_duration_delta_s=2.0` before writing `captured`.

Update `recorder.main` in the same file to record its PID in the existing
manifest before capture, preserve the existing restrictive umask and signal
handlers, and remove only the compatibility `recorder.pid` file on exit. The
manifest retains the lifecycle result after the process ends.

- [x] **Step 7: Add the 48 kHz helper cap**

Change the Swift output format line to:

```swift
mSampleRate: min(asbd.mSampleRate, 48_000),
```

No new converter is required because `ExtAudioFile` already converts from the
client format to the file format. Compile it:

```bash
make -C packages/miris/helper clean
make -C packages/miris/helper
```

Expected: `helper/audiotap` builds successfully.

- [x] **Step 8: Fault-probe abrupt writer termination**

Run a disposable local capture through `miris.recorder`, wait until the
manifest reaches `recording`, send SIGKILL, and inspect both WAVs with
`session.inspect_wav`. Store only the observed command and result in the task
report. Reaching `recording` also proves that active `system.wav` growth is
observable. If the helper is alive but file growth cannot be observed, first
add a failing wrapper test, then add the smallest helper readiness/frame signal
and parse it in `SystemTap`. If both files reopen after SIGKILL, add no repair
implementation. If either produces a reproducible RIFF length error, add one
failing `test_repair_pcm_riff_*` to `test_session.py` and implement only the
deterministic RIFF/data length rewrite specified by the design.

- [x] **Step 9: Run focused and full Python tests**

```bash
cd packages/miris
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 .venv/bin/python -m pytest \
  tests/test_recorder.py tests/test_systemtap.py tests/test_session.py -v
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 .venv/bin/python -m pytest -q
```

Expected: PASS.

- [x] **Step 10: Commit**

```bash
git add packages/miris/miris/recorder.py packages/miris/miris/systemtap.py \
  packages/miris/helper/audiotap.swift packages/miris/tests/test_recorder.py \
  packages/miris/tests/test_systemtap.py packages/miris/miris/session.py \
  packages/miris/tests/test_session.py
git commit -m "feat(miris): stop unreliable two-channel recordings"
```

---

### Task 3: Manifest-Based CLI and Manual Recovery

**Files:**
- Modify: `packages/miris/miris/cli.py`
- Modify: `packages/miris/tests/test_cli.py`

**Interfaces:**
- Consumes: Task 1 lifecycle APIs and Task 2 recorder state contract.
- Produces: `wait_for_state(session_dir: Path, states: set[SessionState], timeout_s: float, sleep: Callable[[float], None] = time.sleep) -> SessionManifest`.
- Produces: `stop_recorder(session_dir: Path, manifest: SessionManifest, timeout_s: float = 10.0) -> SessionManifest`.
- Produces commands `miris recover <session-id>` and `miris process <session-id>`.
- Keeps `miris retry <session-id>` as a compatibility alias for `process`.

- [x] **Step 1: Write failing start and process-identity tests**

Add:

```python
def test_cmd_start_waits_for_recording_manifest(tmp_path, monkeypatch):
    cfg = _cfg(tmp_path)

    def fake_popen(args, **kwargs):
        session_dir = Path(args[-1])
        session.transition(session_dir, SessionState.RECORDING,
                           recorder_pid=os.getpid())
        return object()

    monkeypatch.setattr(cli.subprocess, "Popen", fake_popen)
    cli.cmd_start(cfg)
    created = next(cfg.sessions_dir.iterdir())
    assert session.load(created).state is SessionState.RECORDING


def test_stop_refuses_reused_pid(tmp_path, monkeypatch):
    cfg = _cfg(tmp_path)
    session_dir = cfg.sessions_dir / "20260914-143000"
    session_dir.mkdir(parents=True)
    session.create(session_dir)
    session.transition(session_dir, SessionState.RECORDING, recorder_pid=42)
    kill_calls = []
    monkeypatch.setattr(cli.os, "kill", lambda *args: kill_calls.append(args))
    monkeypatch.setattr(session, "process_matches", lambda *args, **kwargs: False)
    with pytest.raises(SystemExit, match="does not belong"):
        cli.cmd_stop(cfg)
    assert kill_calls == []
```

- [x] **Step 2: Run CLI tests and verify red**

```bash
cd packages/miris
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 .venv/bin/python -m pytest tests/test_cli.py -v
```

Expected: FAIL because start returns on PID creation and stop does not verify
process identity or enforce a deadline.

- [x] **Step 3: Implement manifest-based start and bounded stop**

`cmd_start` must call `reconcile_stale`, reject only `latest_active`, create the
session manifest, spawn the existing detached recorder, and wait at most five
seconds for `RECORDING`, `FAILED`, or `INTERRUPTED`. On timeout, signal only an
identity-verified recorder and report the manifest/log path.

`cmd_stop` must require `RECORDING`, verify the PID command and exact session
path, send SIGTERM, and wait at most ten seconds for `CAPTURED` or
`INTERRUPTED`. After the deadline, SIGKILL the verified process, transition to
`INTERRUPTED`, and return without processing. Only `CAPTURED` proceeds to the
pipeline.

- [x] **Step 4: Write failing manual recovery tests**

Add:

```python
def write_valid_wav(path, sample_rate, channels):
    with wave.open(str(path), "wb") as wav:
        wav.setnchannels(channels)
        wav.setsampwidth(2)
        wav.setframerate(sample_rate)
        wav.writeframes(b"\0\0" * sample_rate * channels)


def session_in_state(tmp_path, state):
    cfg = _cfg(tmp_path)
    session_dir = cfg.sessions_dir / "20260914-143000"
    session_dir.mkdir(parents=True)
    session.create(session_dir)
    session.transition(session_dir, SessionState.RECORDING, recorder_pid=42)
    if state is not SessionState.RECORDING:
        session.transition(session_dir, SessionState.INTERRUPTED,
                           recorder_pid=None)
    if state is SessionState.CAPTURED:
        session.transition(session_dir, SessionState.CAPTURED)
    return cfg, session_dir


def test_recover_requires_interrupted_state(tmp_path):
    cfg, session_dir = session_in_state(tmp_path, SessionState.CAPTURED)
    with pytest.raises(SystemExit, match="not interrupted"):
        cli.cmd_recover(cfg, session_dir.name)


def test_recover_valid_partial_audio_stops_at_captured(tmp_path):
    cfg, session_dir = session_in_state(tmp_path, SessionState.INTERRUPTED)
    write_valid_wav(session_dir / "mic.wav", 16_000, 1)
    write_valid_wav(session_dir / "system.wav", 48_000, 2)
    cli.cmd_recover(cfg, session_dir.name)
    manifest = session.load(session_dir)
    assert manifest.state is SessionState.CAPTURED
    assert not (session_dir / "transcript.md").exists()


def test_process_requires_captured_and_calls_pipeline(tmp_path, monkeypatch):
    cfg, session_dir = session_in_state(tmp_path, SessionState.CAPTURED)
    called = []
    monkeypatch.setattr(cli.pipeline, "process_session",
                        lambda cfg, path: called.append(path) or path / "note.md")
    cli.cmd_process(cfg, session_dir.name)
    assert called == [session_dir]
```

- [x] **Step 5: Implement recover, process, status, and retry alias**

`cmd_recover` refuses an identity-matching live recorder, validates both WAVs
with no duration threshold, updates channel metadata, and transitions only
`INTERRUPTED -> CAPTURED`. It does not call the pipeline.

`cmd_process` accepts only `CAPTURED` and calls `pipeline.process_session`.
`cmd_retry` delegates to `cmd_process`. `cmd_status` prints one stable line:

```text
<session-id> <state> [<error-code>]
```

Wire `recover` and `process` into argparse with one required `session_id`.

- [x] **Step 6: Run Task 3 tests and suite**

```bash
cd packages/miris
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 .venv/bin/python -m pytest tests/test_cli.py -v
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 .venv/bin/python -m pytest -q
```

Expected: PASS.

- [x] **Step 7: Commit**

```bash
git add packages/miris/miris/cli.py packages/miris/tests/test_cli.py
git commit -m "feat(miris): add bounded stop and manual recovery"
```

---

### Task 4: Retry-Safe Processing and Idempotent Vault Notes

**Files:**
- Modify: `packages/miris/miris/pipeline.py`
- Modify: `packages/miris/miris/vault.py`
- Modify: `packages/miris/tests/test_pipeline.py`
- Modify: `packages/miris/tests/test_vault.py`

**Interfaces:**
- Consumes: Task 1 `SessionState`, `SessionError`, `load`, `transition`, and `write`.
- Changes: `process_session` requires `CAPTURED`, transitions through `PROCESSING`, and returns to `CAPTURED` on any processing failure.
- Changes: `save_note` returns an existing note whose frontmatter contains the same exact JSON-quoted `session_id`.

- [x] **Step 1: Write failing state and cache tests**

Add:

```python
def write_system_wav(path):
    with wave.open(str(path), "wb") as wav:
        wav.setnchannels(2)
        wav.setsampwidth(2)
        wav.setframerate(48_000)
        wav.writeframes(np.zeros(48_000 * 2, dtype=np.int16).tobytes())


def pipeline_session(tmp_path, state):
    cfg = _cfg(tmp_path, "exit 1")
    session_dir = _session_with_mic(tmp_path)
    write_system_wav(session_dir / "system.wav")
    session.create(session_dir)
    session.transition(session_dir, SessionState.RECORDING,
                       recorder_pid=os.getpid())
    if state is SessionState.INTERRUPTED:
        session.transition(session_dir, state, recorder_pid=None)
    else:
        session.transition(session_dir, SessionState.STOPPING)
        session.transition(session_dir, SessionState.CAPTURED,
                           recorder_pid=None)
    return cfg, session_dir


def test_process_session_requires_captured(tmp_path):
    cfg, session_dir = pipeline_session(tmp_path, SessionState.INTERRUPTED)
    with pytest.raises(ValueError, match="captured"):
        pipeline.process_session(cfg, session_dir, transcriber_factory=FakeTranscriber)


def test_processing_failure_returns_to_captured(tmp_path, monkeypatch):
    cfg, session_dir = pipeline_session(tmp_path, SessionState.CAPTURED)
    monkeypatch.setattr(pipeline, "build_transcript", lambda *args: (_ for _ in ()).throw(RuntimeError("boom")))
    with pytest.raises(RuntimeError, match="boom"):
        pipeline.process_session(cfg, session_dir)
    manifest = session.load(session_dir)
    assert manifest.state is SessionState.CAPTURED
    assert manifest.error.code == "processing_failed"


def test_empty_transcript_cache_is_rebuilt(tmp_path, monkeypatch):
    cfg, session_dir = pipeline_session(tmp_path, SessionState.CAPTURED)
    (session_dir / "transcript.md").write_text("", encoding="utf-8")
    monkeypatch.setattr(pipeline, "build_transcript", lambda *args: "rebuilt")
    pipeline.process_session(cfg, session_dir, transcriber_factory=FakeTranscriber)
    assert (session_dir / "transcript.md").read_text(encoding="utf-8") == "rebuilt"
```

- [x] **Step 2: Run pipeline tests and verify red**

```bash
cd packages/miris
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 .venv/bin/python -m pytest tests/test_pipeline.py -v
```

Expected: FAIL because the pipeline has no lifecycle transitions and accepts an
empty cache.

- [x] **Step 3: Implement lifecycle-safe processing**

At entry, require `CAPTURED` and transition to `PROCESSING` with
`processing_pid=os.getpid()`. Write `transcript.md`, `notes.json`, and
`note.path` through one private helper:

```python
def _write_text_atomic(path: Path, text: str) -> None:
    tmp = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    tmp.write_text(text, encoding="utf-8")
    os.replace(tmp, path)
```

Reuse `transcript.md` only when it is nonempty and the manifest has
`transcript_ready=True`. After an atomic transcript write, update that flag.
On success transition to `COMPLETED` with `processing_pid=None` and
`note_path=str(note)`. On any exception, transition back to `CAPTURED` with
`processing_pid=None` and `SessionError("processing_failed", str(exc), now)`,
then re-raise so the tray can notify.

Do not create a transcript-only vault note for LLM failure. The valid capture
and transcript remain in the session for retry.

- [x] **Step 4: Write failing idempotent-note tests**

Add:

```python
FIXED_DATE = dt.date(2026, 9, 14)


def test_save_note_reuses_same_session_after_crash(tmp_path):
    cfg = _cfg(tmp_path)
    first = vault.save_note(cfg, NOTES, "old", "session-1", date=FIXED_DATE)
    second = vault.save_note(cfg, NOTES, "new", "session-1", date=FIXED_DATE)
    assert second == first
    assert "new" in first.read_text(encoding="utf-8")


def test_same_title_different_session_keeps_collision_behavior(tmp_path):
    cfg = _cfg(tmp_path)
    first = vault.save_note(cfg, NOTES, "a", "session-1", date=FIXED_DATE)
    second = vault.save_note(cfg, NOTES, "b", "session-2", date=FIXED_DATE)
    assert first != second
    assert second.name.endswith("-2.md")
```

- [x] **Step 5: Implement idempotent atomic vault writes**

Before choosing a collision suffix, inspect existing candidates for the exact
frontmatter line:

```python
session_line = f"session: {json.dumps(session_id)}"
```

Reuse only a file whose frontmatter contains that whole line. Write the final
note atomically through a temp file in the vault directory and `os.replace`.
Do not scan outside the target category directory.

- [x] **Step 6: Run Task 4 tests and suite**

```bash
cd packages/miris
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 .venv/bin/python -m pytest \
  tests/test_pipeline.py tests/test_vault.py -v
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 .venv/bin/python -m pytest -q
```

Expected: PASS.

- [x] **Step 7: Commit**

```bash
git add packages/miris/miris/pipeline.py packages/miris/miris/vault.py \
  packages/miris/tests/test_pipeline.py packages/miris/tests/test_vault.py
git commit -m "fix(miris): preserve captures across processing retries"
```

---

### Task 5: Manifest-Driven Tray UI

**Files:**
- Modify: `apps/tray/src-tauri/src/miris.rs`
- Modify: `apps/tray/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: Task 1 JSON schema and Task 3 `recover`/`process` commands.
- Produces: Rust `SessionState`, `SessionError`, `ChannelInfo`, and `SessionManifest` serde types mirroring schema version 1.
- Produces: `SessionView { path: PathBuf, manifest: SessionManifest, derived_state: SessionState }` and `MirisSnapshot { active: Option<SessionView>, pending: Option<SessionView>, last_note: Option<PathBuf> }`.
- Produces: `snapshot(sessions_dir: &Path) -> MirisSnapshot` with `active`, `pending`, and `last_note`; tests use private `snapshot_with_command_reader` to avoid inspecting real processes.
- Produces: `recover(miris_bin: &str, session_id: &str) -> Result<(), String>` and `process(miris_bin: &str, session_id: &str) -> Result<PathBuf, String>`.

- [ ] **Step 1: Write failing manifest parsing and selection tests**

Add Rust tests using `tempfile`:

```rust
fn write_manifest(root: &Path, id: &str, state: &str, pid: Option<i32>) {
    let dir = root.join(id);
    std::fs::create_dir_all(&dir).unwrap();
    let body = serde_json::json!({
        "schema_version": 1,
        "session_id": id,
        "state": state,
        "created_at": "2026-09-14T14:30:00-03:00",
        "updated_at": "2026-09-14T14:30:01-03:00",
        "recorder_pid": pid,
        "processing_pid": serde_json::Value::Null,
        "channels": {"mic": null, "system": null},
        "error": serde_json::Value::Null,
        "note_path": serde_json::Value::Null,
        "transcript_ready": false
    });
    std::fs::write(dir.join("session.json"), body.to_string()).unwrap();
}


#[test]
fn snapshot_selects_active_and_newest_pending_independently() {
    let tmp = tempfile::tempdir().unwrap();
    write_manifest(tmp.path(), "20260914-100000", "interrupted", None);
    write_manifest(tmp.path(), "20260914-110000", "captured", None);
    write_manifest(tmp.path(), "20260914-120000", "recording", Some(std::process::id() as i32));
    let snap = snapshot_with_command_reader(tmp.path(), |_, dir| {
        format!("python -m miris.recorder {}", dir.display())
    });
    assert_eq!(snap.active.unwrap().manifest.state, SessionState::Recording);
    assert_eq!(snap.pending.unwrap().manifest.session_id, "20260914-110000");
}

#[test]
fn stale_recording_is_derived_as_interrupted() {
    let tmp = tempfile::tempdir().unwrap();
    write_manifest(tmp.path(), "20260914-120000", "recording", Some(99_999_999));
    let snap = snapshot_with_command_reader(tmp.path(), |_, _| String::new());
    assert!(snap.active.is_none());
    assert_eq!(snap.pending.unwrap().derived_state, SessionState::Interrupted);
}
```

- [ ] **Step 2: Run Rust tests and verify red**

```bash
cd apps/tray/src-tauri
cargo test --lib miris::tests
```

Expected: FAIL because the tray understands only PID-based idle/recording.
If Cargo cannot write its global cache under the sandbox, rerun this exact
command through the normal approval mechanism. Do not create a fresh cache and
redownload the dependency graph merely to avoid the permission boundary.

- [ ] **Step 3: Implement serde manifest reads and derived state**

Use serde derives and `#[serde(rename_all = "snake_case")]`. Reject unknown
`schema_version` values from active selection, log the parse failure, and keep
scanning. Keep the tray read-only. Check process identity with `ps -p <pid> -o
command=` and require both `miris.recorder` and the exact session path. Derive
a dead or mismatched `recording`, `starting`, or `stopping` process as
interrupted and a dead `processing` process as captured for display. Python
persists the correction on the next command.

`snapshot` must select active and pending independently so an interrupted
session does not block a new recording.

- [ ] **Step 4: Write failing menu-state tests**

Extract pure helpers and test them:

```rust
#[test]
fn menu_labels_follow_lifecycle() {
    assert_eq!(primary_action(None), ("miris_start", "Gravar reunião", true));
    assert_eq!(primary_action(Some(SessionState::Starting)).1, "Iniciando gravação...");
    assert_eq!(primary_action(Some(SessionState::Recording)).0, "miris_stop");
    assert_eq!(pending_action(SessionState::Interrupted).0, "miris_recover");
    assert_eq!(pending_action(SessionState::Captured).0, "miris_process");
}

#[test]
fn quit_is_disabled_only_during_active_work() {
    assert!(!quit_enabled(Some(SessionState::Recording)));
    assert!(!quit_enabled(Some(SessionState::Processing)));
    assert!(quit_enabled(None));
    assert!(quit_enabled(Some(SessionState::Interrupted)));
}
```

- [ ] **Step 5: Implement tray actions and notifications**

Keep one primary meeting item and add one pending item only when present. Put
the session id in action ids as `miris_recover:<id>` and
`miris_process:<id>`. Parse with `strip_prefix`; never shell-interpolate it.

Use the existing `processing: AtomicBool` to suppress duplicate stop, recover,
and process clicks. Run CLI calls off the UI thread. Report distinct messages:

```text
Falha ao iniciar gravação: <error>
Gravação interrompida: <error>
Falha ao recuperar gravação: <error>
Falha ao processar reunião: <error>
Nota salva: <filename>
```

Disable the existing `quit` menu item when `active` is present. Keep LARIS
daemon shutdown unchanged when quit is enabled.

- [ ] **Step 6: Run focused Rust tests, full Rust tests, and frontend build**

```bash
cd apps/tray/src-tauri
cargo test --lib

cd ..
npm run build
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/tray/src-tauri/src/miris.rs apps/tray/src-tauri/src/lib.rs
git commit -m "feat(tray): surface reliable meeting lifecycle"
```

---

### Task 6: Two-Channel Doctor and Automated Release Gate

**Files:**
- Modify: `packages/miris/miris/cli.py`
- Modify: `packages/miris/tests/test_cli.py`

**Interfaces:**
- Consumes: completed Task 2 recorder contract.
- Changes: `miris doctor` verifies microphone and system audio in one bounded three-second temporary capture and reports each channel separately.

- [x] **Step 1: Write a failing doctor orchestration test**

Add:

```python
def test_doctor_checks_both_capture_channels(tmp_path, monkeypatch, capsys):
    cfg = _cfg(tmp_path)
    monkeypatch.setattr(cli, "run_doctor_capture", lambda cfg, duration_s=3.0: {
        "mic": ChannelInfo("mic.wav", 48_000, 16_000, 1),
        "system": ChannelInfo("system.wav", 144_000, 48_000, 2),
    })
    assert cli.cmd_doctor(cfg, exit_on_finish=False) is True
    output = capsys.readouterr().out
    assert "[ok] microphone capture" in output
    assert "[ok] system-audio capture" in output
```

Also test either missing channel returns false and always tears down the helper.

- [x] **Step 2: Run the focused test and verify red**

```bash
cd packages/miris
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 .venv/bin/python -m pytest tests/test_cli.py -k doctor -v
```

Expected: FAIL because current doctor captures only system audio.

- [x] **Step 3: Implement bounded two-channel doctor capture**

Add `run_doctor_capture(cfg, duration_s=3.0) -> dict[str, ChannelInfo]`. It uses
the real recorder in a temporary session, schedules the stop event after three
seconds, and returns `validate_capture(..., max_duration_delta_s=2.0)`. Keep the
existing helper, vault, categories, and LLM checks. `cmd_doctor` prints separate
microphone and system results and returns a bool when `exit_on_finish=False`;
the CLI path still exits 0 or 1.

- [x] **Step 4: Run every automated gate**

```bash
cd packages/miris
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 .venv/bin/python -m pytest -q

cd ../../apps/tray/src-tauri
cargo test --lib

cd ..
npm run build
```

Expected: all commands exit 0. Record exact counts in the task report.

- [x] **Step 5: Commit**

```bash
git add packages/miris/miris/cli.py packages/miris/tests/test_cli.py
git commit -m "test(miris): verify both channels in doctor"
```

---

### Task 7: Owner-Mac Validation, Documentation, and v1.0.0 Gate

**Files:**
- Create: `docs/releases/2026-09-14-v1.0.0-validation.md`
- Modify: `docs/REPO-STATUS-AUDIT.md`
- Modify: `docs/BACKLOG.md`
- Modify: `docs/INSTALL.md`
- Modify: `packages/miris/README.md`
- Modify: `packages/miris/docs/superpowers/specs/2026-07-16-miris-core-design.md`
- Modify: `packages/miris/docs/superpowers/plans/2026-07-16-miris-core.md`
- Modify after all gates pass: `packages/miris/pyproject.toml`
- Modify after all gates pass: `apps/tray/package.json`
- Modify after all gates pass: `apps/tray/package-lock.json`
- Modify after all gates pass: `apps/tray/src-tauri/Cargo.toml`
- Modify after all gates pass: `apps/tray/src-tauri/Cargo.lock`
- Modify after all gates pass: `apps/tray/src-tauri/tauri.conf.json`

**Interfaces:**
- Produces: an evidence ledger with commands, hardware, results, failure causes, and acceptance verdict.
- Produces: coordinated `1.0.0` version fields only after PASS.
- Does not create the git tag without a final explicit user authorization after the evidence ledger says PASS.

- [ ] **Step 1: Create the validation ledger before manual testing**

Use this exact structure, with checkboxes initially unchecked and result fields
empty strings rather than claims:

```markdown
# VOXIS v1.0.0 Validation

## Environment

- Commit: `<git rev-parse HEAD>`
- macOS: `<sw_vers -productVersion>`
- Mac model: `<system_profiler SPHardwareDataType>`
- Input device: `<selected device>`
- Output device: `<selected device>`

## Automated gates

| Command | Exit | Result |
|---|---:|---|
| MIRIS pytest |  |  |
| Tray cargo test --lib |  |  |
| Tray npm run build |  |  |

## Failure matrix

| Case | Expected | Observed | Verdict |
|---|---|---|---|
| Microphone denied | Startup fails, no false recording state |  |  |
| System audio denied | Startup fails, no false recording state |  |  |
| Helper terminated | Both channels stop, interrupted in <= 4s |  |  |
| Recorder terminated | Restart shows manual recovery |  |  |
| Microphone disconnected | Both channels stop, interrupted in <= 4s |  |  |
| LLM failure | Captured remains retryable |  |  |
| Quit during active work | Quit disabled |  |  |

## Four-hour soak

- Started:
- Finished:
- Mic duration:
- System duration:
- Duration delta:
- Orphan processes:
- Final manifest state:
- Transcript contains `Eu`:
- Transcript contains `Eles`:
- Unacknowledged log errors:
- Verdict:

## Release verdict

`BLOCKED` until every row is PASS.
```

- [ ] **Step 2: Build the personal release candidate**

Run:

```bash
cd apps/tray
npm run tauri build
```

Expected: exit 0 with a VOXIS application bundle under
`src-tauri/target/release/bundle/macos/`. Record the exact bundle path and
version in the ledger. Use this same bundle for every manual gate.

- [ ] **Step 3: Run and record the permission and injected-failure matrix**

Use the built VOXIS tray, not direct CLI interaction, except for killing the
named test processes. For every case, preserve its session directory and paste
the exact notification, manifest error code, and elapsed detection time into
the ledger. A mismatch is a failed gate and returns to the responsible task;
do not edit the expected result.

- [ ] **Step 4: Run and record the four-hour soak**

Play known system audio and provide known microphone speech at the beginning,
middle, and end. Start and stop from the tray. Record WAV durations using
`session.inspect_wav`, check processes with `pgrep -fl`, process the session,
and verify both `Eu` and `Eles` transcript labels. PASS requires duration delta
at most two seconds, no orphan, no unacknowledged error, and `completed`.

- [ ] **Step 5: Re-run automated gates after the soak**

Run the exact Task 6 commands and copy counts and exit codes into the ledger.
Expected: all exit 0.

- [ ] **Step 6: Reconcile documentation from evidence**

Update the audit with the final commit and verification snapshot. Move only
verified completed items out of the open backlog. Mark the old MIRIS spec and
plan `Implemented` while linking this spec, plan, and validation ledger. Update
INSTALL and MIRIS README with:

```text
- Tray is the supported recording interface.
- Recording requires both microphone and system-audio permissions.
- Interrupted sessions are recovered manually from the tray.
- Raw session audio remains under ~/.miris/sessions/<session-id>/.
- Quit is disabled while recording or processing.
```

Do not rewrite unrelated historical sections.

- [ ] **Step 7: Update all versions only after a PASS verdict**

Set `1.0.0` in Python, npm, Cargo, Cargo lock root package, and Tauri config.
Use package managers only for their own lockfile synchronization:

```bash
cd apps/tray
npm install --package-lock-only
cd src-tauri
cargo check
cargo check --locked
```

Then rerun all automated gates. If any fails, restore the ledger verdict to
`BLOCKED`; do not tag.

- [ ] **Step 8: Run negative-flow review on the evidence ledger**

Attack every PASS row against its raw command or artifact. Classify and amend
real holes, fragilities, and one-Mac meta-risks. Leave the explicit limitations
in the ledger. Change the final verdict to `PASS` only when no real hole
remains.

- [ ] **Step 9: Commit the release candidate documentation and versions**

```bash
git add docs/REPO-STATUS-AUDIT.md docs/BACKLOG.md docs/INSTALL.md \
  docs/releases/2026-09-14-v1.0.0-validation.md packages/miris/README.md \
  packages/miris/docs/superpowers/specs/2026-07-16-miris-core-design.md \
  packages/miris/docs/superpowers/plans/2026-07-16-miris-core.md \
  packages/miris/pyproject.toml apps/tray/package.json apps/tray/package-lock.json \
  apps/tray/src-tauri/Cargo.toml apps/tray/src-tauri/Cargo.lock \
  apps/tray/src-tauri/tauri.conf.json
git commit -m "release: prepare VOXIS v1.0.0"
```

- [ ] **Step 10: Stop for explicit tag authorization**

Report the ledger path, commit, exact test counts, soak result, and remaining
limitations. Ask the user before running:

```bash
git tag -a v1.0.0 -m "VOXIS v1.0.0"
```

Do not push or publish a release unless separately requested.
