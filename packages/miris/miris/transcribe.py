"""Per-channel STT (mlx-whisper via laris) + timeline merge."""
import sys
import wave
from dataclasses import dataclass
from pathlib import Path

import numpy as np

TARGET_SR = 16000


@dataclass(frozen=True)
class Segment:
    start_s: float
    end_s: float
    speaker: str
    text: str


def load_wav_mono16k(path: Path) -> np.ndarray:
    with wave.open(str(path), "rb") as w:
        sr = w.getframerate()
        n_ch = w.getnchannels()
        width = w.getsampwidth()
        raw = w.readframes(w.getnframes())
    if width == 2:
        audio = np.frombuffer(raw, dtype=np.int16).astype(np.float32) / 32768.0
    else:
        raise ValueError(f"unsupported sample width {width} in {path}")
    if n_ch > 1:
        audio = audio[: len(audio) - len(audio) % n_ch]  # tolerate truncated WAVs
        audio = audio.reshape(-1, n_ch).mean(axis=1)
    if sr != TARGET_SR:
        import torch
        import torchaudio.functional as F

        audio = F.resample(torch.from_numpy(audio), sr, TARGET_SR).numpy()
    return audio


def speech_spans(audio: np.ndarray, sr: int = TARGET_SR) -> list[tuple[int, int]]:
    """Silero VAD over the whole file -> utterance-sized (start, end) sample spans."""
    import torch
    from silero_vad import get_speech_timestamps, load_silero_vad

    model = load_silero_vad()
    stamps = get_speech_timestamps(torch.from_numpy(audio), model, sampling_rate=sr)
    return [(t["start"], t["end"]) for t in stamps]


def collapse_repetitions(text: str, max_run: int = 3) -> str:
    """Cap consecutive repeated words - Whisper's hallucination mode on noisy
    or overlapping audio repeats one token hundreds of times."""
    out: list[str] = []
    run = 0
    for word in text.split():
        if out and word.casefold() == out[-1].casefold():
            run += 1
            if run >= max_run:
                continue
        else:
            run = 0
        out.append(word)
    return " ".join(out)


LANGUAGE_PROBE_S = 30  # one Whisper window


def channel_language(audio: np.ndarray, spans: list[tuple[int, int]], transcriber) -> str | None:
    """One language guess for a whole channel, over its first 30 s of speech.
    Whisper guessing per utterance mislabels short ones (a 1 s "bom dia" came
    out as German); None when the transcriber cannot or should not guess."""
    detect = getattr(transcriber, "detect_language", None)
    if detect is None or not spans:
        return None
    pieces, left = [], LANGUAGE_PROBE_S * TARGET_SR
    for start, end in spans:  # stop at 30 s: a long meeting is never copied whole
        pieces.append(audio[start:min(end, start + left)])
        left -= len(pieces[-1])
        if left <= 0:
            break
    try:
        return detect(np.concatenate(pieces))
    except Exception as exc:  # noqa: BLE001 - an aid, never worth losing the channel
        print(f"[transcribe] language detection failed ({exc}) - guessing per utterance",
              file=sys.stderr)
        return None


def transcribe_channel(path: Path, speaker: str, transcriber,
                       spans_fn=speech_spans) -> list[Segment]:
    audio = load_wav_mono16k(path)
    spans = spans_fn(audio)
    language = channel_language(audio, spans, transcriber)
    kwargs = {"language": language} if language else {}
    segments = []
    for start, end in spans:
        text = collapse_repetitions(transcriber.transcribe(audio[start:end], **kwargs))
        if text:
            segments.append(Segment(start / TARGET_SR, end / TARGET_SR, speaker, text))
    return segments


def merge(*channels: list[Segment]) -> str:
    ordered = sorted((s for ch in channels for s in ch), key=lambda s: s.start_s)
    lines = []
    for seg in ordered:
        minutes, seconds = divmod(int(seg.start_s), 60)
        lines.append(f"[{minutes:02d}:{seconds:02d}] {seg.speaker}: {seg.text}")
    return "\n".join(lines)
