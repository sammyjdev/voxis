"""Persisted settings override layer: file > env > default.
The VOXIS tray writes this file; laris reads it once at import (daemon start).
Live reload is deliberately out of scope - the tray restarts the daemon."""
import json
import os
from pathlib import Path

KNOWN_KEYS = (
    "ASSISTANT_MODE", "DICTATE_KEY", "STT_MODEL", "NOISE_REDUCTION",
    "INITIAL_PROMPT",
)


def settings_path() -> Path:
    """Resolved settings file location (env override or default)."""
    return Path(os.environ.get(
        "LARIS_SETTINGS_PATH", "~/.config/voice-at/laris-settings.json"
    )).expanduser()


def load_settings() -> dict[str, str]:
    """Known str-valued keys from the settings file; {} when missing/corrupt.
    Never writes - the file belongs to the tray app."""
    try:
        loaded = json.loads(settings_path().read_text(encoding="utf-8"))
    except (OSError, ValueError, RecursionError):
        # RecursionError: pathological nesting is "corrupt" too - the real
        # file is always a flat dict of at most 5 str keys
        return {}
    if not isinstance(loaded, dict):
        return {}
    return {k: v for k, v in loaded.items()
            if k in KNOWN_KEYS and isinstance(v, str)}
