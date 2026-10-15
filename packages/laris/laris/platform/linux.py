"""Linux backend (GNOME Wayland): faster-whisper for STT, clipboard plus a
uinput Shift+Insert for output, and the orb as an X11 popup under XWayland
(overlay_linux.py). No ducking yet: a no-op the daemon can call every
utterance."""
from __future__ import annotations

import threading
import time
from pathlib import Path
from typing import Callable

import numpy as np


_SYSTEM_PYTHON = "/usr/bin/python3"


class _NoDucker:
    def duck(self) -> None:
        pass

    def restore(self) -> None:
        pass


class ToggleTrigger:
    """Stands in for PushToTalk where no global key events exist. Nothing
    sets `pressed`; `hands_free` is flipped by toggle(), which the server's
    /toggle route calls when a desktop shortcut runs `laris toggle`."""

    label = "toggle: laris toggle"
    stop_hint = "run laris toggle again to stop"

    # GNOME fires a custom shortcut again on key autorepeat (by default once
    # on press, then every 30 ms after 500 ms), one `laris toggle` run each.
    # A request this close to the previous one, taken or not, is the same key
    # still held; one held 1.2 s flipped hands-free 25 times (2026-10-14).
    # With no key-release signal, two taps closer than this count as one.
    HOLD_WINDOW_SECONDS = 1.0

    def __init__(self, now: Callable[[], float] = time.monotonic) -> None:
        self.pressed = threading.Event()
        self.hands_free = threading.Event()
        self._now = now
        self._lock = threading.Lock()
        self._last_request: float | None = None

    def toggle(self) -> bool:
        """Flips hands-free unless this is a key repeat; returns the new state."""
        with self._lock:
            now = self._now()
            repeat = (self._last_request is not None
                      and now - self._last_request < self.HOLD_WINDOW_SECONDS)
            self._last_request = now
            if not repeat:
                if self.hands_free.is_set():
                    self.hands_free.clear()
                else:
                    self.hands_free.set()
            return self.hands_free.is_set()

    def start(self) -> None:
        pass


class LinuxBackend:
    def __init__(self) -> None:
        from laris.stt_faster import FasterWhisperTranscriber
        self._stt = FasterWhisperTranscriber()

    def warm(self) -> None:
        self._stt.warm()

    def transcribe(self, audio: np.ndarray, sample_rate: int) -> str:
        return self._stt.transcribe(audio, sample_rate)

    def type_text(self, text: str) -> None:
        from laris.dictation_linux import type_text
        type_text(text)

    def make_overlay(self, url: str):
        from laris.overlay import Overlay
        # PyGObject (GTK + WebKitGTK) does not build in the venv, the system
        # Python ships it; the child imports only stdlib-backed laris modules
        package_root = str(Path(__file__).resolve().parent.parent.parent)
        return Overlay(
            url=url,
            argv=[_SYSTEM_PYTHON, "-m", "laris.overlay_linux"],
            env={"GDK_BACKEND": "x11", "PYTHONPATH": package_root},
        )

    def make_ducker(self):
        return _NoDucker()
