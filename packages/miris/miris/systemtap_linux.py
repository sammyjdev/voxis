"""System-audio capture on Linux: `pw-record` on the default sink's monitor.

Same lifecycle as the macOS audiotap helper (alive, bytes_written, stop are
inherited): one process writing system.wav until SIGTERM, which finalizes
the WAV header. 16 kHz mono s16 is what transcribe.load_wav_mono16k reads
without resampling; PipeWire does the downmix and resampling."""

import shutil
import subprocess
from pathlib import Path

from miris.systemtap import SystemTap

# no --target: with stream.capture.sink the session manager links the stream
# to the default sink's monitor, i.e. everything the speakers play
PW_RECORD_ARGS = (
    "-P", "{ stream.capture.sink=true }",
    "--rate", "16000", "--channels", "1", "--format", "s16",
)


class PwRecordTap(SystemTap):
    def __init__(self, out_wav: Path):
        self.out_wav = out_wav
        self.proc: subprocess.Popen | None = None

    def requirement(self) -> tuple[str, str, bool]:
        found = shutil.which("pw-record")
        return "pw-record", found or "not found", found is not None

    def start(self) -> None:
        try:
            self.proc = subprocess.Popen(["pw-record", *PW_RECORD_ARGS, str(self.out_wav)])
        except FileNotFoundError:
            raise FileNotFoundError("pw-record not found (install pipewire-bin)") from None
