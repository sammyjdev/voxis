import sys
from types import SimpleNamespace

import pytest


class _FakeListener:
    def __init__(self, **kwargs):
        self.kwargs = kwargs
        self.daemon = False

    def start(self):
        pass

    def stop(self):
        pass


def _install_fake_pynput(monkeypatch):
    key = SimpleNamespace(cmd_r=object(), shift_r=object(), f5=object())
    keyboard = SimpleNamespace(Key=key, Listener=_FakeListener)
    monkeypatch.setitem(sys.modules, "pynput", SimpleNamespace(keyboard=keyboard))


@pytest.mark.parametrize("key_name", ["alt_r", "ctrl"])
def test_push_to_talk_rejects_broken_macos_keys(key_name):
    from laris.hotkey import PushToTalk

    with pytest.raises(ValueError):
        PushToTalk(key_name)


@pytest.mark.parametrize("key_name", ["cmd_r", "shift_r", "f5"])
def test_push_to_talk_constructs_for_allowed_keys(monkeypatch, key_name):
    _install_fake_pynput(monkeypatch)
    from laris.hotkey import PushToTalk

    ptt = PushToTalk(key_name)

    assert ptt.pressed.is_set() is False
    assert ptt.hands_free.is_set() is False


class _Clock:
    def __init__(self):
        self.t = 100.0

    def __call__(self):
        return self.t


def _make_ptt(monkeypatch):
    _install_fake_pynput(monkeypatch)
    from laris.hotkey import PushToTalk

    clock = _Clock()
    ptt = PushToTalk("cmd_r", now=clock)
    return ptt, ptt._target, clock


def test_hold_and_release_drives_pressed_event(monkeypatch):
    ptt, key, clock = _make_ptt(monkeypatch)

    ptt._on_press(key)
    assert ptt.pressed.is_set()

    clock.t += 1.0
    ptt._on_release(key)
    assert not ptt.pressed.is_set()


def test_double_tap_arms_hands_free_without_ptt(monkeypatch):
    ptt, key, clock = _make_ptt(monkeypatch)

    ptt._on_press(key)
    clock.t += 0.1
    ptt._on_release(key)  # tap 1 (short)
    clock.t += 0.2
    ptt._on_press(key)  # tap 2 inside the window

    assert ptt.hands_free.is_set()
    assert not ptt.pressed.is_set()


def test_press_while_hands_free_disarms_without_ptt(monkeypatch):
    ptt, key, clock = _make_ptt(monkeypatch)
    ptt.hands_free.set()

    ptt._on_press(key)

    assert not ptt.hands_free.is_set()
    assert not ptt.pressed.is_set()


def test_two_quick_holds_do_not_arm_hands_free(monkeypatch):
    """Dictating two sentences back-to-back is two HOLDS - only short taps
    count toward the double-tap gesture."""
    ptt, key, clock = _make_ptt(monkeypatch)

    ptt._on_press(key)
    clock.t += 1.5
    ptt._on_release(key)  # hold, not a tap
    clock.t += 0.2
    ptt._on_press(key)  # quick second press

    assert not ptt.hands_free.is_set()
    assert ptt.pressed.is_set()  # normal PTT proceeds


def test_other_keys_are_ignored(monkeypatch):
    ptt, _, clock = _make_ptt(monkeypatch)

    ptt._on_press("a")
    ptt._on_release("a")

    assert not ptt.pressed.is_set()
    assert not ptt.hands_free.is_set()


def _fake_quartz(tap_result):
    """Stand-in for the Quartz module: CGEventTapCreate returns tap_result,
    and CFMachPortInvalidate records what the probe releases."""
    invalidated = []
    q = SimpleNamespace(
        kCGSessionEventTap=0,
        kCGHeadInsertEventTap=0,
        kCGEventTapOptionListenOnly=1,
        kCGEventKeyDown=10,
        CGEventMaskBit=lambda e: 1 << e,
        CGEventTapCreate=lambda *a: tap_result,
        CFMachPortInvalidate=invalidated.append,
    )
    return q, invalidated


def test_check_trusted_false_when_event_tap_denied(monkeypatch):
    """A listen-only tap comes back None without Input Monitoring (even if
    Accessibility is granted) - that is the accurate 'PTT will not work'."""
    from laris.hotkey import PushToTalk

    q, _ = _fake_quartz(None)
    monkeypatch.setitem(sys.modules, "Quartz", q)
    assert PushToTalk.check_trusted() is False


def test_check_trusted_true_and_releases_tap_when_granted(monkeypatch):
    from laris.hotkey import PushToTalk

    sentinel = object()
    q, invalidated = _fake_quartz(sentinel)
    monkeypatch.setitem(sys.modules, "Quartz", q)

    assert PushToTalk.check_trusted() is True
    assert invalidated == [sentinel]  # the probe must not leak the tap


def test_check_trusted_true_without_quartz(monkeypatch):
    """Non-macOS (no Quartz): nothing to gate, never block."""
    from laris.hotkey import PushToTalk

    monkeypatch.setitem(sys.modules, "Quartz", None)  # import Quartz -> ImportError
    assert PushToTalk.check_trusted() is True
