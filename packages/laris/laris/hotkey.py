"""Global push-to-talk hotkey via pynput."""
from __future__ import annotations

import threading
import time
from typing import Any, Callable

_ALLOWED = {
    "cmd_r", "cmd_l",
    "shift_r", "shift_l",
    "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12", "f13", "f14", "f15",
}
_BROKEN_ON_MACOS = {"ctrl", "ctrl_l", "ctrl_r", "alt", "alt_l", "alt_r"}

# Wispr Flow's gesture: double-TAP the PTT key toggles hands-free.
# Only short taps count - dictating two sentences back-to-back is two
# HOLDS and must never arm it.
_TAP_MAX_SECONDS = 0.3
_DOUBLE_TAP_WINDOW_SECONDS = 0.4


class PushToTalk:
    """Single PTT key: hold = record, double-tap = hands-free toggle.
    While hands_free is set, a single press clears it and is consumed
    (no recording starts). Command vs dictation intent is decided by the
    utterance itself ("abrir <projeto>" etc.), not by a second key - a
    shift-based command key kept colliding with normal typing."""

    def __init__(
        self,
        key: str,
        now: Callable[[], float] = time.monotonic,
    ) -> None:
        self.key_name = key
        self._validate_key(key)

        try:
            from pynput import keyboard
        except ImportError as exc:
            raise SystemExit("[Hotkey] pynput not installed: pip install pynput") from exc

        self._keyboard = keyboard
        self._target = getattr(keyboard.Key, key, None)
        if self._target is None:
            raise ValueError("unknown hotkey")

        self.pressed = threading.Event()
        self.hands_free = threading.Event()
        self._now = now
        self._press_started = 0.0
        self._last_tap_at = 0.0
        self._listener: Any | None = None

    @staticmethod
    def _validate_key(key_name: str) -> None:
        if key_name in _BROKEN_ON_MACOS:
            raise ValueError(
                f"[Hotkey] '{key_name}' does not work as a global hotkey on macOS "
                f"(pynput #297). Use one of: {sorted(_ALLOWED)}"
            )
        if key_name not in _ALLOWED:
            raise ValueError(f"[Hotkey] unsupported key '{key_name}'. Use one of: {sorted(_ALLOWED)}")

    @staticmethod
    def check_trusted() -> bool:
        """True when this process can actually receive global keyboard events.

        Probes by creating a listen-only CGEventTap and checking it is non-None:
        on macOS 10.15+ the tap comes back None unless BOTH Accessibility and
        Input Monitoring are granted, so this is the accurate 'push-to-talk will
        work' signal. AXIsProcessTrusted reports Accessibility only and can be
        True while the tap silently fails - the exact trap that let PTT die with
        no error. Non-macOS (no Quartz): nothing to gate, assume True.
        """
        try:
            import Quartz
        except ImportError:
            return True

        tap = Quartz.CGEventTapCreate(
            Quartz.kCGSessionEventTap,
            Quartz.kCGHeadInsertEventTap,
            Quartz.kCGEventTapOptionListenOnly,
            Quartz.CGEventMaskBit(Quartz.kCGEventKeyDown),
            lambda *_a: None,
            None,
        )
        if tap is None:
            return False
        Quartz.CFMachPortInvalidate(tap)  # probe only - don't leak the tap
        return True

    def _on_press(self, key) -> None:
        if key != self._target:
            return
        if self.hands_free.is_set():
            self.hands_free.clear()  # consumed: stop hands-free, no PTT
            self._press_started = float("-inf")  # its release is not a tap
            return
        if self._now() - self._last_tap_at < _DOUBLE_TAP_WINDOW_SECONDS:
            self.hands_free.set()  # consumed: second tap arms hands-free
            self._last_tap_at = 0.0
            self._press_started = float("-inf")
            return
        self._press_started = self._now()
        self.pressed.set()

    def _on_release(self, key) -> None:
        if key != self._target:
            return
        self.pressed.clear()
        if self._now() - self._press_started < _TAP_MAX_SECONDS:
            self._last_tap_at = self._now()

    def start(self) -> None:
        if not self.check_trusted():
            print(
                "[Hotkey] WARNING: process lacks Accessibility permission; "
                "push-to-talk may fail silently."
            )
        self._listener = self._keyboard.Listener(
            on_press=self._on_press,
            on_release=self._on_release,
        )
        self._listener.daemon = True
        self._listener.start()
        print(
            f"[Hotkey] Push-to-talk active: key={self.key_name} "
            "(hold = record, double-tap = hands-free)"
        )

    def stop(self) -> None:
        if self._listener:
            self._listener.stop()
