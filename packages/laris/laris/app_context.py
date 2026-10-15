"""App-aware formatting: detects the frontmost macOS app and formats
transcribed text accordingly. Code editors get literal passthrough; prose
apps get basic punctuation/capitalization restoration.

Uses NSWorkspace.sharedWorkspace().frontmostApplication().bundleIdentifier()
via pyobjc to identify the focused app."""
from __future__ import annotations

_CODE_EDITORS = {
    "com.microsoft.VSCode",
    "com.todesktop.230313mzl4w4u92",  # Cursor
    "com.apple.dt.Xcode",
    "com.googlecode.iterm2",
    "com.apple.Terminal",
    "com.sublimetext.4",
    "com.sublimetext.3",
    "dev.warp.Warp-Stable",
}

_PROSE_APPS = {
    "com.apple.mail",
    "com.apple.Notes",
    "com.apple.TextEdit",
    "com.tinyspeck.slackmacgap",  # Slack
    "com.apple.Safari",
    "com.google.Chrome",
    "org.mozilla.firefox",
}


def _get_workspace():
    from AppKit import NSWorkspace
    return NSWorkspace.sharedWorkspace()


def current_bundle_id() -> str | None:
    """Returns the bundle ID of the frontmost macOS app, or None on failure."""
    try:
        ws = _get_workspace()
        app = ws.frontmostApplication()
        if app is None:
            return None
        return app.bundleIdentifier()
    except Exception:  # noqa: BLE001 - if pyobjc fails, return None
        return None


def current_format_mode() -> str:
    """Returns 'code' or 'prose' based on the frontmost app.
    Default is 'prose' (most typing is prose)."""
    bundle_id = current_bundle_id()
    if bundle_id is None:
        return "prose"
    if bundle_id in _CODE_EDITORS:
        return "code"
    return "prose"


def format_for_focus(text: str) -> str:
    """Applies the appropriate formatting based on the frontmost app.
    - 'code' mode: literal passthrough (preserve as-transcribed)
    - 'prose' mode: capitalize first letter, append period if missing
    """
    mode = current_format_mode()
    if mode == "code":
        return text

    words = text.lstrip(" ")
    if not words:
        return text
    space = text[: len(text) - len(words)]  # hands-free's separator stays in front
    text = words[0].upper() + words[1:]
    terminal = (".", "!", "?", ":")
    if not text.endswith(terminal) and "\n" not in text:
        text += "."
    return space + text
