# MIRIS Reliable Recording Design

**Date:** 2026-09-14
**Status:** Approved
**Product scope:** Personal macOS app on one Apple Silicon Mac
**Source audit:** `docs/REPO-STATUS-AUDIT.md`

## Goal

Make the VOXIS tray a trustworthy control surface for recording meetings and
interviews. A recording is trustworthy only when microphone and system audio
are both active, monitored, recoverable after failure, and represented
honestly in the UI.

The release target is `v1.0.0`. The tag may be created only after every
automated and manual release gate in this document passes.

## Product decisions

- The release supports macOS 14.2 or newer on Apple Silicon only.
- The app is for one owner's Mac. Public distribution, notarization, generic
  onboarding, and support for different hardware are not release goals.
- The tray is the official user interface. The MIRIS CLI remains an internal
  diagnostic, testing, and recovery interface.
- Microphone and system audio are both mandatory. Failure of either channel
  stops both channels.
- An interrupted recording preserves all recoverable audio written before the
  failure.
- Interrupted recordings require an explicit manual recovery action before
  transcription or note generation.
- A normal stop processes the captured recording automatically.
- A processing failure never invalidates or deletes a valid capture.
- The release must pass a continuous four-hour recording on the owner's Mac.
- Rust orchestration and Tauri do not make capture portable. Windows and Linux
  adapters are out of scope.

## Existing system

MIRIS currently uses a detached Python recorder. `sounddevice` writes
`mic.wav`, while a Swift Core Audio Process Tap helper writes `system.wav`.
The tray invokes the MIRIS CLI and infers recording state from a live PID.
After stop, Python transcribes both channels, generates structured notes with
the configured LLM command, and writes a Markdown note into the configured
vault.

The implementation already preserves raw WAV files and has unit coverage for
the recorder, helper wrapper, pipeline, CLI, and tray integration. It does not
yet prove that both channels are ready before reporting success, monitor both
channels throughout a session, bound stop waits, or expose interrupted
recordings for manual recovery. The current `doctor` command checks system
audio but not the complete two-channel contract described by the old spec.

## Architecture

The current capture architecture remains in place:

```text
VOXIS tray
  -> MIRIS CLI
    -> detached Python recorder
      -> sounddevice microphone -> mic.wav
      -> Swift Core Audio helper -> system.wav
    -> Python transcription and notes pipeline
      -> vault Markdown note
```

Reliability is added through one session manifest, explicit lifecycle rules,
bounded waits, channel health monitoring, and recoverable file validation. No
database, background service, job framework, or new capture dependency is
introduced.

The Swift helper caps system WAV output at 48 kHz, 16-bit stereo. The current
tap can inherit a 96 kHz output-device rate, which would exceed classic RIFF's
4 GiB limit during the required four-hour session. Native `ExtAudioFile`
already performs sample-rate conversion, and transcription later reduces the
signal to 16 kHz, so retaining a higher rate adds file size without product
value. Four hours at the capped format is about 2.8 GB.

## Session manifest

Each session directory contains `session.json`. Python owns writes. Writes use
a temporary file in the same directory followed by `os.replace`. The tray is
a read-only consumer.

Required fields:

```json
{
  "schema_version": 1,
  "session_id": "20260914-143000",
  "state": "recording",
  "created_at": "2026-09-14T14:30:00-03:00",
  "updated_at": "2026-09-14T14:30:06-03:00",
  "recorder_pid": 12345,
  "processing_pid": null,
  "channels": {
    "mic": {
      "path": "mic.wav",
      "frames": 96000,
      "sample_rate": 16000,
      "channels": 1
    },
    "system": {
      "path": "system.wav",
      "frames": 288000,
      "sample_rate": 48000,
      "channels": 2
    }
  },
  "error": null,
  "note_path": null,
  "transcript_ready": false
}
```

`recorder_pid` may be null after capture ends. `processing_pid` is set only
while the post-capture pipeline is running. Channel values may be null
while the session is `starting` or when startup fails before the channel is
opened. `error` is null or an object with `code`, `message`, and `occurred_at`.
`note_path` is populated only after the vault write succeeds.
`transcript_ready` becomes true only after an atomic nonempty transcript write.

The state values are:

- `starting`: the session directory exists but both channels are not ready.
- `recording`: both channels have produced frames and remain healthy.
- `stopping`: a normal stop is closing both channels.
- `captured`: both WAV files passed structural validation and await or survived
  processing.
- `processing`: transcription, note generation, or vault persistence is
  running for a valid capture.
- `completed`: the note was persisted and `note_path` was recorded.
- `interrupted`: capture started but a channel, recorder, or host lifecycle
  failure prevented a normal completion.
- `failed`: startup ended before a valid two-channel recording began.

Allowed transitions:

```text
starting -> recording | failed | interrupted
recording -> stopping | interrupted
stopping -> captured | interrupted
captured -> processing
processing -> completed | captured
interrupted -> captured
```

The last transition is allowed only through explicit manual recovery after
both files pass recovery validation. A stale `processing` session whose
recorded processing process is gone returns to `captured` with an error.
Terminal `failed` sessions cannot be processed because they never established
a valid two-channel capture.

## Start flow

1. The tray invokes `miris start` off the UI thread.
2. The CLI rejects a second active session. Active means `starting`,
   `recording`, `stopping`, or `processing`. Older `captured`, `interrupted`,
   `failed`, and `completed` sessions do not block a new meeting.
3. The CLI creates the session directory and initial `starting` manifest.
4. The detached recorder starts the system helper and microphone stream.
5. Each channel must produce at least one frame within five seconds.
6. The recorder writes `recording` only after both conditions are true.
7. The CLI returns success only after observing `recording`.
8. Startup timeout or channel startup failure stops any channel that did start,
   records `failed`, and returns a useful error.

A PID file may remain for process signaling compatibility, but it is not proof
that recording is active. Before signaling a PID, the CLI verifies that the
live process command belongs to `miris.recorder` and names the same session
directory. A reused PID must never receive a stop signal.

## Recording health

The recorder owns a small watchdog loop. It checks:

- The system helper process is alive.
- The microphone callback has not reported a stream error.
- The bounded microphone queue has not overflowed.
- Both channel frame counters continue to advance while callbacks are active.
- Manifest writes and WAV writes succeed.

Silence is valid audio and must not be treated as failure. Health is based on
frame delivery and file progress, not amplitude.

The watchdog samples health every 250 milliseconds. A channel that delivers no
new frames for two seconds is failed. This keeps recorder detection within the
four-second user-notification target after the tray's existing two-second poll.
System sleep therefore interrupts an active recording instead of pretending
that an unobserved interval was captured.

The microphone queue holds at most 50 existing 100-millisecond chunks. Five
seconds of backlog means the writer is no longer keeping pace. The callback
uses a nonblocking enqueue and records overflow as a failure because blocking
the audio callback or silently dropping frames would make timestamps and
content unreliable.

The manifest updates channel counters at most once per second. The microphone
counter comes from accepted callback frames. `SystemTap` derives system frames
from observed WAV growth and its format header. If the active file cannot
expose reliable progress, the Swift helper must emit a minimal readiness and
frame-count signal for the wrapper. This signal is not a new service or
protocol beyond the owned child process.

Any channel failure causes the recorder to:

1. Record the first failure cause.
2. Stop accepting new frames.
3. Close the microphone writer and stop the system helper.
4. Validate and preserve both partial files.
5. Write `interrupted` atomically.
6. Exit without starting the processing pipeline.

The tray polls the manifest on its existing two-second interval, updates the
recording icon, and notifies the user. The notification target is at most four
seconds after the recorder detects the failure.

## Normal stop and processing

1. The tray invokes `miris stop` off the UI thread and prevents duplicate stop
   requests.
2. The CLI requests recorder termination.
3. The recorder writes `stopping`, closes both writers, and validates both WAV
   files.
4. The helper receives five seconds to finalize. The recorder process receives
   ten seconds total to exit. A process that exceeds its deadline receives a
   forced termination and the session becomes `interrupted`.
5. Structurally valid files whose durations differ by no more than two seconds
   produce `captured`. A larger mismatch on normal stop becomes `interrupted`.
6. The pipeline writes `processing`, transcribes both channels, generates the
   note, writes it to the vault, records `note_path`, and writes `completed`.

If transcription, LLM execution, or vault persistence fails, the pipeline
records the error and returns the session to `captured`. The tray offers a new
processing attempt. It does not call the recording interrupted and does not
delete or replace the audio.

Transcript and note metadata writes use temporary files followed by atomic
replacement. The existing transcript cache may be reused only when it is
nonempty and its manifest state proves the write completed. A failed, partial,
or empty transcript is not treated as a valid cache entry.

Vault persistence is idempotent per `session_id`. Retrying after a crash must
reuse a note whose frontmatter already names that session instead of creating
`-2`, `-3`, or later duplicates. A different session with the same title still
uses the existing collision behavior.

## Crash discovery and manual recovery

At tray startup and before each MIRIS action, session discovery examines
nonterminal manifests. A session in `starting`, `recording`, or `stopping`
whose recorder process is no longer the recorded live process is changed to
`interrupted` with a crash-recovery error code. A session in `processing`
whose processing process is gone returns to `captured` with a retryable error.
The tray also derives these stale states while reading, so a full disk cannot
leave the UI claiming that a dead process is still recording merely because
the manifest cannot be updated.

The tray exposes `Recover recording` for the newest interrupted session.
Clicking the action invokes `miris recover <session-id>` and is the explicit
manual decision. No additional confirmation dialog is required.
Recovery performs these steps:

1. Refuse recovery while the recorded process is still alive.
2. Inspect both files without modifying the originals.
3. Repair WAV container metadata only when payload and format metadata make
   the repair deterministic.
4. Write repaired output through a temporary file and replace the damaged file
   only after the repaired file can be reopened and validated.
5. Require both channels to contain frames and record their duration mismatch.
6. Move the session to `captured` on success.
7. Leave the session `interrupted` with a precise reason on failure.

Recovery does not automatically start transcription. After successful
recovery, the tray presents a separate `Process recording` action.

The implementation must first fault-test the existing Python and Swift WAV
writers. If their partial files are already readable after forced process
termination, no repair code is added. Repair support is implemented only for
reproducible invalid output. Supported repair is limited to PCM RIFF/WAVE: use
the readable `fmt ` chunk or manifest channel format, preserve payload bytes,
and rewrite the RIFF and `data` lengths into a temporary file. Unknown codecs,
ambiguous payload boundaries, or files over the RIFF 32-bit size limit remain
`interrupted` rather than being guessed at.

## Tray behavior

The tray derives meeting state from `session.json` instead of treating a live
PID as sufficient. Its primary action represents only the active session:

- No active session: `Record meeting`, enabled.
- `starting`: `Starting recording...`, disabled.
- `recording`: `Stop recording (<minutes> min)`, enabled, recording icon.
- `stopping`: `Stopping recording...`, disabled.
- `processing`: `Processing meeting...`, disabled.

When there is no active session, a second action represents the newest pending
session. `captured` shows `Process recording`; `interrupted` shows `Recover
recording`. Completing one pending action reveals the next newest pending
session on the following poll. This keeps old work accessible without blocking
an urgent new recording or requiring a new window.

Notifications distinguish startup failure, capture interruption, recovery
failure, processing failure, and successful note creation. The existing
`Last note` action continues to use the persisted note path.

`Quit` is disabled while a session is `starting`, `recording`, `stopping`, or
`processing`. The user stops or finishes the active operation first. This
prevents a detached recording without adding a dialog plugin or a second stop
path. The app may quit when the latest session is `captured`, `completed`,
`interrupted`, or `failed`.

## Files and responsibilities

- Create `packages/miris/miris/session.py`: manifest schema, atomic writes,
  transition validation, discovery, WAV validation, and deterministic recovery.
- Modify `packages/miris/miris/recorder.py`: readiness, bounded queue, frame
  counters, watchdog, joint shutdown, and manifest updates.
- Modify `packages/miris/miris/systemtap.py`: bounded startup and health details
  needed by the recorder.
- Modify `packages/miris/miris/cli.py`: manifest-based start, bounded stop,
  status, `recover`, and processing retry commands.
- Modify `packages/miris/miris/pipeline.py`: `captured`, `processing`, and
  `completed` transitions plus retry-safe cache handling.
- Modify `apps/tray/src-tauri/src/miris.rs`: deserialize session manifests and
  invoke the internal CLI actions.
- Modify `apps/tray/src-tauri/src/lib.rs`: manifest-driven menu, notifications,
  recovery, processing retry, and safe quit behavior.
- Modify `packages/miris/helper/audiotap.swift`: cap output at 48 kHz. Add a
  minimal readiness or frame-count signal only if fault tests demonstrate that
  progress cannot be observed correctly from the Python wrapper.
- Add focused Python and Rust tests beside the existing MIRIS and tray tests.
- Update install, backlog, status audit, old MIRIS spec, and old MIRIS plan only
  after implementation evidence exists.

## Automated verification

Every production change starts from a failing focused test. Required cases:

- Start remains `starting` until both channels produce frames.
- Start fails within five seconds when either channel never becomes ready.
- A second recording cannot start while a session is active, but an older
  pending recovery does not block a new recording.
- System helper death interrupts both channels.
- Microphone callback error interrupts both channels.
- Microphone queue overflow interrupts both channels without silent data loss.
- Manifest write failure interrupts capture and remains diagnosable from the
  recorder log.
- System output above 48 kHz is converted to 48 kHz and remains within the RIFF
  size limit for four hours of stereo 16-bit audio.
- Normal stop produces two readable WAV files and `captured`.
- Stop timeout forces termination and produces `interrupted`.
- Dead recorder discovery converts abandoned nonterminal state to
  `interrupted`.
- Interrupted sessions never process automatically.
- Recovery refuses a live session and incomplete or incompatible channel data.
- Valid recovery produces `captured` without modifying valid audio payload.
- Processing failure returns to `captured` and preserves audio.
- A dead processing process returns a stale `processing` session to `captured`.
- Successful processing produces `completed` with an existing `note_path`.
- Retrying after a crash between vault write and manifest update does not
  create a duplicate note for the same session.
- Empty or partial transcript cache is not reused.
- Tray menu and icon states match every manifest state.
- Duplicate stop, recovery, and process actions are suppressed.
- Quit is disabled during capture or processing and enabled in stable states.

The required automated commands are:

```bash
cd packages/miris
PYTEST_DISABLE_PLUGIN_AUTOLOAD=1 python3 -m pytest -q

cd apps/tray
npm run build

cd apps/tray/src-tauri
cargo test --lib
```

The Python suite currently has one environment baseline failure because
`torchaudio` is absent from the active interpreter. The Rust suite is currently
blocked in the sandbox because Cargo cannot write its global registry cache.
Neither condition may be recorded as a passing release gate. Execution must
use the project environment or an approved writable cache and report the real
results.

## Owner-Mac release validation

Record the macOS version, Mac model, input device, output device, VOXIS commit,
and build version with the results.

Required manual cases:

- Permission denied for microphone.
- Permission denied for system audio.
- System helper terminated during recording.
- Recorder process terminated during recording.
- Microphone disconnected during recording.
- Normal meeting recording through the tray to a saved note.
- LLM command failure followed by successful processing retry.
- App restart with an interrupted session followed by manual recovery.
- Quit remains disabled during capture and becomes enabled after the session
  reaches a stable state.
- Continuous four-hour capture with both channels producing known periodic
  input.

The four-hour result passes only when:

- Both WAV files reopen successfully.
- Both contain frames for the session.
- Their durations differ by no more than two seconds.
- No recorder or helper process remains after stop.
- The manifest reaches `completed` after processing.
- The note contains transcript entries from both channel labels.
- Recorder and helper logs contain no unacknowledged error.

## Release sequence

1. Reproduce and lock the current baseline.
2. Add the manifest and lifecycle tests.
3. Add capture readiness and watchdog behavior.
4. Add bounded stop, crash discovery, and manual recovery.
5. Integrate manifest states and actions into the tray.
6. Harden pipeline retries without changing successful note behavior.
7. Run the full automated gate.
8. Run the owner-Mac failure matrix and four-hour test.
9. Reconcile the audit, backlog, old spec, old plan, installation guide, and
   version fields against the verified implementation.
10. Build the personal release and create `v1.0.0` only if every gate passes.

Implementation may parallelize independent Python lifecycle, Rust tray, and
verification/documentation work only after the manifest contract is merged.
Parallel workers must have non-overlapping file ownership and integrate through
the exact schema and transitions in this document.

## Limitations

- The release is validated on one Mac and does not establish reliability on
  other hardware or macOS versions.
- The four-hour test is one bounded soak test, not a statistical reliability
  guarantee.
- A normal process crash or host restart is recoverable when the operating
  system has persisted the written bytes. Sudden physical power loss is not a
  release guarantee.
- Audio semantics are not continuously inspected. The watchdog proves frame
  delivery and persistence, not that the selected device captured the intended
  speaker.
- Local storage exhaustion can still interrupt a recording. The interruption
  must be explicit when the manifest can be written and preserve already
  persisted data. If the disk is completely full, the tray derives failure
  from the dead process and stale manifest. The app does not reserve four hours
  of disk space upfront.
- Meeting and interview reliability share the same capture path. They differ
  only in classification and vault routing, so separate capture engines are
  unnecessary.

## Out of scope

- Windows or Linux support.
- A full Rust or native capture rewrite.
- Public code signing and notarization.
- Automatic meeting detection.
- Live transcription.
- Cloud synchronization or backup.
- Speaker diarization beyond microphone versus system channels.
- A transcription word-error-rate benchmark.
- A database or durable job queue.
