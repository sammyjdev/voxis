"""STT via faster-whisper (CTranslate2), the Linux backend's transcriber;
macOS keeps mlx-whisper (transcriber.py). Device is chosen at runtime: CUDA
float16 when present, else CPU int8. Restored from the Windows port
(b39a406^) with INITIAL_PROMPT parity added."""
from __future__ import annotations

import numpy as np

from faster_whisper import WhisperModel

from laris import config


# CTranslate2 4.x wheels need the CUDA 12 cuBLAS and cuDNN 9. Lt before
# cuBLAS: cuBLAS needs it, and cudnn's RPATH $ORIGIN finds its own sub-libraries.
_CUDA_LIBRARIES = (
    ("cublas", ("libcublasLt.so.12", "libcublas.so.12")),
    ("cudnn", ("libcudnn.so.9",)),
)


def _load_cuda_libraries() -> bool:
    """CTranslate2 dlopens cuBLAS and cuDNN by soname on the first inference,
    not at model load, so a missing library crashed warm() after CUDA had
    already been chosen. Load them now - from the nvidia-* wheels when
    installed (requirements-linux.txt), else by soname from the system. Once
    loaded RTLD_GLOBAL, CTranslate2's own dlopen resolves to these."""
    import ctypes
    import importlib
    import os

    for package, names in _CUDA_LIBRARIES:
        try:
            module = importlib.import_module(f"nvidia.{package}")
            lib_dir = os.path.join(list(module.__path__)[0], "lib")
        except ImportError:
            lib_dir = None
        for name in names:
            try:
                ctypes.CDLL(os.path.join(lib_dir, name) if lib_dir else name,
                            mode=ctypes.RTLD_GLOBAL)
            except OSError:
                return False
    return True


def _cuda_available() -> bool:
    """True only if a GPU is present AND the libraries inference needs load.
    Best-effort: any failure (no driver, no NVIDIA GPU, ctranslate2 built
    CPU-only, no cuBLAS/cuDNN) means CPU."""
    try:
        import ctranslate2
        if ctranslate2.get_cuda_device_count() == 0:
            return False
    except Exception:  # noqa: BLE001 - absence of CUDA is the common case, not an error
        return False
    return _load_cuda_libraries()


class FasterWhisperTranscriber:
    def __init__(self) -> None:
        if _cuda_available():
            device, compute_type = "cuda", "float16"
        else:
            device, compute_type = "cpu", "int8"
        self._model = WhisperModel(
            config.STT_MODEL_FASTER, device=device, compute_type=compute_type
        )

    def warm(self) -> None:
        """Force model load + first inference so the first real utterance
        doesn't pay it."""
        self.transcribe(np.zeros(config.SAMPLE_RATE // 2, dtype=np.float32),
                        config.SAMPLE_RATE)

    def detect_language(self, audio: np.ndarray) -> str:
        """Whisper's language guess over the first 30 s of audio."""
        return self._model.detect_language(audio.astype(np.float32))[0]

    def transcribe(self, audio: np.ndarray, sample_rate: int = config.SAMPLE_RATE,
                   language: str | None = None) -> str:
        """A non-empty `language` overrides STT_LANGUAGE for this call."""
        if len(audio) == 0:
            return ""
        # omitted entirely when unset, same rule as transcriber.py: an empty
        # initial_prompt still primes the decoder and can leak into the text
        extra = (
            {"initial_prompt": config.INITIAL_PROMPT}
            if config.INITIAL_PROMPT
            else {}
        )
        segments, _info = self._model.transcribe(
            audio.astype(np.float32),
            language=language or config.STT_LANGUAGE or None,  # "" -> auto-detect
            task="transcribe",
            condition_on_previous_text=False,  # same anti-repetition fix as mlx path
            **extra,
        )
        return "".join(seg.text for seg in segments).strip()
