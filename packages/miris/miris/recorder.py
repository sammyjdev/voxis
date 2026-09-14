"""Reliable two-channel recording process."""

import dataclasses
import datetime as dt
import os
import queue
import signal
import sys
import threading
import time
import wave
from pathlib import Path
from typing import Callable

import numpy as np

from miris import session
from miris.systemtap import SystemTap

SAMPLE_RATE = 16000
CHUNK_SAMPLES = 1600  # 100ms
START_TIMEOUT_S = 5.0
STALL_TIMEOUT_S = 2.0
WATCHDOG_INTERVAL_S = 0.25
QUEUE_CHUNKS = 50
MANIFEST_INTERVAL_S = 1.0
WAV_HEADER_BYTES = 44


class WavWriter:
    """Incremental 16-bit PCM mono WAV writer."""

    def __init__(self, path: Path, sample_rate: int = SAMPLE_RATE):
        self._wav = wave.open(str(path), "wb")
        self._wav.setnchannels(1)
        self._wav.setsampwidth(2)
        self._wav.setframerate(sample_rate)

    def write(self, chunk: np.ndarray) -> None:
        pcm = np.clip(chunk, -1.0, 1.0)
        self._wav.writeframes((pcm * 32767).astype(np.int16).tobytes())

    def close(self) -> None:
        self._wav.close()


def _default_open_stream(callback):
    import sounddevice as sd

    return sd.InputStream(
        samplerate=SAMPLE_RATE,
        channels=1,
        dtype="float32",
        blocksize=CHUNK_SAMPLES,
        callback=callback,
    )


def _error(code: str, exc: object) -> session.SessionError:
    return session.SessionError(
        code, str(exc), dt.datetime.now().astimezone().isoformat()
    )


def _partial_channels(session_dir: Path) -> dict[str, session.ChannelInfo | None]:
    channels: dict[str, session.ChannelInfo | None] = {"mic": None, "system": None}
    for name in channels:
        try:
            channels[name] = session.inspect_wav(session_dir / f"{name}.wav")
        except (EOFError, OSError, session.CaptureInvalid):
            pass
    return channels


def _failure_channels(session_dir: Path) -> dict[str, session.ChannelInfo | None]:
    try:
        return session.validate_capture(session_dir, max_duration_delta_s=None)
    except (EOFError, OSError, wave.Error, session.CaptureInvalid):
        return _partial_channels(session_dir)


def _progress_channels(
    session_dir: Path, mic_frames: int, system_bytes: int
) -> dict[str, session.ChannelInfo | None]:
    channels: dict[str, session.ChannelInfo | None] = {"mic": None, "system": None}
    if mic_frames:
        channels["mic"] = session.ChannelInfo("mic.wav", mic_frames, SAMPLE_RATE, 1)
    try:
        with wave.open(str(session_dir / "system.wav"), "rb") as wav:
            channels_count = wav.getnchannels()
            sample_rate = wav.getframerate()
            sample_width = wav.getsampwidth()
        frames = max(
            0, (system_bytes - WAV_HEADER_BYTES) // (channels_count * sample_width)
        )
        if frames:
            channels["system"] = session.ChannelInfo(
                "system.wav", frames, sample_rate, channels_count
            )
    except (EOFError, OSError, wave.Error):
        pass
    return channels


def _heartbeat(session_dir: Path, mic_frames: int, system_bytes: int) -> None:
    manifest = session.load(session_dir)
    session.write(
        session_dir,
        dataclasses.replace(
            manifest,
            updated_at=dt.datetime.now().astimezone().isoformat(),
            channels=_progress_channels(session_dir, mic_frames, system_bytes),
        ),
    )


def record(
    session_dir: Path,
    stop: threading.Event,
    tap_factory: Callable[[Path], SystemTap] = SystemTap,
    open_stream: Callable | None = None,
    now: Callable[[], float] = time.monotonic,
) -> session.SessionState:
    """Record microphone and system audio, returning the terminal state."""
    tap: SystemTap | None = None
    writer: WavWriter | None = None
    first_error: list[tuple[str, object] | None] = [None]
    q: queue.Queue[np.ndarray] = queue.Queue(maxsize=QUEUE_CHUNKS)
    mic_frames = 0
    mic_callback_frames = 0
    ready = False
    state = session.SessionState.STARTING

    def fail(code: str, detail: object) -> None:
        if first_error[0] is None:
            first_error[0] = (code, detail)

    def callback(indata, frames, time_info, status):
        nonlocal mic_callback_frames
        if first_error[0] is not None:
            return
        if status:
            fail("microphone_error", status)
            return
        try:
            q.put_nowait(indata[:, 0].copy())
            mic_callback_frames += len(indata)
        except queue.Full:
            fail("microphone_queue_overflow", "microphone queue is full")

    try:
        try:
            tap = tap_factory(session_dir / "system.wav")
            tap.start()
            initial_system_bytes = tap.bytes_written()
        except Exception as exc:
            error = _error("system_tap_start_failed", exc)
            session.transition(session_dir, session.SessionState.FAILED, error=error)
            return session.SessionState.FAILED

        try:
            writer = WavWriter(session_dir / "mic.wav")
            stream_factory = open_stream or _default_open_stream
            stream = stream_factory(callback)
        except Exception as exc:
            fail("microphone_start_failed", exc)
            stream = None

        started_at = now()
        last_mic_frames = 0
        last_system_bytes = initial_system_bytes
        last_mic_progress = started_at
        last_system_progress = started_at
        next_manifest = started_at + MANIFEST_INTERVAL_S

        if stream is not None:
            try:
                with stream:
                    while True:
                        try:
                            chunk = q.get(
                                timeout=0 if stop.is_set() else WATCHDOG_INTERVAL_S
                            )
                            writer.write(chunk)
                            mic_frames += len(chunk)
                        except queue.Empty:
                            pass
                        except Exception as exc:
                            fail("writer_error", exc)

                        current = now()
                        try:
                            system_bytes = tap.bytes_written()
                            alive = tap.alive()
                        except Exception as exc:
                            fail("system_tap_error", exc)
                            system_bytes = last_system_bytes
                            alive = False
                        if not alive and first_error[0] is None:
                            fail("system_tap_died", "system audio helper exited")

                        if mic_callback_frames > last_mic_frames:
                            last_mic_frames = mic_callback_frames
                            last_mic_progress = current
                        if system_bytes > last_system_bytes:
                            last_system_bytes = system_bytes
                            last_system_progress = current

                        if (
                            not ready
                            and mic_frames > 0
                            and system_bytes > WAV_HEADER_BYTES
                        ):
                            ready = True
                            session.transition(
                                session_dir, session.SessionState.RECORDING
                            )
                            state = session.SessionState.RECORDING

                        if ready and first_error[0] is None and not stop.is_set():
                            if current - last_mic_progress >= STALL_TIMEOUT_S:
                                fail("microphone_stalled", "microphone frames stopped")
                            elif current - last_system_progress >= STALL_TIMEOUT_S:
                                fail("system_tap_stalled", "system WAV stopped growing")
                        elif (
                            not ready
                            and first_error[0] is None
                            and current - started_at >= START_TIMEOUT_S
                        ):
                            missing = "microphone" if mic_frames == 0 else "system_tap"
                            fail(
                                f"{missing}_start_timeout",
                                f"{missing} did not become ready",
                            )

                        if current >= next_manifest:
                            try:
                                _heartbeat(
                                    session_dir, mic_callback_frames, system_bytes
                                )
                            except Exception as exc:
                                fail("manifest_error", exc)
                            next_manifest = current + MANIFEST_INTERVAL_S

                        if first_error[0] is not None:
                            break
                        if stop.is_set() and q.empty():
                            break
            except Exception as exc:
                fail("microphone_stream_error", exc)

        if first_error[0] is None and not ready:
            fail("capture_not_ready", "both channels did not become ready")
        if first_error[0] is None:
            session.transition(session_dir, session.SessionState.STOPPING)
            state = session.SessionState.STOPPING
    finally:
        if writer is not None:
            try:
                writer.close()
            except Exception as exc:
                if first_error[0] is None:
                    first_error[0] = ("writer_error", exc)
        if tap is not None:
            try:
                tap.stop()
            except Exception as exc:
                if first_error[0] is None:
                    first_error[0] = ("system_tap_stop_failed", exc)

    if first_error[0] is not None:
        code, detail = first_error[0]
        terminal = (
            session.SessionState.FAILED
            if state is session.SessionState.STARTING
            and (
                code.endswith("_start_failed")
                or code.endswith("_start_timeout")
                or code == "capture_not_ready"
            )
            else session.SessionState.INTERRUPTED
        )
        session.transition(
            session_dir,
            terminal,
            error=_error(code, detail),
            channels=_failure_channels(session_dir),
        )
        return terminal

    try:
        channels = session.validate_capture(session_dir, max_duration_delta_s=2.0)
    except session.CaptureInvalid as exc:
        session.transition(
            session_dir,
            session.SessionState.INTERRUPTED,
            error=_error("capture_invalid", exc),
            channels=_failure_channels(session_dir),
        )
        return session.SessionState.INTERRUPTED
    session.transition(session_dir, session.SessionState.CAPTURED, channels=channels)
    return session.SessionState.CAPTURED


def main() -> None:
    os.umask(0o077)
    session_dir = Path(sys.argv[1])
    stop = threading.Event()
    signal.signal(signal.SIGTERM, lambda *_: stop.set())
    signal.signal(signal.SIGINT, lambda *_: stop.set())
    pid_file = session_dir / "recorder.pid"
    pid_file.write_text(str(os.getpid()))
    try:
        manifest = session.load(session_dir)
        session.write(
            session_dir, dataclasses.replace(manifest, recorder_pid=os.getpid())
        )
        record(session_dir, stop)
    finally:
        pid_file.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
