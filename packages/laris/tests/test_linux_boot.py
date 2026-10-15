"""Booting the daemon on Linux: no key listener exists there (pynput sees no
global keys on Wayland, Quartz is macOS), so the loop must run off the
toggle trigger, and the server's /toggle must reach that same trigger."""
import sys
from unittest.mock import MagicMock

import pytest
from fastapi.testclient import TestClient

from laris import peer_discovery


class _FakeBackend:
    def warm(self):
        pass

    def transcribe(self, audio, sample_rate):
        return ""

    def type_text(self, text):
        pass

    def make_overlay(self, url):
        return MagicMock()

    def make_ducker(self):
        return MagicMock()


class _StopLoop(Exception):
    pass


@pytest.fixture
def no_macos_input_stack(monkeypatch):
    """Any import of these now raises ImportError, as on a Linux venv."""
    for name in ("pynput", "Quartz", "AppKit", "mlx_whisper"):
        monkeypatch.setitem(sys.modules, name, None)


def test_a_daemon_driven_by_the_toggle_trigger_reaches_ready_without_a_key_listener(
        monkeypatch, no_macos_input_stack):
    from laris import daemon
    from laris.platform.linux import ToggleTrigger

    trigger = ToggleTrigger()
    waited_on = {}
    events = []

    def _wait(pressed, hands_free=None, on_tick=None):
        waited_on.update(pressed=pressed, hands_free=hands_free)
        raise _StopLoop

    monkeypatch.setattr("laris.platform.get_backend", lambda: _FakeBackend())
    monkeypatch.setattr(daemon, "_wait_for_ptt", _wait)

    with pytest.raises(_StopLoop):
        daemon.run(
            make_voice_session=lambda: MagicMock(),
            on_ready=lambda: events.append("ready"),
            trigger=trigger,
        )

    assert events == ["ready"]
    assert waited_on == {"pressed": trigger.pressed, "hands_free": trigger.hands_free}


def test_on_linux_a_toggle_reaches_the_event_the_daemon_loop_waits_on(
        monkeypatch, tmp_path, no_macos_input_stack):
    import laris.__main__ as entry

    captured = {}
    real_build_app = entry.build_app

    def _capture_app(*args, **kwargs):
        captured["app"] = real_build_app(*args, **kwargs)
        return captured["app"]

    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setattr(peer_discovery, "_peers_path_override", tmp_path / "peers.json")
    monkeypatch.setattr(entry, "_daemon_already_running", lambda: None)
    monkeypatch.setattr(entry, "build_app", _capture_app)
    monkeypatch.setattr(entry.uvicorn, "run", lambda *a, **k: None)
    monkeypatch.setattr(entry, "_wait_for_port", lambda *a, **k: True)
    monkeypatch.setattr("laris.daemon.run", lambda **kwargs: captured.update(kwargs))

    entry.main([])
    token = peer_discovery.peer_token("laris")
    response = TestClient(captured["app"]).post(f"/toggle?token={token}")

    assert response.status_code == 200
    assert captured["trigger"].hands_free.is_set()


def test_on_linux_the_boot_log_names_the_faster_whisper_model_and_the_toggle(
        monkeypatch, tmp_path, capsys, no_macos_input_stack):
    import laris.__main__ as entry
    from laris import config

    monkeypatch.setattr(config, "STT_MODEL_FASTER", "large-v3-turbo")
    monkeypatch.setattr(config, "DICTATE_KEY", "cmd_r")
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setattr(peer_discovery, "_peers_path_override", tmp_path / "peers.json")
    monkeypatch.setattr(entry, "_daemon_already_running", lambda: None)
    monkeypatch.setattr(entry.uvicorn, "run", lambda *a, **k: None)
    monkeypatch.setattr(entry, "_wait_for_port", lambda *a, **k: True)
    monkeypatch.setattr("laris.daemon.run", lambda **kwargs: None)

    entry.main([])
    out = capsys.readouterr().out

    assert "STT: large-v3-turbo " in out
    assert "laris toggle" in out
    assert "mlx-community" not in out
    assert "cmd_r" not in out


def test_a_toggled_hands_free_session_tells_how_to_stop_it_with_laris_toggle(
        monkeypatch, capsys, no_macos_input_stack):
    from laris import config, daemon
    from laris.platform.linux import ToggleTrigger

    class _SilentVad:
        def stream_utterances(self, should_stop, on_chunk=None):
            return iter([])

    trigger = ToggleTrigger()
    trigger.hands_free.set()
    waits = iter([("hands_free", trigger.hands_free)])

    def _wait(pressed, hands_free=None, on_tick=None):
        try:
            return next(waits)
        except StopIteration:
            raise _StopLoop

    monkeypatch.setattr(config, "DICTATE_KEY", "cmd_r")
    monkeypatch.setattr("laris.platform.get_backend", lambda: _FakeBackend())
    monkeypatch.setattr("laris.audio_capture.VadCapture", _SilentVad)
    monkeypatch.setattr(daemon, "_wait_for_ptt", _wait)

    with pytest.raises(_StopLoop):
        daemon.run(make_voice_session=lambda: MagicMock(), trigger=trigger)
    out = capsys.readouterr().out

    assert "hands-free ON (run laris toggle again to stop)" in out
    assert "dictate key" not in out
    assert "cmd_r" not in out
