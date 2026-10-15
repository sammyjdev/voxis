import dataclasses
import json
import os
import wave

import numpy as np
import pytest

from miris.config import Config
from miris import notes as notes_mod
from miris import pipeline, session


def _cfg(tmp_path, llm_body: str) -> Config:
    script = tmp_path / "llm.sh"
    script.write_text(f"#!/bin/sh\n{llm_body}\n")
    script.chmod(0o755)
    return Config(vault_root=tmp_path / "vault", inbox="inbox",
                  llm_command=[str(script)],
                  categories={"trabalho": "work"},
                  sessions_dir=tmp_path / "sessions")


def _session_with_mic(tmp_path):
    session = tmp_path / "sessions" / "20260716-1000"
    session.mkdir(parents=True)
    with wave.open(str(session / "mic.wav"), "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(16000)
        w.writeframes(np.zeros(16000, dtype=np.int16).tobytes())
    return session


def write_system_wav(path):
    with wave.open(str(path), "wb") as wav:
        wav.setnchannels(2)
        wav.setsampwidth(2)
        wav.setframerate(48_000)
        wav.writeframes(np.zeros(48_000 * 2, dtype=np.int16).tobytes())


def pipeline_session(tmp_path, state, llm_body="exit 1"):
    cfg = _cfg(tmp_path, llm_body)
    session_dir = _session_with_mic(tmp_path)
    write_system_wav(session_dir / "system.wav")
    session.create(session_dir)
    session.transition(session_dir, session.SessionState.RECORDING,
                       recorder_pid=os.getpid())
    if state is session.SessionState.INTERRUPTED:
        session.transition(session_dir, state, recorder_pid=None)
    else:
        session.transition(session_dir, session.SessionState.STOPPING)
        session.transition(session_dir, session.SessionState.CAPTURED,
                           recorder_pid=None)
    return cfg, session_dir


class FakeTranscriber:
    def warm(self):
        pass

    def transcribe(self, audio):
        return "fala"


def test_process_session_happy_path(tmp_path, monkeypatch):
    good = json.dumps({"categoria": "trabalho", "titulo": "Sync",
                       "resumo": "R", "action_items": []})
    cfg, session_dir = pipeline_session(tmp_path, session.SessionState.CAPTURED,
                                        f"echo '{good}'")
    monkeypatch.setattr("miris.transcribe.speech_spans",
                        lambda a, sr=16000: [(0, 16000)])
    note = pipeline.process_session(cfg, session_dir, transcriber_factory=FakeTranscriber)
    assert note.parent == tmp_path / "vault/work"
    assert "fala" in note.read_text()
    assert (session_dir / "transcript.md").exists()
    assert (session_dir / "notes.json").exists()


def test_process_session_records_note_path(tmp_path, monkeypatch):
    good = json.dumps({"categoria": "trabalho", "titulo": "Sync",
                       "resumo": "R", "action_items": []})
    cfg, session_dir = pipeline_session(tmp_path, session.SessionState.CAPTURED,
                                        f"echo '{good}'")
    monkeypatch.setattr("miris.transcribe.speech_spans",
                        lambda a, sr=16000: [(0, 16000)])
    note = pipeline.process_session(cfg, session_dir, transcriber_factory=FakeTranscriber)
    assert (session_dir / "note.path").read_text(encoding="utf-8") == str(note)


def test_process_session_llm_failure_preserves_capture_for_retry(tmp_path, monkeypatch):
    cfg = _cfg(tmp_path, "exit 1")
    _, session_dir = pipeline_session(tmp_path, session.SessionState.CAPTURED)
    monkeypatch.setattr("miris.transcribe.speech_spans",
                        lambda a, sr=16000: [(0, 16000)])
    with pytest.raises(notes_mod.NotesError):
        pipeline.process_session(cfg, session_dir, transcriber_factory=FakeTranscriber)
    manifest = session.load(session_dir)
    assert manifest.state is session.SessionState.CAPTURED
    assert manifest.error.code == "processing_failed"
    assert (session_dir / "transcript.md").exists()
    assert not (session_dir / "note.path").exists()


def test_process_session_reuses_cached_transcript(tmp_path):
    good = json.dumps({"categoria": "trabalho", "titulo": "Sync",
                       "resumo": "R", "action_items": []})
    cfg, session_dir = pipeline_session(tmp_path, session.SessionState.CAPTURED,
                                        f"echo '{good}'")
    (session_dir / "transcript.md").write_text("[00:00] Eu: cached")
    session.write(session_dir, dataclasses.replace(
        session.load(session_dir), transcript_ready=True))

    def boom():
        raise AssertionError("must not transcribe when cached")

    note = pipeline.process_session(cfg, session_dir, transcriber_factory=boom)
    assert "cached" in note.read_text()


def test_process_session_requires_captured(tmp_path):
    cfg, session_dir = pipeline_session(tmp_path, session.SessionState.INTERRUPTED)
    with pytest.raises(ValueError, match="captured"):
        pipeline.process_session(cfg, session_dir, transcriber_factory=FakeTranscriber)


def test_processing_failure_returns_to_captured(tmp_path, monkeypatch):
    cfg, session_dir = pipeline_session(tmp_path, session.SessionState.CAPTURED)
    monkeypatch.setattr(pipeline, "build_transcript",
                        lambda *args: (_ for _ in ()).throw(RuntimeError("boom")))
    with pytest.raises(RuntimeError, match="boom"):
        pipeline.process_session(cfg, session_dir)
    manifest = session.load(session_dir)
    assert manifest.state is session.SessionState.CAPTURED
    assert manifest.error.code == "processing_failed"


def test_empty_transcript_cache_is_rebuilt(tmp_path, monkeypatch):
    good = json.dumps({"categoria": "trabalho", "titulo": "Sync",
                       "resumo": "R", "action_items": []})
    cfg, session_dir = pipeline_session(tmp_path, session.SessionState.CAPTURED,
                                        f"echo '{good}'")
    (session_dir / "transcript.md").write_text("", encoding="utf-8")
    monkeypatch.setattr(pipeline, "build_transcript", lambda *args: "rebuilt")
    pipeline.process_session(cfg, session_dir, transcriber_factory=FakeTranscriber)
    assert (session_dir / "transcript.md").read_text(encoding="utf-8") == "rebuilt"


def _transcribe_one_second_of_mic(tmp_path, monkeypatch):
    """build_transcript with its default transcriber over a 1 s mic.wav whose
    VAD finds one span covering the whole second."""
    monkeypatch.setenv("STT_LANGUAGE", "placeholder")  # restored after the test
    monkeypatch.delenv("STT_LANGUAGE")
    monkeypatch.setattr(pipeline.transcribe, "speech_spans", lambda audio: [(0, len(audio))])
    session_dir = _session_with_mic(tmp_path)
    return pipeline.build_transcript(session_dir)


def test_on_linux_meetings_are_transcribed_by_faster_whisper_at_16khz(tmp_path, monkeypatch):
    import sys

    calls = []

    class FakeFasterWhisper:
        def warm(self):
            calls.append("warm")

        def transcribe(self, audio, sample_rate):
            calls.append((len(audio), sample_rate))
            return "reunião no linux"

    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setitem(sys.modules, "mlx_whisper", None)
    monkeypatch.setitem(sys.modules, "laris.transcriber", None)
    monkeypatch.setattr("laris.stt_faster.FasterWhisperTranscriber", FakeFasterWhisper)

    transcript = _transcribe_one_second_of_mic(tmp_path, monkeypatch)

    assert transcript == "[00:00] Eu: reunião no linux"
    assert calls == ["warm", (16000, 16000)]


LANGUAGE_PROBE = """
import os, sys, types
sys.platform = "linux"
os.environ.pop("STT_LANGUAGE", None)


class LanguageProbe:
    def warm(self):
        pass

    def transcribe(self, audio, sample_rate):
        from laris import config  # what faster-whisper is handed at transcribe time
        return f"lang={config.STT_LANGUAGE!r}"


sys.modules["laris.stt_faster"] = types.SimpleNamespace(FasterWhisperTranscriber=LanguageProbe)
from miris import pipeline
pipeline.transcribe.speech_spans = lambda audio: [(0, len(audio))]
print(pipeline.build_transcript(__import__("pathlib").Path(sys.argv[1])))
"""


def test_on_linux_meetings_reach_faster_whisper_with_language_auto_detect(tmp_path):
    """Meetings mix pt-BR and English. laris.config reads STT_LANGUAGE once,
    at its first import, so the "" default must be set before that import;
    a fresh process is the only place that ordering is real."""
    import subprocess
    import sys

    env = {k: v for k, v in os.environ.items() if k != "STT_LANGUAGE"}
    session_dir = _session_with_mic(tmp_path)

    out = subprocess.run([sys.executable, "-c", LANGUAGE_PROBE, str(session_dir)],
                         capture_output=True, text=True, env=env, check=True)

    assert out.stdout.strip() == "[00:00] Eu: lang=''"


def test_on_macos_meetings_are_still_transcribed_by_mlx_whisper(tmp_path, monkeypatch):
    import sys
    import types

    calls = []

    class FakeMlxTranscriber:
        def warm(self):
            calls.append("warm")

        def transcribe(self, audio, sample_rate=16000):
            calls.append((len(audio), sample_rate))
            return "reunião no mac"

    monkeypatch.setattr(sys, "platform", "darwin")
    monkeypatch.setitem(sys.modules, "laris.transcriber",
                        types.SimpleNamespace(Transcriber=FakeMlxTranscriber))

    transcript = _transcribe_one_second_of_mic(tmp_path, monkeypatch)

    assert transcript == "[00:00] Eu: reunião no mac"
    assert calls == ["warm", (16000, 16000)]
