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
