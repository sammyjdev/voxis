import threading

import numpy as np


def test_record_while_returns_empty_audio_when_event_is_clear(monkeypatch):
    from laris import audio_capture

    class FakeStream:
        def __enter__(self):
            return self

        def __exit__(self, exc_type, exc, tb):
            return False

    monkeypatch.setattr(audio_capture, "_open_stream", lambda _callback: FakeStream())

    audio = audio_capture.record_while(threading.Event())

    assert isinstance(audio, np.ndarray)
    assert audio.dtype == np.float32
    assert audio.size == 0


def test_stream_utterances_stops_when_asked(monkeypatch):
    """Hands-free relies on should_stop being polled even while the mic is
    silent - the generator must return instead of blocking forever."""
    from contextlib import contextmanager

    from laris import audio_capture

    @contextmanager
    def fake_stream(callback):
        yield

    monkeypatch.setattr(audio_capture, "_open_stream", fake_stream)

    vad = object.__new__(audio_capture.VadCapture)
    vad._model = object()
    vad._VADIterator = lambda model, **kwargs: lambda chunk, return_seconds: None

    utterances = list(vad.stream_utterances(should_stop=lambda: True))

    assert utterances == []


def test_record_while_reports_chunks_live(monkeypatch):
    """The desktop orb needs live amplitude while the key is held - each
    captured chunk must reach on_chunk before the recording ends."""
    import threading
    from contextlib import contextmanager

    import numpy as np

    from laris import audio_capture

    pressed = threading.Event()
    pressed.set()

    @contextmanager
    def fake_stream(callback):
        callback(np.full((4, 1), 0.5, dtype=np.float32), 4, None, None)
        yield

    monkeypatch.setattr(audio_capture, "_open_stream", fake_stream)

    seen = []

    def on_chunk(chunk):
        seen.append(chunk.copy())
        pressed.clear()  # stop after the first chunk

    audio = audio_capture.record_while(pressed, on_chunk=on_chunk)

    assert len(seen) == 1
    assert len(audio) == 4


def test_stream_utterances_reports_chunks_live(monkeypatch):
    from contextlib import contextmanager

    import numpy as np

    from laris import audio_capture

    chunk = np.full((4,), 0.2, dtype=np.float32)
    seen = []

    @contextmanager
    def fake_stream(callback):
        callback(chunk.reshape(-1, 1), 4, None, None)
        yield

    monkeypatch.setattr(audio_capture, "_open_stream", fake_stream)

    vad = object.__new__(audio_capture.VadCapture)
    vad._model = object()
    vad._VADIterator = lambda model, **kwargs: lambda t, return_seconds: None

    stops = iter([False, True, True, True])
    list(vad.stream_utterances(should_stop=lambda: next(stops), on_chunk=seen.append))

    assert len(seen) == 1


def test_denoise_off_returns_audio_unchanged():
    """When NOISE_REDUCTION=off, the denoise wrapper is a passthrough."""
    import numpy as np

    from laris.audio_capture import _denoise_chunk

    chunk = np.ones(512, dtype=np.float32) * 0.5
    result = _denoise_chunk(chunk, enabled=False)
    assert np.array_equal(result, chunk)


def test_denoise_on_changes_audio():
    """When NOISE_REDUCTION=on and DFN3 is loaded, the output differs from input
    (a real denoiser changes the signal)."""
    import numpy as np

    from laris.audio_capture import _denoise_chunk

    chunk = np.random.randn(512 * 8).astype(np.float32) * 0.3
    result = _denoise_chunk(chunk, enabled=True)
    assert result.dtype == np.float32
    assert len(result) == len(chunk)
    assert not np.array_equal(result, chunk)


def test_denoise_handles_short_chunk():
    """Shorter-than-frame chunks must not crash the denoiser — buffer them."""
    import numpy as np

    from laris.audio_capture import _denoise_chunk

    chunk = np.ones(100, dtype=np.float32) * 0.5
    result = _denoise_chunk(chunk, enabled=True)
    assert len(result) == 100
    assert result.dtype == np.float32


class _FakePortAudioError(Exception):
    pass


class _FakeSd:
    """Stands in for the sounddevice module inside _open_stream."""

    PortAudioError = _FakePortAudioError

    def __init__(self, failures: int) -> None:
        self.failures = failures
        self.opens = 0
        self.calls: list[str] = []

    def InputStream(self, **_kwargs):  # noqa: N802 - mirrors sounddevice's name
        self.opens += 1
        if self.opens <= self.failures:
            raise _FakePortAudioError(
                "Error opening InputStream: Internal PortAudio error", -9986
            )
        return "stream"

    def _terminate(self):
        self.calls.append("terminate")

    def _initialize(self):
        self.calls.append("initialize")


def test_open_stream_reinitializes_portaudio_after_a_usb_replug(monkeypatch):
    """PortAudio enumerates devices once at init and is blind to USB hotplug:
    after a replug its cached device id is dead and opening raises -9986. A
    re-init is the only way to see the new device without restarting laris."""
    import sys

    from laris import audio_capture

    fake = _FakeSd(failures=1)
    monkeypatch.setitem(sys.modules, "sounddevice", fake)

    stream = audio_capture._open_stream(lambda *_a: None)

    assert stream == "stream"
    assert fake.calls == ["terminate", "initialize"]
    assert fake.opens == 2


def test_open_stream_does_not_reinitialize_when_the_device_is_healthy(monkeypatch):
    """The retry must cost nothing on the happy path - re-initializing
    PortAudio before every utterance would add latency to every dictation."""
    import sys

    from laris import audio_capture

    fake = _FakeSd(failures=0)
    monkeypatch.setitem(sys.modules, "sounddevice", fake)

    assert audio_capture._open_stream(lambda *_a: None) == "stream"
    assert fake.calls == []
    assert fake.opens == 1


def _speak_then_stop(monkeypatch, voiced_chunks, read_before_stop=None, captured_after_stop=0):
    """The mic captures `voiced_chunks` chunks of speech; VAD hears it start on
    the first and never hears it end. The stop comes once the loop has read
    `read_before_stop` of them (all, by default), the rest still queued; or,
    with `captured_after_stop`, while the loop is busy elsewhere, and that
    many more chunks reach the mic after it."""
    from contextlib import contextmanager

    from laris import audio_capture, config

    chunk = np.full((config.CHUNK_SAMPLES,), 0.2, dtype=np.float32)
    state = {"stopped": False, "read": 0}
    read = voiced_chunks if read_before_stop is None else read_before_stop

    @contextmanager
    def fake_stream(callback):
        for _ in range(voiced_chunks):
            callback(chunk.reshape(-1, 1), len(chunk), None, None)
        if captured_after_stop:
            state["stopped"] = True
            for _ in range(captured_after_stop):
                callback(chunk.reshape(-1, 1), len(chunk), None, None)
        yield

    def count_read(_chunk):
        state["read"] += 1
        if state["read"] >= read:
            state["stopped"] = True

    events = iter([{"start": 0}])
    monkeypatch.setattr(audio_capture, "_open_stream", fake_stream)
    monkeypatch.setattr(config, "NOISE_REDUCTION", "off")
    vad = object.__new__(audio_capture.VadCapture)
    vad._model = object()
    vad._VADIterator = lambda model, **kwargs: type(
        "FakeVad", (), {"__call__": lambda self, t, return_seconds: next(events, None),
                        "reset_states": lambda self: None})()

    utterances = list(vad.stream_utterances(should_stop=lambda: state["stopped"],
                                            on_chunk=count_read))
    return utterances, len(chunk)


def test_stopping_mid_sentence_still_delivers_what_was_said(monkeypatch):
    """A tap right after speaking used to drop the words: the utterance only
    closes after 400 ms of silence, and the stop came first."""
    utterances, chunk_len = _speak_then_stop(monkeypatch, voiced_chunks=16)

    assert [(len(audio), rate) for audio, rate in utterances] == [(16 * chunk_len, 16000)]


def test_audio_still_queued_when_the_stop_arrives_is_part_of_the_last_utterance(monkeypatch):
    """The mic callback runs ahead of the loop; the chunks it queued before the
    stop hold the last syllables."""
    utterances, chunk_len = _speak_then_stop(monkeypatch, voiced_chunks=16, read_before_stop=4)

    assert [(len(audio), rate) for audio, rate in utterances] == [(16 * chunk_len, 16000)]


def test_speech_reaching_the_mic_after_the_stop_is_not_delivered(monkeypatch):
    """A stop tapped while an utterance is being transcribed is seen only once
    the loop resumes; what the mic picked up after the tap must not be pasted."""
    utterances, chunk_len = _speak_then_stop(monkeypatch, voiced_chunks=16, captured_after_stop=16)

    assert [(len(audio), rate) for audio, rate in utterances] == [(16 * chunk_len, 16000)]


def test_a_blip_shorter_than_the_minimum_speech_at_stop_is_dropped(monkeypatch):
    utterances, _ = _speak_then_stop(monkeypatch, voiced_chunks=2)

    assert utterances == []
