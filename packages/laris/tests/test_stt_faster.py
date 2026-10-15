"""faster-whisper transcriber, the Linux STT: device choice and decoder
options, with the model mocked (the real one is covered by the manual
end-to-end run in the Linux plan)."""
from unittest.mock import MagicMock

import numpy as np


def _build(monkeypatch, cuda: bool):
    import laris.stt_faster as mod

    captured = {}
    model = MagicMock()
    model.transcribe.return_value = ([MagicMock(text=" ok ")], None)

    def fake_model(name, device, compute_type, **kw):
        captured.update(name=name, device=device, compute_type=compute_type)
        return model

    monkeypatch.setattr(mod, "_cuda_available", lambda: cuda)
    monkeypatch.setattr(mod, "WhisperModel", fake_model)
    return mod.FasterWhisperTranscriber(), captured, model


def test_with_a_cuda_gpu_the_model_runs_on_cuda_in_float16(monkeypatch):
    _t, captured, _m = _build(monkeypatch, cuda=True)

    assert (captured["device"], captured["compute_type"]) == ("cuda", "float16")


def test_without_a_cuda_gpu_the_model_runs_on_cpu_in_int8(monkeypatch):
    _t, captured, _m = _build(monkeypatch, cuda=False)

    assert (captured["device"], captured["compute_type"]) == ("cpu", "int8")


def _device_chosen_for(monkeypatch, gpus: int, libraries_load: bool):
    import laris.stt_faster as mod

    captured = {}
    monkeypatch.setattr("ctranslate2.get_cuda_device_count", lambda: gpus)
    monkeypatch.setattr(mod, "_load_cuda_libraries", lambda: libraries_load)
    monkeypatch.setattr(mod, "WhisperModel",
                        lambda name, device, compute_type, **kw: captured.update(
                            device=device, compute_type=compute_type))
    mod.FasterWhisperTranscriber()
    return captured["device"], captured["compute_type"]


def test_a_gpu_whose_cuda_libraries_cannot_load_falls_back_to_cpu(monkeypatch):
    """CTranslate2 loads cuBLAS/cuDNN only on the first inference. Choosing
    CUDA off the device count alone crashed warm() with "libcublas.so.12 is
    not found" and the daemon never became ready (seen 2026-10-14)."""
    assert _device_chosen_for(monkeypatch, gpus=1, libraries_load=False) == ("cpu", "int8")


def test_a_gpu_with_loadable_cuda_libraries_runs_on_cuda(monkeypatch):
    assert _device_chosen_for(monkeypatch, gpus=1, libraries_load=True) == ("cuda", "float16")


def test_without_a_gpu_the_cuda_libraries_are_never_loaded(monkeypatch):
    import laris.stt_faster as mod

    def _must_not_load():
        raise AssertionError("loaded CUDA libraries with no GPU present")

    monkeypatch.setattr(mod, "_load_cuda_libraries", _must_not_load)
    monkeypatch.setattr("ctranslate2.get_cuda_device_count", lambda: 0)
    monkeypatch.setattr(mod, "WhisperModel", lambda *a, **k: None)

    mod.FasterWhisperTranscriber()


def test_initial_prompt_is_forwarded_when_configured(monkeypatch):
    from laris import config

    monkeypatch.setattr(config, "INITIAL_PROMPT", "VOXIS, LARIS")
    t, _c, model = _build(monkeypatch, cuda=False)

    t.transcribe(np.zeros(16000, dtype=np.float32), 16000)

    assert model.transcribe.call_args.kwargs["initial_prompt"] == "VOXIS, LARIS"


def test_no_initial_prompt_is_sent_when_unset(monkeypatch):
    """An empty prompt still primes the decoder and can leak into the text."""
    from laris import config

    monkeypatch.setattr(config, "INITIAL_PROMPT", "")
    t, _c, model = _build(monkeypatch, cuda=False)

    t.transcribe(np.zeros(16000, dtype=np.float32), 16000)

    assert "initial_prompt" not in model.transcribe.call_args.kwargs


def test_segments_are_joined_and_trimmed_into_one_utterance(monkeypatch):
    t, _c, model = _build(monkeypatch, cuda=False)
    model.transcribe.return_value = ([MagicMock(text=" olá,"), MagicMock(text=" mundo ")], None)

    assert t.transcribe(np.zeros(16000, dtype=np.float32), 16000) == "olá, mundo"


def test_empty_audio_transcribes_to_nothing_without_calling_the_model(monkeypatch):
    t, _c, model = _build(monkeypatch, cuda=False)

    assert t.transcribe(np.zeros(0, dtype=np.float32), 16000) == ""
    model.transcribe.assert_not_called()
