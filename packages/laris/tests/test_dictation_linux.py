"""Linux dictation output: the text goes on the Wayland clipboard (and the
PRIMARY selection, which some terminals paste on Shift+Insert), one
Shift+Insert is typed through uinput, and the user's clipboard comes back."""
import subprocess

import pytest
from evdev import ecodes as e

from laris import dictation_linux


class FakeKeyboard:
    def __init__(self, fail=False, fail_on=None):
        self.keys = []
        self.attempts = []
        self.fail = fail
        self.fail_on = fail_on

    def write(self, etype, code, value):
        self.attempts.append((code, value))
        if self.fail or (code, value) == self.fail_on:
            raise OSError("uinput gone")
        self.keys.append((code, value))

    def syn(self):
        pass


@pytest.fixture
def desktop(monkeypatch):
    """A fake Wayland clipboard holding "antigo", a PRIMARY selection holding
    "selecionado", and a fake uinput keyboard."""
    state = {"clipboard": b"antigo", "primary": b"selecionado", "copies": [],
             "copy_rc": 0, "primary_rc": 0}
    keyboard = FakeKeyboard()

    def fake_run(cmd, input=None, capture_output=False, timeout=None):
        target = "primary" if "--primary" in cmd else "clipboard"
        if cmd[0] == "wl-paste":
            return subprocess.CompletedProcess(cmd, 0, stdout=state[target])
        if cmd[0] == "wl-copy":
            state["copies"].append((target, input))
            rc = state["copy_rc"] or (state["primary_rc"] if target == "primary" else 0)
            if rc == 0:
                state[target] = input
            return subprocess.CompletedProcess(cmd, rc)
        raise AssertionError(f"unexpected command {cmd}")

    monkeypatch.setattr(dictation_linux.subprocess, "run", fake_run)
    monkeypatch.setattr(dictation_linux.time, "sleep", lambda s: None)
    monkeypatch.setattr(dictation_linux, "_keyboard", lambda: keyboard)
    return state, keyboard


SHIFT_INSERT = [
    (e.KEY_LEFTSHIFT, 1), (e.KEY_INSERT, 1), (e.KEY_INSERT, 0), (e.KEY_LEFTSHIFT, 0),
]


def test_dictated_text_is_pasted_with_shift_insert_and_the_old_clipboard_comes_back(desktop):
    state, keyboard = desktop

    dictation_linux.type_text("ação, olá")

    assert state["copies"][:2] == [
        ("clipboard", "ação, olá".encode()),
        ("primary", "ação, olá".encode()),
    ]
    assert keyboard.keys == SHIFT_INSERT
    assert state["clipboard"] == b"antigo"


def test_when_the_clipboard_cannot_be_set_no_key_is_typed(desktop):
    state, keyboard = desktop
    state["copy_rc"] = 1

    dictation_linux.type_text("perdido")

    assert keyboard.keys == []


def test_when_the_keystroke_fails_the_text_stays_on_the_clipboard_for_a_manual_paste(desktop, monkeypatch):
    state, _keyboard = desktop
    monkeypatch.setattr(dictation_linux, "_keyboard", lambda: FakeKeyboard(fail=True))

    dictation_linux.type_text("cole você")

    assert state["clipboard"] == "cole você".encode()


def test_the_users_primary_selection_comes_back_after_the_paste(desktop):
    state, _keyboard = desktop

    dictation_linux.type_text("ditado")

    assert state["primary"] == b"selecionado"


def test_when_the_primary_selection_cannot_be_set_the_old_clipboard_comes_back(desktop):
    state, keyboard = desktop
    state["primary_rc"] = 1

    dictation_linux.type_text("ditado")

    assert keyboard.keys == []
    assert state["clipboard"] == b"antigo"


def test_a_keystroke_that_fails_midway_still_releases_shift(desktop, monkeypatch):
    """A Shift left held by the virtual keyboard capitalizes everything the
    user types next."""
    state, _keyboard = desktop
    keyboard = FakeKeyboard(fail_on=(e.KEY_INSERT, 1))
    monkeypatch.setattr(dictation_linux, "_keyboard", lambda: keyboard)

    dictation_linux.type_text("ditado")

    assert keyboard.keys == [(e.KEY_LEFTSHIFT, 1), (e.KEY_INSERT, 0), (e.KEY_LEFTSHIFT, 0)]


def test_a_failed_insert_release_still_releases_shift(desktop, monkeypatch):
    state, _keyboard = desktop
    keyboard = FakeKeyboard(fail_on=(e.KEY_INSERT, 0))
    monkeypatch.setattr(dictation_linux, "_keyboard", lambda: keyboard)

    dictation_linux.type_text("ditado")

    assert keyboard.attempts[-2:] == [(e.KEY_INSERT, 0), (e.KEY_LEFTSHIFT, 0)]
    assert keyboard.keys[-1] == (e.KEY_LEFTSHIFT, 0)
