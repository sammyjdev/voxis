"""Dictation output into the focused app on Linux (GNOME Wayland).

Same shape as dictation.py on macOS: clipboard, one paste keystroke, then a
best-effort restore of the user's clipboard. Wayland lets no client inject
keys, so the keystroke goes through a uinput virtual keyboard (the user needs
rw on /dev/uinput). Shift+Insert, not Ctrl+V: it pastes in terminals, where
Ctrl+V does not, and in GTK, browsers and Electron editors alike."""
import subprocess
import time

# wl-copy / wl-paste must never freeze the single-threaded voice loop
_SUBPROCESS_TIMEOUT_SECONDS = 4
_PASTE_SETTLE_SECONDS = 0.3
# a uinput device is invisible to the compositor for a moment after creation;
# keys sent before then are dropped (seen in the 2026-10-14 spike)
_DEVICE_SETTLE_SECONDS = 0.5

_device = None


def _keyboard():
    """One virtual keyboard for the daemon's lifetime, created on first paste."""
    global _device
    if _device is None:
        from evdev import UInput, ecodes as e
        _device = UInput({e.EV_KEY: [e.KEY_LEFTSHIFT, e.KEY_INSERT]}, name="laris-paste")
        time.sleep(_DEVICE_SETTLE_SECONDS)
    return _device


def _paste_via_keyboard() -> None:
    from evdev import ecodes as e

    kbd = _keyboard()
    try:
        for code in (e.KEY_LEFTSHIFT, e.KEY_INSERT):
            kbd.write(e.EV_KEY, code, 1)
            kbd.syn()
    finally:
        # release each key even after a failed press or release: a Shift left
        # held by the virtual keyboard capitalizes everything the user types next
        for code in (e.KEY_INSERT, e.KEY_LEFTSHIFT):
            try:
                kbd.write(e.EV_KEY, code, 0)
                kbd.syn()
            except OSError as exc:
                print(f"[Dictation] could not release key {code}: {exc!r}")


def _copy(data: bytes, primary: bool = False) -> bool:
    cmd = ["wl-copy", "--primary"] if primary else ["wl-copy"]
    try:
        return subprocess.run(cmd, input=data, timeout=_SUBPROCESS_TIMEOUT_SECONDS).returncode == 0
    except Exception:  # noqa: BLE001 - a missing or hung wl-copy is a failed copy
        return False


def _read(primary: bool = False) -> bytes | None:
    cmd = ["wl-paste", "--no-newline", "--primary"] if primary else ["wl-paste", "--no-newline"]
    try:
        previous = subprocess.run(cmd, capture_output=True, timeout=_SUBPROCESS_TIMEOUT_SECONDS)
    except Exception:  # noqa: BLE001 - restore is best-effort; paste must still happen
        return None
    return previous.stdout if previous.returncode == 0 else None


def type_text(text: str) -> None:
    """Paste text into the focused app via the clipboard + Shift+Insert."""
    previous_clipboard = _read()
    previous_primary = _read(primary=True)

    data = text.encode()
    # PRIMARY too: some terminals paste the primary selection on Shift+Insert
    if not (_copy(data) and _copy(data, primary=True)):
        print("[Dictation] wl-copy failed - is wl-clipboard installed?")
        # CLIPBOARD may already hold the dictated text when only PRIMARY failed
        if previous_clipboard is not None:
            _copy(previous_clipboard)
        return

    try:
        _paste_via_keyboard()
    except Exception as exc:  # noqa: BLE001 - paste failure must not kill the voice loop
        print(
            f"[Dictation] paste keystroke failed: {exc!r} - check rw on /dev/uinput; "
            "the dictated text stays on the clipboard"
        )
        return

    # ponytail: fixed settle delay, same ceiling as the macOS path - a slow app
    # could read the clipboard after the restore and paste the old content
    time.sleep(_PASTE_SETTLE_SECONDS)

    if previous_clipboard is not None:
        _copy(previous_clipboard)
    if previous_primary is not None:
        _copy(previous_primary, primary=True)
