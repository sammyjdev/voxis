"""STT via mlx-whisper (Metal/Apple Silicon), locked to pt-BR - ported
from ~/code/random/macos/transcriber.py, with one fix applied here:
condition_on_previous_text=False (avoids a known Whisper failure mode,
infinite repetition of a phrase - the source being ported does not set
this parameter)."""
import numpy as np

from laris import config

_MIN_MLX_WHISPER = (0, 31, 2)


def _version_tuple(v: str) -> tuple:
    parts = []
    for p in v.split(".")[:3]:
        digits = "".join(ch for ch in p if ch.isdigit())
        parts.append(int(digits) if digits else 0)
    return tuple(parts)


class Transcriber:
    def __init__(self) -> None:
        try:
            import mlx_whisper
        except ImportError:
            raise SystemExit(
                "[STT] mlx-whisper not installed (requires Apple Silicon):\n"
                "  pip install 'mlx-whisper>=0.4.0'"
            )
        # the mlx module exposes no __version__ attribute; read it from the
        # installed distribution metadata (getattr fell back to "0" and made
        # the thread-safety warning fire on every run, even on new versions)
        try:
            from importlib.metadata import version as _dist_version
            version = _dist_version("mlx")
        except Exception:  # noqa: BLE001 - metadata missing is not fatal
            version = "0"
        if _version_tuple(version) < _MIN_MLX_WHISPER:
            print(
                f"[STT] WARNING: mlx {version} < 0.31.2 - "
                "older versions are not thread-safe. Upgrade."
            )
        self._mlx_whisper = mlx_whisper
        self.model = config.STT_MODEL

    def warm(self) -> None:
        """Force model load + Metal compile now (~2s) so the first real
        utterance doesn't pay it - measured 2.9s cold vs 0.8s warm."""
        self.transcribe(np.zeros(config.SAMPLE_RATE // 2, dtype=np.float32))

    def detect_language(self, audio: np.ndarray) -> str:
        """Whisper's language guess over the first 30 s of audio. mlx-whisper
        exposes it only through transcribe(), which detects with language=None."""
        result = self._mlx_whisper.transcribe(
            audio[: 30 * config.SAMPLE_RATE].astype(np.float32),
            path_or_hf_repo=self.model,
            language=None,
            task="transcribe",
            word_timestamps=False,   # do NOT change: known memory leak with True
            fp16=True,
            condition_on_previous_text=False,
        )
        return result["language"]

    def transcribe(self, audio: np.ndarray, sample_rate: int = config.SAMPLE_RATE,
                   language: str | None = None) -> str:
        """A non-empty `language` overrides STT_LANGUAGE for this call."""
        if len(audio) == 0:
            return ""
        # omitted entirely when unset: an empty initial_prompt still primes the
        # decoder and can leak into the transcript
        extra = (
            {"initial_prompt": config.INITIAL_PROMPT}
            if config.INITIAL_PROMPT
            else {}
        )
        result = self._mlx_whisper.transcribe(
            audio.astype(np.float32),
            path_or_hf_repo=self.model,
            language=language or config.STT_LANGUAGE or None,  # "" -> auto-detect
            task="transcribe",
            word_timestamps=False,   # do NOT change: known memory leak with True
            fp16=True,
            condition_on_previous_text=False,
            **extra,
        )
        return (result.get("text") or "").strip()
