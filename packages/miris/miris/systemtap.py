"""Own the audiotap helper process (system-audio capture)."""

import os
import signal
import subprocess
import sys
from pathlib import Path

DEFAULT_BIN = Path(__file__).resolve().parent.parent / "helper" / "audiotap"


class SystemTap:
    def __init__(self, out_wav: Path, binary: Path | None = None):
        self.out_wav = out_wav
        # explicit binary wins; MIRIS_AUDIOTAP only overrides the default
        self.binary = Path(binary or os.environ.get("MIRIS_AUDIOTAP") or DEFAULT_BIN)
        self.proc: subprocess.Popen | None = None

    def requirement(self) -> tuple[str, str, bool]:
        """(what, where, present) for `miris doctor`."""
        return "audiotap binary", str(self.binary), self.binary.exists()

    def start(self) -> None:
        if not self.binary.exists():
            raise FileNotFoundError(
                f"audiotap binary not found: {self.binary} (run: make -C helper)"
            )
        self.proc = subprocess.Popen([str(self.binary), str(self.out_wav)])

    def alive(self) -> bool:
        return self.proc is not None and self.proc.poll() is None

    def bytes_written(self) -> int:
        try:
            return self.out_wav.stat().st_size
        except FileNotFoundError:
            return 0

    def stop(self, grace_s: float = 5.0) -> None:
        if self.proc is None or self.proc.poll() is not None:
            return
        self.proc.send_signal(signal.SIGTERM)
        try:
            self.proc.wait(timeout=grace_s)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()


def default_tap_factory(out_wav: Path) -> SystemTap:
    """The system-audio tap for this platform: pw-record on Linux, the
    audiotap helper on macOS."""
    if sys.platform.startswith("linux"):
        from miris.systemtap_linux import PwRecordTap
        return PwRecordTap(out_wav)
    return SystemTap(out_wav)
