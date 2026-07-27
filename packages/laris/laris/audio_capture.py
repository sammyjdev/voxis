"""Microphone capture for push-to-talk and always-on VAD."""
import queue
import sys
import threading
import time
import types
from collections import namedtuple
from typing import Callable, Generator, Tuple

import numpy as np

from laris import config

_Utterance = Tuple[np.ndarray, int]  # (float32 mono, sample_rate)

# --- DeepFilterNet3 noise reduction (lazy-loaded, optional) ---
_dfn_state = None  # global, lazily initialized on first call


def _init_dfn():
    """Lazily initialize DeepFilterNet3. Returns (model, df_state) or None on failure."""
    global _dfn_state
    if _dfn_state is not None:
        return _dfn_state

    try:
        import torchaudio

        AudioMetaData = namedtuple(
            "AudioMetaData",
            ["sample_rate", "num_frames", "num_channels", "bits_per_sample", "encoding"],
        )
        backend_mod = types.ModuleType("torchaudio.backend")
        backend_mod.common = types.ModuleType("torchaudio.backend.common")
        backend_mod.common.AudioMetaData = AudioMetaData
        sys.modules["torchaudio.backend"] = backend_mod
        sys.modules["torchaudio.backend.common"] = backend_mod.common
        if not hasattr(torchaudio, "info"):
            torchaudio.info = lambda file, **kwargs: AudioMetaData(16000, 0, 1, 16, "PCM_S")

        from df.enhance import init_df

        model, df_state, _ = init_df(log_level="ERROR")
        _dfn_state = (model, df_state)
        return _dfn_state
    except Exception as exc:  # noqa: BLE001
        print(
            f"[Audio] WARN: DeepFilterNet3 unavailable ({exc}), "
            "running without noise reduction"
        )
        _dfn_state = (None, None)
        return _dfn_state


def _denoise_chunk(chunk: np.ndarray, enabled: bool = True) -> np.ndarray:
    """Apply DeepFilterNet3 noise reduction to a chunk of 16kHz mono float32 audio.
    When disabled or unavailable, returns the input unchanged (passthrough)."""
    if not enabled or len(chunk) == 0:
        return chunk

    model, df_state = _init_dfn()
    if model is None:
        return chunk

    try:
        import torch
        import torchaudio.functional as F
        from df.enhance import enhance

        tensor = torch.from_numpy(chunk).unsqueeze(0)  # [1, T]
        tensor_48k = F.resample(tensor, 16000, 48000)
        enhanced_48k = enhance(model, df_state, tensor_48k)
        enhanced_16k = F.resample(enhanced_48k, 48000, 16000)
        result = enhanced_16k.squeeze(0).numpy().astype(np.float32)

        if len(result) > len(chunk):
            result = result[: len(chunk)]
        elif len(result) < len(chunk):
            result = np.pad(result, (0, len(chunk) - len(result)))
        return result
    except Exception:  # noqa: BLE001
        return chunk


def _open_stream(callback):
    import sounddevice as sd

    def _new_stream():
        return sd.InputStream(
            samplerate=config.SAMPLE_RATE,
            channels=1,
            dtype="float32",
            blocksize=config.CHUNK_SAMPLES,
            callback=callback,
        )

    try:
        return _new_stream()
    except sd.PortAudioError:
        # PortAudio enumerates devices once at Pa_Initialize and never sees USB
        # hotplug: unplugging a mic leaves it holding a dead device id, and every
        # later open fails with -9986 until the process restarts. Re-initializing
        # re-enumerates. Only on failure - doing it upfront would add latency to
        # every utterance.
        sd._terminate()
        sd._initialize()
        return _new_stream()


def record_while(
    pressed: threading.Event,
    timeout_s: float | None = None,
    on_chunk: Callable[[np.ndarray], None] | None = None,
) -> np.ndarray:
    """Record from the microphone while pressed is set. Returns float32 mono
    audio. on_chunk sees each captured chunk live (desktop orb amplitude)."""
    timeout_s = timeout_s or config.MAX_SPEECH_DURATION_S
    chunks: list[np.ndarray] = []
    q: queue.Queue[np.ndarray] = queue.Queue()

    def _cb(indata, frames, time_info, status):
        q.put(indata[:, 0].copy())

    def _take(chunk: np.ndarray) -> None:
        chunks.append(chunk)
        if on_chunk is not None:
            on_chunk(chunk)

    start = time.monotonic()
    with _open_stream(_cb):
        while pressed.is_set() and (time.monotonic() - start) < timeout_s:
            try:
                _take(q.get(timeout=0.1))
            except queue.Empty:
                pass
        while not q.empty():
            _take(q.get_nowait())

    if not chunks:
        return np.zeros(0, dtype=np.float32)
    audio = np.concatenate(chunks)
    if config.NOISE_REDUCTION == "on":
        audio = _denoise_chunk(audio, enabled=True)
    return audio


class VadCapture:
    """Iterates voiced utterances detected by Silero VAD, always-on."""

    def __init__(self) -> None:
        try:
            from silero_vad import load_silero_vad, VADIterator
        except ImportError:
            raise SystemExit(
                "[Audio] silero-vad not installed: pip install silero-vad"
            )
        self._model = load_silero_vad()
        self._VADIterator = VADIterator

    def stream_utterances(
        self,
        should_stop: Callable[[], bool] | None = None,
        on_chunk: Callable[[np.ndarray], None] | None = None,
    ) -> Generator[_Utterance, None, None]:
        import torch

        vad = self._VADIterator(
            self._model,
            threshold=config.VAD_THRESHOLD,
            sampling_rate=config.SAMPLE_RATE,
            min_silence_duration_ms=config.VAD_MIN_SILENCE_MS,
        )
        q: queue.Queue[np.ndarray] = queue.Queue()

        def _cb(indata, frames, time_info, status):
            q.put(indata[:, 0].copy())

        voiced: list[np.ndarray] = []
        in_speech = False
        max_chunks = int(
            config.MAX_SPEECH_DURATION_S * config.SAMPLE_RATE / config.CHUNK_SAMPLES
        )
        min_samples = int(config.MIN_SPEECH_DURATION_MS * config.SAMPLE_RATE / 1000)

        with _open_stream(_cb):
            while True:
                # polled even while silent: a blocking get() would make
                # hands-free impossible to stop until the next utterance
                if should_stop is not None and should_stop():
                    return
                try:
                    chunk = q.get(timeout=0.1)
                except queue.Empty:
                    continue
                if on_chunk is not None:
                    on_chunk(chunk)
                event = vad(torch.from_numpy(chunk), return_seconds=False)

                if event and "start" in event:
                    in_speech = True
                    voiced = [chunk]
                    continue

                if in_speech:
                    voiced.append(chunk)
                    ended = (event and "end" in event) or len(voiced) >= max_chunks
                    if ended:
                        in_speech = False
                        audio = np.concatenate(voiced)
                        voiced = []
                        vad.reset_states()
                        if len(audio) >= min_samples:
                            if config.NOISE_REDUCTION == "on":
                                audio = _denoise_chunk(audio, enabled=True)
                            yield audio, config.SAMPLE_RATE
