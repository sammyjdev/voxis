"""Daemon-side handle for the desktop dictation indicator.

The indicator itself is a separate AppKit process (overlay_app.py):
AppKit needs to own a main thread's run loop, and uvicorn already owns
ours. The child is spawned lazily and controlled over stdin with
one-line commands ("show dictate", "show command", "show transcribing",
"hide"); daemon death closes the pipe and the child exits with it.

The child renders the SAME web orb as the dashboard: a transparent
WKWebView pointed at /overlay (URL passed via OVERLAY_URL env), which
subscribes to /ws/voice on its own - visuals and captions stay in one
place, the frontend.
"""
from __future__ import annotations

import os
import subprocess
import sys


class Overlay:
    def __init__(self, url: str = "", argv: list[str] | None = None,
                 env: dict[str, str] | None = None) -> None:
        """argv/env: the child to run; macOS's AppKit panel by default, Linux
        passes its GTK popup (overlay_linux.py)."""
        self._url = url
        self._argv = argv or [sys.executable, "-m", "laris.overlay_app"]
        self._env = env or {}
        self._proc: subprocess.Popen | None = None

    def _send(self, line: str) -> None:
        try:
            if self._proc is None or self._proc.poll() is not None:
                self._proc = subprocess.Popen(
                    self._argv,
                    stdin=subprocess.PIPE,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    env={**os.environ, **self._env, "OVERLAY_URL": self._url},
                )
            self._proc.stdin.write(f"{line}\n".encode())
            self._proc.stdin.flush()
        except Exception:  # noqa: BLE001 - a cosmetic overlay must never break the voice loop
            self._proc = None

    def show(self, mode: str) -> None:
        self._send(f"show {mode}")

    def hide(self) -> None:
        self._send("hide")
