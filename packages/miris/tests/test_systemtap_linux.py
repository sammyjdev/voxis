"""System audio on Linux: pw-record captures the default sink's monitor (what
the speakers play) as 16 kHz mono s16, behind the same tap interface the
recorder already drives on macOS."""
import sys
from pathlib import Path

from miris import systemtap


class _FakeProcess:
    def poll(self):
        return None


def test_on_linux_the_system_tap_records_the_default_sink_monitor_as_16khz_mono(
        tmp_path, monkeypatch):
    spawned = []
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setattr("subprocess.Popen",
                        lambda argv, **kwargs: spawned.append(argv) or _FakeProcess())
    out = tmp_path / "system.wav"

    systemtap.default_tap_factory(out).start()

    assert spawned == [[
        "pw-record", "-P", "{ stream.capture.sink=true }",
        "--rate", "16000", "--channels", "1", "--format", "s16", str(out),
    ]]


def test_on_macos_the_system_tap_is_still_the_audiotap_helper(tmp_path, monkeypatch):
    monkeypatch.setattr(sys, "platform", "darwin")
    monkeypatch.delenv("MIRIS_AUDIOTAP", raising=False)

    tap = systemtap.default_tap_factory(tmp_path / "system.wav")

    assert type(tap) is systemtap.SystemTap
    assert tap.binary == systemtap.DEFAULT_BIN
    assert tap.out_wav == Path(tmp_path / "system.wav")


def test_without_pw_record_a_linux_recording_fails_at_start_naming_it(tmp_path, monkeypatch):
    import threading

    from miris import recorder, session

    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setenv("PATH", str(tmp_path / "empty-bin"))
    session.create(tmp_path)

    state = recorder.record(tmp_path, threading.Event())

    error = session.load(tmp_path).error
    assert state is session.SessionState.FAILED
    assert error.code == "system_tap_start_failed"
    assert "pw-record not found" in error.message


FAKE_PW_RECORD = """#!/bin/sh
for out; do :; done   # the last argument is the output file
trap 'echo finalized >> "$out"; exit 1' TERM
echo started > "$out"
while true; do sleep 0.1; done
"""


def _fake_pw_record(tmp_path, monkeypatch, body=FAKE_PW_RECORD):
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    script = bin_dir / "pw-record"
    script.write_text(body)
    script.chmod(0o755)
    monkeypatch.setenv("PATH", f"{bin_dir}:/usr/bin:/bin")


def _wait_for(predicate):
    import time

    for _ in range(100):
        if predicate():
            return True
        time.sleep(0.05)
    return False


def test_stopping_the_linux_tap_lets_pw_record_finalize_the_wav(tmp_path, monkeypatch):
    """pw-record writes the real header sizes only on SIGTERM (it exits 1
    then); a kill would leave a placeholder header behind."""
    from miris.systemtap_linux import PwRecordTap

    _fake_pw_record(tmp_path, monkeypatch)
    out = tmp_path / "system.wav"
    tap = PwRecordTap(out)
    tap.start()
    assert _wait_for(out.exists)

    tap.stop()
    tap.stop()  # a second stop is a no-op

    assert not tap.alive()
    assert out.read_text().split() == ["started", "finalized"]


def test_a_pw_record_that_ignores_sigterm_is_killed_after_the_grace_period(tmp_path, monkeypatch):
    from miris.systemtap_linux import PwRecordTap

    _fake_pw_record(tmp_path, monkeypatch, body=FAKE_PW_RECORD.replace(
        "trap 'echo finalized >> \"$out\"; exit 1' TERM", "trap '' TERM"))
    out = tmp_path / "system.wav"
    tap = PwRecordTap(out)
    tap.start()
    assert _wait_for(out.exists)

    tap.stop(grace_s=0.2)

    assert not tap.alive()
    assert out.read_text().split() == ["started"]


def test_a_pw_record_that_exits_on_its_own_reads_as_dead(tmp_path, monkeypatch):
    """The recorder turns a dead tap into system_tap_died."""
    from miris.systemtap_linux import PwRecordTap

    _fake_pw_record(tmp_path, monkeypatch, body="#!/bin/sh\nexit 1\n")
    tap = PwRecordTap(tmp_path / "system.wav")
    tap.start()

    assert _wait_for(lambda: not tap.alive())


def test_a_killed_pw_record_wav_is_repaired_with_every_frame_it_wrote(tmp_path):
    """SIGKILL leaves pw-record's placeholder header (RIFF size 8, data size
    0) in front of the audio, as seen on 2026-10-15; recovery must rebuild it."""
    import struct

    from miris import session

    frames = 23525
    placeholder = (
        b"RIFF" + struct.pack("<I", 8) + b"WAVE"
        + b"fmt " + struct.pack("<IHHIIHH", 16, 1, 1, 16000, 32000, 2, 16)
        + b"data" + struct.pack("<I", 0)
    )
    wav = tmp_path / "system.wav"
    wav.write_bytes(placeholder + b"\x00\x00" * frames)

    info = session.repair_wav(wav)

    assert (info.frames, info.sample_rate, info.channels) == (frames, 16000, 1)
    assert session.inspect_wav(wav).frames == frames

