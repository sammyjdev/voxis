import os
import subprocess
import wave
import pytest
from pathlib import Path

from miris.config import Config
from miris import cli, session


def _cfg(tmp_path) -> Config:
    return Config(vault_root=tmp_path / "vault", inbox="inbox", llm_command=["true"],
                  categories={}, sessions_dir=tmp_path / "sessions")


def test_live_session_none_when_empty(tmp_path):
    cfg = _cfg(tmp_path)
    cfg.sessions_dir.mkdir(parents=True)
    assert cli._live_session(cfg) == (None, None)


def test_live_session_finds_running_pid(tmp_path):
    cfg = _cfg(tmp_path)
    session = cfg.sessions_dir / "20260716-1000"
    session.mkdir(parents=True)
    (session / "recorder.pid").write_text(str(os.getpid()))
    assert cli._live_session(cfg) == (session, os.getpid())


def test_live_session_cleans_corrupt_pid(tmp_path):
    cfg = _cfg(tmp_path)
    session = cfg.sessions_dir / "20260716-1000"
    session.mkdir(parents=True)
    (session / "recorder.pid").write_text("")  # truncated write
    assert cli._live_session(cfg) == (None, None)
    assert not (session / "recorder.pid").exists()


def test_cmd_start_detaches_recorder_stdio(tmp_path, monkeypatch):
    """The recorder must not inherit the CLI's stdio: a caller that reads the
    CLI's pipes (the VOXIS tray uses Command::output) would block until the
    recorder exits - i.e. for the entire recording."""
    cfg = _cfg(tmp_path)
    seen = {}

    def fake_popen(args, **kwargs):
        seen.update(kwargs)
        session_dir = Path(args[-1])
        session.transition(session_dir, session.SessionState.RECORDING,
                           recorder_pid=12345)

    monkeypatch.setattr(cli.subprocess, "Popen", fake_popen)
    cli.cmd_start(cfg)
    assert seen.get("stdin") is subprocess.DEVNULL
    assert seen.get("stdout") is not None
    assert seen.get("stderr") is not None


def test_live_session_cleans_stale_pid(tmp_path):
    cfg = _cfg(tmp_path)
    session = cfg.sessions_dir / "20260716-1000"
    session.mkdir(parents=True)
    (session / "recorder.pid").write_text("999999999")
    assert cli._live_session(cfg) == (None, None)
    assert not (session / "recorder.pid").exists()


def test_cmd_start_waits_for_recording_manifest(tmp_path, monkeypatch):
    cfg = _cfg(tmp_path)

    def fake_popen(args, **kwargs):
        session_dir = Path(args[-1])
        session.transition(session_dir, session.SessionState.RECORDING,
                           recorder_pid=os.getpid())
        return object()

    monkeypatch.setattr(cli.subprocess, "Popen", fake_popen)
    cli.cmd_start(cfg)
    created = next(cfg.sessions_dir.iterdir())
    assert session.load(created).state is session.SessionState.RECORDING


def test_stop_refuses_reused_pid(tmp_path, monkeypatch):
    cfg = _cfg(tmp_path)
    session_dir = cfg.sessions_dir / "20260914-143000"
    session_dir.mkdir(parents=True)
    session.create(session_dir)
    session.transition(session_dir, session.SessionState.RECORDING, recorder_pid=42)
    kill_calls = []
    monkeypatch.setattr(cli.os, "kill", lambda *args: kill_calls.append(args))
    monkeypatch.setattr(session, "process_matches", lambda *args, **kwargs: False)
    with pytest.raises(SystemExit, match="does not belong"):
        cli.cmd_stop(cfg)
    assert kill_calls == []


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
    session.transition(session_dir, session.SessionState.RECORDING, recorder_pid=42)
    if state is not session.SessionState.RECORDING:
        session.transition(session_dir, session.SessionState.INTERRUPTED,
                           recorder_pid=None)
    if state is session.SessionState.CAPTURED:
        session.transition(session_dir, session.SessionState.CAPTURED)
    return cfg, session_dir


def test_recover_requires_interrupted_state(tmp_path):
    cfg, session_dir = session_in_state(tmp_path, session.SessionState.CAPTURED)
    with pytest.raises(SystemExit, match="not interrupted"):
        cli.cmd_recover(cfg, session_dir.name)


def test_recover_valid_partial_audio_stops_at_captured(tmp_path):
    cfg, session_dir = session_in_state(tmp_path, session.SessionState.INTERRUPTED)
    write_valid_wav(session_dir / "mic.wav", 16_000, 1)
    write_valid_wav(session_dir / "system.wav", 48_000, 2)
    cli.cmd_recover(cfg, session_dir.name)
    manifest = session.load(session_dir)
    assert manifest.state is session.SessionState.CAPTURED
    assert not (session_dir / "transcript.md").exists()


def test_process_requires_captured_and_calls_pipeline(tmp_path, monkeypatch):
    cfg, session_dir = session_in_state(tmp_path, session.SessionState.CAPTURED)
    called = []
    monkeypatch.setattr(cli.pipeline, "process_session",
                        lambda cfg, path: called.append(path) or path / "note.md")
    cli.cmd_process(cfg, session_dir.name)
    assert called == [session_dir]


def test_doctor_checks_both_capture_channels(tmp_path, monkeypatch, capsys):
    cfg = _cfg(tmp_path)
    cfg.vault_root.mkdir(parents=True)
    monkeypatch.setattr(
        cli.subprocess, "run",
        lambda *args, **kwargs: subprocess.CompletedProcess([], 0, stdout="OK"))
    monkeypatch.setattr(cli, "run_doctor_capture", lambda cfg, duration_s=3.0: {
        "mic": session.ChannelInfo("mic.wav", 48_000, 16_000, 1),
        "system": session.ChannelInfo("system.wav", 144_000, 48_000, 2),
    })
    assert cli.cmd_doctor(cfg, exit_on_finish=False) is True
    output = capsys.readouterr().out
    assert "[ok] microphone capture" in output
    assert "[ok] system-audio capture" in output


def test_doctor_reports_missing_channel_and_returns_false(tmp_path, monkeypatch,
                                                          capsys):
    cfg = _cfg(tmp_path)
    cfg.vault_root.mkdir(parents=True)
    monkeypatch.setattr(
        cli.subprocess, "run",
        lambda *args, **kwargs: subprocess.CompletedProcess([], 0, stdout="OK"))

    def missing_mic(cfg, duration_s=3.0):
        raise session.CaptureInvalid("missing capture: mic.wav")

    monkeypatch.setattr(cli, "run_doctor_capture", missing_mic)
    assert cli.cmd_doctor(cfg, exit_on_finish=False) is False
    output = capsys.readouterr().out
    assert "[FAIL]" in output
    assert "missing capture: mic.wav" in output
