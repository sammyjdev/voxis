"""Selects the platform backend at runtime. Import is lazy so importing this
package never pulls mlx unless the backend is actually constructed.

macOS transcribes with mlx-whisper (Apple Silicon); Linux with faster-whisper.
The Windows port lives in its own fork."""
from __future__ import annotations

import sys

from laris.platform.base import PlatformBackend


def get_backend() -> PlatformBackend:
    if sys.platform == "darwin":
        from laris.platform.macos import MacOSBackend
        return MacOSBackend()
    if sys.platform.startswith("linux"):
        from laris.platform.linux import LinuxBackend
        return LinuxBackend()
    raise SystemExit(f"[laris] unsupported platform: {sys.platform}")
