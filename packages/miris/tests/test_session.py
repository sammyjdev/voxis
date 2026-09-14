import datetime as dt
import io
import json
import struct
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
    session.transition(session_dir, session.SessionState.RECORDING, recorder_pid=42)
    session.transition(session_dir, session.SessionState.STOPPING)
    session.transition(session_dir, session.SessionState.CAPTURED, recorder_pid=None)


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
    session.transition(session_dir, session.SessionState.PROCESSING, processing_pid=321)
    result = session.transition(
        session_dir,
        session.SessionState.CAPTURED,
        processing_pid=None,
        error=session.SessionError("processing_failed", "boom", FIXED_ISO),
    )
    assert result.state is session.SessionState.CAPTURED


def test_process_matches_requires_module_and_session(tmp_path):
    session_dir = tmp_path / "s1"

    def reader(pid):
        return f"python -m miris.recorder {session_dir}"

    assert session.process_matches(42, "miris.recorder", session_dir, reader)
    assert not session.process_matches(42, "miris.recorder", tmp_path / "s2", reader)

    def unrelated_reader(pid):
        return "sleep 999"

    assert not session.process_matches(
        42, "miris.recorder", session_dir, unrelated_reader
    )


def test_process_matches_requires_exact_marker_token(tmp_path):
    session_dir = tmp_path / "s1"

    def reader(pid):
        return f"python -m miris.recorder-helper {session_dir}"

    assert not session.process_matches(42, "miris.recorder", session_dir, reader)


def test_load_rejects_unsupported_schema_version(tmp_path):
    session_dir = make_session(tmp_path)
    manifest_path = session_dir / "session.json"
    data = json.loads(manifest_path.read_text(encoding="utf-8"))
    data["schema_version"] = 2
    manifest_path.write_text(json.dumps(data), encoding="utf-8")
    with pytest.raises(ValueError, match="schema_version"):
        session.load(session_dir)


def test_write_rejects_unsupported_schema_version(tmp_path):
    session_dir = make_session(tmp_path)
    manifest = session.load(session_dir)
    invalid = session.SessionManifest(
        schema_version=2,
        session_id=manifest.session_id,
        state=manifest.state,
        created_at=manifest.created_at,
        updated_at=manifest.updated_at,
        recorder_pid=manifest.recorder_pid,
        processing_pid=manifest.processing_pid,
        channels=manifest.channels,
        error=manifest.error,
        note_path=manifest.note_path,
        transcript_ready=manifest.transcript_ready,
    )
    with pytest.raises(ValueError, match="schema_version"):
        session.write(session_dir, invalid)


def test_discovery_skips_non_mapping_channels(tmp_path):
    session_dir = tmp_path / "20260914-143000"
    session_dir.mkdir()
    (session_dir / "session.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "session_id": session_dir.name,
                "state": "recording",
                "created_at": FIXED_ISO,
                "updated_at": FIXED_ISO,
                "recorder_pid": 42,
                "processing_pid": None,
                "channels": [],
                "error": None,
                "note_path": None,
                "transcript_ready": False,
            }
        ),
        encoding="utf-8",
    )
    assert session.latest_active(tmp_path) is None


def test_reconcile_skips_non_mapping_channels(tmp_path):
    session_dir = tmp_path / "20260914-143000"
    session_dir.mkdir()
    (session_dir / "session.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "session_id": session_dir.name,
                "state": "recording",
                "created_at": FIXED_ISO,
                "updated_at": FIXED_ISO,
                "recorder_pid": 42,
                "processing_pid": None,
                "channels": [],
                "error": None,
                "note_path": None,
                "transcript_ready": False,
            }
        ),
        encoding="utf-8",
    )

    def unrelated_reader(pid):
        return "sleep 999"

    assert session.reconcile_stale(tmp_path, command_reader=unrelated_reader) == []


def test_reconcile_dead_recording_as_interrupted(tmp_path):
    session_dir = make_session(tmp_path)
    session.transition(session_dir, session.SessionState.RECORDING, recorder_pid=42)
    changed = session.reconcile_stale(tmp_path, command_reader=lambda pid: "")
    assert changed[0].state is session.SessionState.INTERRUPTED
    assert changed[0].error.code == "recorder_gone"


def test_reconcile_dead_processing_as_captured(tmp_path):
    session_dir = make_session(tmp_path)
    advance_to_captured(session_dir)
    session.transition(session_dir, session.SessionState.PROCESSING, processing_pid=43)
    changed = session.reconcile_stale(tmp_path, command_reader=lambda pid: "")
    assert changed[0].state is session.SessionState.CAPTURED
    assert changed[0].error.code == "processor_gone"


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


def damaged_riff_bytes(seconds, sample_rate, channels):
    buf = io.BytesIO()
    frames = int(seconds * sample_rate)
    audio = np.zeros(frames * channels, dtype=np.int16)
    with wave.open(buf, "wb") as wav:
        wav.setnchannels(channels)
        wav.setsampwidth(2)
        wav.setframerate(sample_rate)
        wav.writeframes(audio.tobytes())
    valid = buf.getvalue()
    assert valid[36:40] == b"data"  # stdlib wave layout: fmt then data at 36
    payload = valid[44:]
    broken = bytearray(valid[:44])
    struct.pack_into("<I", broken, 4, 40)  # stale RIFF size, header only
    struct.pack_into("<I", broken, 40, 0)  # data size never updated
    return bytes(broken) + payload


def test_repair_pcm_riff_lengths_recover_truncated_capture(tmp_path):
    path = tmp_path / "system.wav"
    path.write_bytes(damaged_riff_bytes(0.5, 48_000, 2))
    with pytest.raises(session.CaptureInvalid):
        session.inspect_wav(path)
    info = session.repair_wav(path)
    assert info.frames == int(0.5 * 48_000)
    assert info.sample_rate == 48_000
    assert info.channels == 2
    assert session.inspect_wav(path).frames == info.frames


def test_repair_pcm_riff_rejects_non_pcm_and_headerless(tmp_path):
    path = tmp_path / "system.wav"
    raw = damaged_riff_bytes(0.1, 48_000, 2)
    broken = bytearray(raw)
    struct.pack_into("<H", broken, 20, 3)  # fmt tag: IEEE float, not PCM
    path.write_bytes(bytes(broken))
    with pytest.raises(session.CaptureInvalid):
        session.repair_wav(path)

    empty = tmp_path / "empty.wav"
    empty.write_bytes(b"RIFF" + b"\0" * 60)
    with pytest.raises(session.CaptureInvalid):
        session.repair_wav(empty)


def test_legacy_note_session_becomes_completed(tmp_path):
    session_dir = make_wavs(tmp_path, mic_seconds=1, system_seconds=1)
    (session_dir / "session.json").unlink()
    note = tmp_path / "note.md"
    note.write_text("x", encoding="utf-8")
    (session_dir / "note.path").write_text(str(note), encoding="utf-8")
    assert (
        session.ensure_legacy_manifest(session_dir).state
        is session.SessionState.COMPLETED
    )


def test_legacy_directory_note_does_not_become_completed(tmp_path):
    session_dir = make_wavs(tmp_path, mic_seconds=1, system_seconds=1)
    (session_dir / "session.json").unlink()
    note_dir = tmp_path / "note-dir"
    note_dir.mkdir()
    (session_dir / "note.path").write_text(str(note_dir), encoding="utf-8")
    manifest = session.ensure_legacy_manifest(session_dir)
    assert manifest.state is session.SessionState.CAPTURED
    assert manifest.note_path is None
