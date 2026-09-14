import time
import subprocess
from pathlib import Path

import pytest

from miris.systemtap import SystemTap

FAKE_HELPER = """#!/bin/sh
trap 'echo done >> "$1"; exit 0' TERM INT
echo started > "$1"
while true; do sleep 0.1; done
"""


class FakeProcess:
    def __init__(self, wait_raises_timeout=False):
        self.wait_raises_timeout = wait_raises_timeout
        self.killed = False
        self.waited_after_kill = False

    def poll(self):
        return None

    def send_signal(self, signal):
        pass

    def kill(self):
        self.killed = True

    def wait(self, timeout=None):
        if self.wait_raises_timeout and not self.killed:
            raise subprocess.TimeoutExpired("helper", 0.01)
        if self.killed:
            self.waited_after_kill = True


def _fake_binary(tmp_path) -> Path:
    binary = tmp_path / "fake_audiotap"
    binary.write_text(FAKE_HELPER)
    binary.chmod(0o755)
    return binary


def test_start_raises_when_binary_missing(tmp_path):
    tap = SystemTap(tmp_path / "out.wav", binary=tmp_path / "nope")
    with pytest.raises(FileNotFoundError):
        tap.start()


def test_lifecycle_start_alive_stop(tmp_path):
    out = tmp_path / "out.wav"
    tap = SystemTap(out, binary=_fake_binary(tmp_path))
    tap.start()
    # the fake helper writes the file only AFTER installing its TERM trap;
    # polling for it (instead of a fixed sleep) kills the SIGTERM race
    for _ in range(100):
        if out.exists():
            break
        time.sleep(0.05)
    assert out.exists()
    assert tap.alive()
    tap.stop()
    assert not tap.alive()
    assert "done" in out.read_text()


def test_bytes_written_reports_owned_output(tmp_path):
    out = tmp_path / "out.wav"
    tap = SystemTap(out, binary=_fake_binary(tmp_path))
    assert tap.bytes_written() == 0
    out.write_bytes(b"RIFF" + b"x" * 100)
    assert tap.bytes_written() == 104


def test_stop_kills_helper_after_grace(tmp_path):
    proc = FakeProcess(wait_raises_timeout=True)
    tap = SystemTap(tmp_path / "out.wav", binary=tmp_path / "helper")
    tap.proc = proc
    tap.stop(grace_s=0.01)
    assert proc.killed and proc.waited_after_kill
