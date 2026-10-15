"""Voice pipeline configuration."""
import os

from laris.settings import load_settings

_FILE_OVERRIDES = load_settings()


def _setting(key: str, default: str) -> str:
    return _FILE_OVERRIDES.get(key, os.environ.get(key, default))


MODE = _setting("ASSISTANT_MODE", "ptt")  # "ptt" | "always_on"
DICTATE_KEY = _setting("DICTATE_KEY", "cmd_r")

SAMPLE_RATE = 16000
CHUNK_SAMPLES = 512               # Silero VAD window at 16 kHz
MIN_SPEECH_DURATION_MS = 400
MAX_SPEECH_DURATION_S = 30
VAD_THRESHOLD = 0.5
VAD_MIN_SILENCE_MS = 400
# audio kept from before the VAD fires: it fires a few chunks into a word, and
# without this "Qual a latência" became "a latência". 100-300 ms all cut WER
# from 20.6% to 17.7% replaying a recorded conversation (2026-10-15)
PREROLL_MS = 200

NOISE_REDUCTION: str = _setting("NOISE_REDUCTION", "on")  # "on" | "off"

# q4: benchmarked 2026-07-11 vs the f16 turbo - 0.76s vs 0.95s per utterance
# and ~1.2GB less resident RAM, transcription quality tied on pt-BR samples.
# Revert with STT_MODEL=mlx-community/whisper-large-v3-turbo if quality dips.
STT_MODEL: str = _setting("STT_MODEL", "mlx-community/whisper-large-v3-turbo-q4")
# Linux backend (faster-whisper / CTranslate2). Same turbo model family as the
# mlx default; runs float16 on CUDA, int8 on CPU.
STT_MODEL_FASTER: str = _setting("STT_MODEL_FASTER", "large-v3-turbo")
# empty string = Whisper auto-detect per utterance (used by miris for
# bilingual meetings); laris dictation keeps the pt-BR lock by default
STT_LANGUAGE: str = os.environ.get("STT_LANGUAGE", "pt")

# Primes Whisper's decoder with domain vocabulary. Dictated prompts are short,
# and an ICAART 2026 pt-BR study measured 3-4x higher WER on 1-5 word sentences
# than on 11+ word ones - the cause is missing context. Empty = not sent.
INITIAL_PROMPT: str = _setting("INITIAL_PROMPT", "")

ORB_THEME_PATH: str = os.environ.get(
    "ORB_THEME_PATH", "~/.config/voice-at/orb-theme.json"
)
