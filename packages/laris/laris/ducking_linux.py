"""Duck the default output while hands-free listens, on Linux (PipeWire).

Same contract as ducking.py on macOS: speakers bleeding into the mic degrade
transcription, so the output drops to DUCK_VOLUME for the session and comes
back after. The sink is remembered by id, so a headset plugged in mid-session
does not get the old speakers' volume. Known ceiling, as on macOS: a volume
the user changes while ducked is overwritten by restore().
"""
from __future__ import annotations

import subprocess

# a hung wpctl must not freeze the single-threaded voice loop
_SUBPROCESS_TIMEOUT_SECONDS = 4
DUCK_VOLUME = 0.20  # wpctl scale, 1.0 = 100%; macOS ducks to 20 of 100


def _wpctl(*args: str) -> str:
    result = subprocess.run(["wpctl", *args], capture_output=True,
                            timeout=_SUBPROCESS_TIMEOUT_SECONDS)
    return result.stdout.decode()


class WpctlDucker:
    def __init__(self) -> None:
        self._saved: tuple[str, str] | None = None  # (sink id, volume)

    def duck(self) -> None:
        if self._saved is not None:
            return  # already ducked: keep the volume from before the first duck
        try:
            # "id 60, type PipeWire:Interface:Node"
            sink_id = _wpctl("inspect", "@DEFAULT_AUDIO_SINK@").split()[1].rstrip(",")
            # "Volume: 0.40" (with " [MUTED]" when muted)
            volume = _wpctl("get-volume", sink_id).split()[1]
            if float(volume) <= DUCK_VOLUME:
                return
            _wpctl("set-volume", sink_id, f"{DUCK_VOLUME:.2f}")
            self._saved = (sink_id, volume)
        except Exception:  # noqa: BLE001 - ducking is best-effort
            pass

    def restore(self) -> None:
        if self._saved is None:
            return
        (sink_id, volume), self._saved = self._saved, None
        try:
            _wpctl("set-volume", sink_id, volume)
        except Exception:  # noqa: BLE001
            pass
