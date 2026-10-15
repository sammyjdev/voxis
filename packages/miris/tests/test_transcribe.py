import wave
from pathlib import Path

import numpy as np

from miris import transcribe
from miris.transcribe import Segment


def _write_wav(path: Path, audio: np.ndarray, sr: int, channels: int = 1):
    with wave.open(str(path), "wb") as w:
        w.setnchannels(channels)
        w.setsampwidth(2)
        w.setframerate(sr)
        w.writeframes((np.clip(audio, -1, 1) * 32767).astype(np.int16).tobytes())


def test_load_wav_16k_mono_roundtrip(tmp_path):
    audio = np.sin(np.linspace(0, 100, 16000)).astype(np.float32) * 0.5
    _write_wav(tmp_path / "a.wav", audio, 16000)
    loaded = transcribe.load_wav_mono16k(tmp_path / "a.wav")
    assert loaded.shape == (16000,)
    assert np.abs(loaded - audio).max() < 0.01


def test_load_wav_downmixes_and_resamples(tmp_path):
    stereo = np.zeros(48000 * 2, dtype=np.float32)  # 1s of 48k stereo interleaved
    _write_wav(tmp_path / "b.wav", stereo, 48000, channels=2)
    loaded = transcribe.load_wav_mono16k(tmp_path / "b.wav")
    assert abs(len(loaded) - 16000) <= 1


def test_load_wav_tolerates_truncated_stereo(tmp_path):
    path = tmp_path / "trunc.wav"
    _write_wav(path, np.zeros(3200, dtype=np.float32), 16000, channels=2)
    with open(path, "r+b") as f:  # chop 2 bytes so samples % channels != 0
        f.truncate(path.stat().st_size - 2)
    loaded = transcribe.load_wav_mono16k(path)
    assert len(loaded) > 0


def test_transcribe_channel_uses_spans_and_transcriber(tmp_path):
    audio = np.zeros(32000, dtype=np.float32)  # 2s @16k
    _write_wav(tmp_path / "c.wav", audio, 16000)

    class FakeTranscriber:
        def transcribe(self, chunk):
            return f"len={len(chunk)}"

    spans = lambda a, sr=16000: [(0, 16000), (16000, 32000)]
    segs = transcribe.transcribe_channel(tmp_path / "c.wav", "Eu", FakeTranscriber(), spans_fn=spans)
    assert segs == [
        Segment(0.0, 1.0, "Eu", "len=16000"),
        Segment(1.0, 2.0, "Eu", "len=16000"),
    ]


def test_collapse_repetitions_caps_hallucinated_runs():
    text = "essas " + "pessoas " * 200 + "também"
    result = transcribe.collapse_repetitions(text.strip())
    assert result == "essas pessoas pessoas pessoas também"
    assert transcribe.collapse_repetitions("um dois três") == "um dois três"


def test_merge_interleaves_by_timestamp():
    me = [Segment(0.0, 1.0, "Eu", "oi"), Segment(65.0, 66.0, "Eu", "tchau")]
    them = [Segment(2.5, 4.0, "Eles", "olá")]
    assert transcribe.merge(me, them) == (
        "[00:00] Eu: oi\n[00:02] Eles: olá\n[01:05] Eu: tchau"
    )


def test_a_channel_is_transcribed_in_the_language_heard_over_its_first_30_seconds_of_speech(tmp_path):
    """Whisper guessing per utterance turned 12% of pt-BR meeting lines into
    English, Russian or Japanese (CORAA benchmark, 2026-10-15): one second of
    speech is too little to tell. One guess per channel over 30 s of its
    speech got 40 of 40 windows right in pt-BR and in English."""
    sr = 16000
    audio = np.zeros(50 * sr, dtype=np.float32)
    audio[: 20 * sr] = 0.5
    audio[25 * sr: 45 * sr] = 0.25
    _write_wav(tmp_path / "c.wav", audio, sr)
    spans = lambda a, sr=16000: [(0, 20 * 16000), (25 * 16000, 45 * 16000)]
    calls = []

    def seconds_at(probe, level):
        return int(np.isclose(probe, level, atol=0.01).sum()) // sr

    class DetectingTranscriber:
        def detect_language(self, probe):
            # first span whole, then the start of the second; no silence
            calls.append(("detect", len(probe), seconds_at(probe[: 20 * sr], 0.5),
                          seconds_at(probe[20 * sr:], 0.25)))
            return "pt"

        def transcribe(self, chunk, language=None):
            calls.append(("transcribe", language))
            return "fala"

    transcribe.transcribe_channel(tmp_path / "c.wav", "Eles", DetectingTranscriber(), spans_fn=spans)

    # one guess, over 30 s made of speech only, applied to every utterance
    assert calls == [("detect", 30 * sr, 20, 10), ("transcribe", "pt"), ("transcribe", "pt")]


def test_a_silent_channel_never_asks_for_a_language(tmp_path):
    _write_wav(tmp_path / "s.wav", np.zeros(16000, dtype=np.float32), 16000)

    class DetectingTranscriber:
        def detect_language(self, probe):
            raise AssertionError("asked for the language of silence")

    segs = transcribe.transcribe_channel(tmp_path / "s.wav", "Eu", DetectingTranscriber(),
                                         spans_fn=lambda a, sr=16000: [])

    assert segs == []


def test_a_channel_whose_language_cannot_be_detected_is_still_transcribed(tmp_path, capsys):
    """Detection is an accuracy aid: if it breaks, the channel falls back to
    Whisper's per-utterance guess instead of dropping out of the meeting."""
    _write_wav(tmp_path / "c.wav", np.zeros(16000, dtype=np.float32), 16000)
    languages = []

    class BrokenDetector:
        def detect_language(self, probe):
            raise RuntimeError("CUDA out of memory")

        def transcribe(self, chunk, language=None):
            languages.append(language)
            return "fala"

    segs = transcribe.transcribe_channel(tmp_path / "c.wav", "Eu", BrokenDetector(),
                                         spans_fn=lambda a, sr=16000: [(0, 16000)])

    assert segs == [Segment(0.0, 1.0, "Eu", "fala")]
    assert languages == [None]
    assert "language detection failed (CUDA out of memory)" in capsys.readouterr().err
