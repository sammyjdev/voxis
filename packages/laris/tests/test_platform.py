"""Platform backend selection and the macOS backend's delegation."""
import sys
from pathlib import Path

import numpy as np
import pytest

from laris import platform as laris_platform
from laris.platform.macos import MacOSBackend


def test_get_backend_darwin_returns_macos(monkeypatch):
    # mock Transcriber so constructing MacOSBackend does not load real
    # mlx_whisper - importing mlx after torch in the same process aborts the
    # interpreter (a fatal native crash), and no other test loads real mlx
    monkeypatch.setattr("laris.transcriber.Transcriber", lambda: object())
    monkeypatch.setattr(sys, "platform", "darwin")
    backend = laris_platform.get_backend()
    assert isinstance(backend, MacOSBackend)


def test_get_backend_linux_returns_linux_backend(monkeypatch):
    monkeypatch.setattr("laris.stt_faster.FasterWhisperTranscriber", lambda: object())
    monkeypatch.setattr(sys, "platform", "linux")
    from laris.platform.linux import LinuxBackend

    assert isinstance(laris_platform.get_backend(), LinuxBackend)


def test_linux_backend_transcribes_with_faster_whisper_and_pastes_with_the_linux_path(monkeypatch):
    calls = {}

    class FakeStt:
        def warm(self):
            calls["warm"] = True

        def transcribe(self, audio, sample_rate):
            calls["transcribe"] = (len(audio), sample_rate)
            return "texto"

    monkeypatch.setattr("laris.stt_faster.FasterWhisperTranscriber", lambda: FakeStt())
    monkeypatch.setattr("laris.dictation_linux.type_text",
                        lambda text: calls.__setitem__("typed", text))
    from laris.platform.linux import LinuxBackend

    backend = LinuxBackend()
    backend.warm()
    assert backend.transcribe(np.zeros(8, dtype=np.float32), 16000) == "texto"
    backend.type_text("oi")
    assert calls == {"warm": True, "transcribe": (8, 16000), "typed": "oi"}


def test_on_linux_the_backend_ducks_the_default_output_through_wpctl(monkeypatch):
    """The daemon ducks once per hands-free session, through the backend."""
    import subprocess

    from laris import ducking_linux
    from laris.platform.linux import LinuxBackend

    calls = []

    def fake_wpctl(argv, **kwargs):
        calls.append(argv)
        out = {"inspect": "id 60, type PipeWire:Interface:Node\n",
               "get-volume": "Volume: 0.40\n"}.get(argv[1], "")
        return subprocess.CompletedProcess(argv, 0, stdout=out.encode())

    monkeypatch.setattr(ducking_linux.subprocess, "run", fake_wpctl)
    backend = LinuxBackend.__new__(LinuxBackend)  # no transcriber construction

    backend.make_ducker().duck()

    assert ["wpctl", "set-volume", "60", "0.20"] in calls


def test_on_linux_the_orb_runs_in_the_system_python_on_xwayland(monkeypatch):
    """GNOME Wayland gives a native client no always-on-top window, but an
    X11 popup under XWayland stays on top and never takes focus. PyGObject
    cannot build in the venv, so the child runs on the system Python with
    the laris package on its path."""
    from unittest.mock import MagicMock

    import laris
    from laris import overlay
    from laris.platform.linux import LinuxBackend

    spawned = []
    monkeypatch.setattr(overlay.subprocess, "Popen",
                        lambda argv, **kw: spawned.append((argv, kw)) or MagicMock(poll=lambda: None))
    backend = LinuxBackend.__new__(LinuxBackend)  # no transcriber construction

    backend.make_overlay("http://127.0.0.1:8421/overlay?token=t").show("dictate")

    argv, kw = spawned[0]
    package_root = str(Path(laris.__file__).resolve().parent.parent)
    assert argv == ["/usr/bin/python3", "-m", "laris.overlay_linux"]
    assert kw["env"]["GDK_BACKEND"] == "x11"
    assert kw["env"]["OVERLAY_URL"] == "http://127.0.0.1:8421/overlay?token=t"
    assert kw["env"]["PYTHONPATH"] == package_root


def test_get_backend_unsupported_exits(monkeypatch):
    monkeypatch.setattr(sys, "platform", "sunos")
    with pytest.raises(SystemExit):
        laris_platform.get_backend()


def test_macos_backend_delegates(monkeypatch):
    calls = {}

    class FakeStt:
        def warm(self):
            calls["warm"] = True

        def transcribe(self, audio, sample_rate):
            calls["transcribe"] = (len(audio), sample_rate)
            return "hello"

    monkeypatch.setattr("laris.transcriber.Transcriber", lambda: FakeStt())
    monkeypatch.setattr("laris.dictation.type_text",
                        lambda text: calls.__setitem__("typed", text))

    backend = MacOSBackend()
    backend.warm()
    assert backend.transcribe(np.zeros(10, dtype=np.float32), 16000) == "hello"
    backend.type_text("hi there")
    assert calls == {"warm": True, "transcribe": (10, 16000), "typed": "hi there"}


def test_macos_backend_overlay_and_ducker_are_real_types():
    from laris.overlay import Overlay
    from laris.ducking import AudioDucker

    backend = MacOSBackend.__new__(MacOSBackend)  # no Transcriber construction
    assert isinstance(backend.make_overlay("http://x"), Overlay)
    assert isinstance(backend.make_ducker(), AudioDucker)
