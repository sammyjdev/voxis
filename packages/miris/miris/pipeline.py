"""Post-meeting pipeline: WAVs -> transcript -> notes -> vault note."""
import json
import os
import datetime as dt
import dataclasses
from pathlib import Path

from miris import notes as notes_mod
from miris import session
from miris import transcribe, vault
from miris.config import Config

CHANNELS = (("mic.wav", "Eu"), ("system.wav", "Eles"))
WAV_HEADER_BYTES = 44


def _write_text_atomic(path: Path, text: str) -> None:
    tmp = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    tmp.write_text(text, encoding="utf-8")
    os.replace(tmp, path)


class _BackendTranscriber:
    """MIRIS transcribes 16 kHz spans with transcribe(audio); the platform
    backend (mlx-whisper on macOS, faster-whisper on Linux) takes the rate."""

    def __init__(self, backend):
        self._backend = backend

    def warm(self) -> None:
        self._backend.warm()

    def detect_language(self, audio) -> str | None:
        """None when the user pinned STT_LANGUAGE (the backend applies it) or
        the backend cannot guess."""
        from laris import config

        detect = getattr(self._backend, "detect_language", None)
        if config.STT_LANGUAGE or detect is None:
            return None
        return detect(audio)

    def transcribe(self, audio, language: str | None = None) -> str:
        kwargs = {"language": language} if language else {}
        return self._backend.transcribe(audio, transcribe.TARGET_SR, **kwargs)


def _default_transcriber_factory():
    # meetings are bilingual (pt-BR + English): default Whisper to auto-detect
    # per utterance unless the user pinned STT_LANGUAGE themselves. Set before
    # the backend import, which reads laris.config once.
    os.environ.setdefault("STT_LANGUAGE", "")
    from laris.platform import get_backend
    return _BackendTranscriber(get_backend())


def build_transcript(session_dir: Path, transcriber_factory=None) -> str:
    """Transcribe every readable channel and merge into one timeline."""
    factory = transcriber_factory or _default_transcriber_factory
    transcriber = factory()
    if hasattr(transcriber, "warm"):
        transcriber.warm()
    # NOTE: keep this sequential. mlx-whisper is not safe to call concurrently
    # from multiple threads sharing one model - parallelizing the two channels
    # to halve latency would need one Transcriber (Metal context) per thread.
    channels = []
    for wav_name, speaker in CHANNELS:
        path = session_dir / wav_name
        if path.exists() and path.stat().st_size > WAV_HEADER_BYTES:
            try:
                channels.append(transcribe.transcribe_channel(
                    path, speaker, transcriber, transcribe.speech_spans))
            except Exception as exc:  # helper killed mid-meeting leaves a broken WAV
                print(f"[transcribe] {wav_name} unreadable ({exc}) - continuing without it")
    return transcribe.merge(*channels)


def process_session(cfg: Config, session_dir: Path, transcriber_factory=None) -> Path:
    """Process a captured session, preserving it for retry after failures."""
    manifest = session.load(session_dir)
    if manifest.state is not session.SessionState.CAPTURED:
        raise ValueError("session must be captured")
    session.transition(session_dir, session.SessionState.PROCESSING,
                       processing_pid=os.getpid())
    try:
        transcript_path = session_dir / "transcript.md"
        if (manifest.transcript_ready and transcript_path.exists()
                and transcript_path.stat().st_size > 0):
            transcript = transcript_path.read_text(encoding="utf-8")
        else:
            transcript = build_transcript(session_dir, transcriber_factory)
            _write_text_atomic(transcript_path, transcript)
            session.write(session_dir, dataclasses.replace(
                session.load(session_dir), transcript_ready=True))

        notes = notes_mod.generate(cfg, transcript)
        _write_text_atomic(
            session_dir / "notes.json",
            json.dumps(notes, ensure_ascii=False, indent=2),
        )
        note = vault.save_note(cfg, notes, transcript, session_dir.name)
        _write_text_atomic(session_dir / "note.path", str(note))
        session.transition(session_dir, session.SessionState.COMPLETED,
                           processing_pid=None, note_path=str(note), error=None)
        return note
    except Exception as exc:
        session.transition(
            session_dir,
            session.SessionState.CAPTURED,
            processing_pid=None,
            error=session.SessionError(
                "processing_failed", str(exc), dt.datetime.now().astimezone().isoformat()
            ),
        )
        raise


def main() -> None:
    # private meeting audio: the detached pipeline child inherits the CLI's
    # restrictive umask only if it sets it itself
    os.umask(0o077)
    import sys

    from miris import config

    note = process_session(config.load(), Path(sys.argv[1]))
    # the CLI that runs this process shares its stdout; the tray names the
    # new note from this line
    print(f"note: {note}")


if __name__ == "__main__":
    main()
