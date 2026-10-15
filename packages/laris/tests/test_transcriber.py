from unittest.mock import MagicMock

import numpy as np
import pytest


def _transcriber(monkeypatch, prompt):
    """Build a Transcriber with mlx_whisper stubbed out (importing the real
    one costs 1.6s and requires Apple Silicon)."""
    from laris import config, transcriber

    fake_mlx = MagicMock()
    fake_mlx.transcribe.return_value = {"text": " ok "}
    monkeypatch.setattr(config, "INITIAL_PROMPT", prompt)

    t = transcriber.Transcriber.__new__(transcriber.Transcriber)
    t._mlx_whisper = fake_mlx
    t.model = "test-model"
    return t, fake_mlx


def test_initial_prompt_is_forwarded_when_configured(monkeypatch):
    t, fake_mlx = _transcriber(monkeypatch, "PortAudio, worktree, AXON")

    t.transcribe(np.zeros(16000, dtype=np.float32))

    assert fake_mlx.transcribe.call_args.kwargs["initial_prompt"] == (
        "PortAudio, worktree, AXON"
    )


def test_no_initial_prompt_is_sent_when_unset(monkeypatch):
    """Passing an empty string is not the same as passing nothing: Whisper
    would prime on it and can echo it into the transcript."""
    t, fake_mlx = _transcriber(monkeypatch, "")

    t.transcribe(np.zeros(16000, dtype=np.float32))

    assert "initial_prompt" not in fake_mlx.transcribe.call_args.kwargs


def test_detect_language_names_the_language_mlx_whisper_hears(monkeypatch):
    t, fake_mlx = _transcriber(monkeypatch, "")
    fake_mlx.transcribe.return_value = {"text": " hello ", "language": "en"}

    assert t.detect_language(np.zeros(16000, dtype=np.float32)) == "en"
    assert fake_mlx.transcribe.call_args.kwargs["language"] is None


def test_a_language_given_per_call_wins_over_the_configured_one(monkeypatch):
    from laris import config

    monkeypatch.setattr(config, "STT_LANGUAGE", "")
    t, fake_mlx = _transcriber(monkeypatch, "")

    t.transcribe(np.zeros(16000, dtype=np.float32), language="pt")

    assert fake_mlx.transcribe.call_args.kwargs["language"] == "pt"
