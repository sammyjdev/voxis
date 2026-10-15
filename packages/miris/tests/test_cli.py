import io
import os
import struct
import subprocess
import sys
import wave

import numpy as np
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


def test_recover_repairs_truncated_system_wav(tmp_path):
    cfg, session_dir = session_in_state(tmp_path, session.SessionState.INTERRUPTED)
    write_valid_wav(session_dir / "mic.wav", 16_000, 1)
    buf = io.BytesIO()
    frames = int(0.5 * 48_000)
    audio = np.zeros(frames * 2, dtype=np.int16)
    with wave.open(buf, "wb") as wav:
        wav.setnchannels(2)
        wav.setsampwidth(2)
        wav.setframerate(48_000)
        wav.writeframes(audio.tobytes())
    valid = buf.getvalue()
    broken = bytearray(valid[:44])
    struct.pack_into("<I", broken, 4, 40)  # stale RIFF size
    struct.pack_into("<I", broken, 40, 0)  # data size never updated
    (session_dir / "system.wav").write_bytes(bytes(broken) + valid[44:])

    cli.cmd_recover(cfg, session_dir.name)

    manifest = session.load(session_dir)
    assert manifest.state is session.SessionState.CAPTURED
    assert manifest.channels["system"].frames == frames


def test_process_requires_captured(tmp_path):
    cfg, session_dir = session_in_state(tmp_path, session.SessionState.INTERRUPTED)
    with pytest.raises(SystemExit, match="not captured"):
        cli.cmd_process(cfg, session_dir.name)


def test_process_runs_pipeline_as_verifiable_subprocess(tmp_path, monkeypatch):
    """A pipeline running inside the CLI process is indistinguishable from a
    dead processor: `ps` shows no miris.pipeline token next to the session
    path, so reconcile_stale and the tray derive captured (and Python's own
    correction mid-run breaks the final transition). It must be a process of
    its own whose command carries the marker."""
    cfg, session_dir = session_in_state(tmp_path, session.SessionState.CAPTURED)
    seen = {}

    def fake_run(args, **kwargs):
        seen["args"] = args
        return subprocess.CompletedProcess(args, 0, stdout="note: /v/n.md\n")

    monkeypatch.setattr(cli.subprocess, "run", fake_run)
    cli.cmd_process(cfg, session_dir.name)
    assert seen["args"] == [sys.executable, "-m", "miris.pipeline", str(session_dir)]


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


def _doctor_with_llm_ok(tmp_path, monkeypatch):
    cfg = _cfg(tmp_path)
    cfg.vault_root.mkdir(parents=True)
    monkeypatch.setattr(
        cli.subprocess, "run",
        lambda *args, **kwargs: subprocess.CompletedProcess([], 0, stdout="OK"))
    return cfg


def test_on_linux_the_doctor_checks_pw_record_and_runs_the_capture(tmp_path, monkeypatch, capsys):
    cfg = _doctor_with_llm_ok(tmp_path, monkeypatch)
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    (bin_dir / "pw-record").write_text("#!/bin/sh\n")
    (bin_dir / "pw-record").chmod(0o755)
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setenv("PATH", str(bin_dir))
    monkeypatch.setattr(cli, "run_doctor_capture", lambda cfg, duration_s=3.0: {
        "mic": session.ChannelInfo("mic.wav", 48_000, 16_000, 1),
        "system": session.ChannelInfo("system.wav", 48_000, 16_000, 1),
    })

    assert cli.cmd_doctor(cfg, exit_on_finish=False) is True
    output = capsys.readouterr().out

    assert f"[ok] pw-record: {bin_dir / 'pw-record'}" in output
    assert "audiotap" not in output
    assert "[ok] system-audio capture: 48000 frames (3.0s @ 16000 Hz)" in output


def test_on_linux_without_pw_record_the_doctor_fails_and_skips_the_capture(
        tmp_path, monkeypatch, capsys):
    cfg = _doctor_with_llm_ok(tmp_path, monkeypatch)
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setenv("PATH", str(tmp_path / "empty-bin"))

    def capture_must_not_run(cfg, duration_s=3.0):
        raise AssertionError("capture ran without pw-record")

    monkeypatch.setattr(cli, "run_doctor_capture", capture_must_not_run)

    assert cli.cmd_doctor(cfg, exit_on_finish=False) is False
    assert "[FAIL] pw-record: not found" in capsys.readouterr().out


def test_on_macos_the_doctor_still_checks_the_audiotap_binary(tmp_path, monkeypatch, capsys):
    cfg = _doctor_with_llm_ok(tmp_path, monkeypatch)
    helper = tmp_path / "audiotap"
    helper.write_text("")
    monkeypatch.setattr(sys, "platform", "darwin")
    monkeypatch.setenv("MIRIS_AUDIOTAP", str(helper))
    monkeypatch.setattr(cli, "run_doctor_capture", lambda cfg, duration_s=3.0: {
        "mic": session.ChannelInfo("mic.wav", 48_000, 16_000, 1),
        "system": session.ChannelInfo("system.wav", 144_000, 48_000, 2),
    })

    cli.cmd_doctor(cfg, exit_on_finish=False)

    assert f"[ok] audiotap binary: {helper}" in capsys.readouterr().out


def _doctor_output_for_llm_command(tmp_path, monkeypatch, capsys, llm_command):
    import dataclasses

    cfg = dataclasses.replace(_doctor_with_llm_ok(tmp_path, monkeypatch), llm_command=llm_command)
    monkeypatch.setattr(cli, "run_doctor_capture", lambda cfg, duration_s=3.0: {
        "mic": session.ChannelInfo("mic.wav", 48_000, 16_000, 1),
        "system": session.ChannelInfo("system.wav", 48_000, 16_000, 1),
    })
    cli.cmd_doctor(cfg, exit_on_finish=False)
    return capsys.readouterr().out


def test_the_doctor_warns_that_a_bare_llm_command_is_not_found_from_the_tray(tmp_path, monkeypatch, capsys):
    """The tray started at login has no shell PATH: a bare `agy` passed the
    doctor in a terminal, then every meeting the tray stopped failed with
    "No such file or directory: 'agy'" (2026-10-15)."""
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    (bin_dir / "agy").write_text("#!/bin/sh\n")
    (bin_dir / "agy").chmod(0o755)
    monkeypatch.setenv("PATH", str(bin_dir))

    out = _doctor_output_for_llm_command(tmp_path, monkeypatch, capsys, ["agy", "-p"])

    assert (f"[WARN] llm_command: the tray started from the desktop does not have your "
            f"shell PATH - write \"{bin_dir / 'agy'}\" instead of \"agy\"") in out


def test_the_doctor_does_not_warn_about_an_llm_command_given_by_full_path(tmp_path, monkeypatch, capsys):
    out = _doctor_output_for_llm_command(tmp_path, monkeypatch, capsys, ["/usr/bin/true"])

    assert "[WARN] llm_command" not in out


def test_the_doctor_reports_an_empty_llm_command_instead_of_crashing(tmp_path, monkeypatch, capsys):
    out = _doctor_output_for_llm_command(tmp_path, monkeypatch, capsys, [])

    assert "[FAIL] llm_command: " in out
