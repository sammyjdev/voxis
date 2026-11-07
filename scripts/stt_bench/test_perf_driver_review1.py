"""Review round 1 regressions for perf_driver.py and voice.py (P5-8, P5-19, P5-22).

Written before the fixes; they fail on the head 581fefc. Same rules as
test_perf_driver.py: fake engine processes, no model, GPU or recording.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import textwrap

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import perf_driver as pd  # noqa: E402
import voice_score as vs  # noqa: E402

FAKE_ENGINE = textwrap.dedent('''\
    import json, sys, time
    args = sys.argv[1:]
    out = args[args.index("--out") + 1]
    sweeps = int(args[args.index("--sweeps") + 1])
    print("whisper_backend_init_gpu: using CUDA0 backend", file=sys.stderr, flush=True)
    print(f"READY {int(time.time() * 1000)}", flush=True)
    sweep = [{"start_s": 0.0, "audio_s": 1.0, "hyp": "abre um pull request", "lang": "pt", "stt_s": 0.1}]
    json.dump({"schema": 2, "config": "fake", "backend": None, "results": sweep,
               "sweeps": [sweep] * sweeps, "load_s": 0.1, "warmup_s": 0.0}, open(out, "w"))
    ''')


# ------------------------------------------------------------ P5-8: six distinct blocks

def test_a_difference_needs_all_six_distinct_protocol_blocks():
    assert pd.claim([0.1], [0.2]) == "no consistent difference"
    assert pd.claim([0.1] * 5, [0.2] * 5) == "no consistent difference"
    assert pd.claim([0.1] * 7, [0.2] * 7) == "no consistent difference"
    assert pd.claim([0.1] * 6, [0.2] * 6, blocks=[1, 1, 2, 3, 4, 5]) == "no consistent difference"
    assert pd.claim([0.1] * 6, [0.2] * 6, blocks=[1, 2, 3, 4, 5, 6]) == "difference: first lower in all 6 blocks"


def _write_run(tmp_path, variant, block, stats):
    run_dir = tmp_path / "prompt" / f"block{block}" / variant
    run_dir.mkdir(parents=True)
    sweep = [{"hyp": "a", "start_s": 0, "audio_s": 1, "stt_s": 0.1, "lang": "pt"}]
    result = {"backend": "CUDA0", "config": "c", "results": sweep, "sweeps": [sweep]}
    (run_dir / "result.json").write_text(json.dumps(result))
    (run_dir / "run.json").write_text(json.dumps(
        {"cmd": ["x", str(run_dir / "result.json")], "exit_code": 0, "stats": stats}))
    return {"block": block, "variant": variant, "dir": str(run_dir), "exit_code": 0, "unavailable": None}


def test_a_report_from_fewer_than_six_blocks_makes_no_difference_claim(tmp_path, monkeypatch):
    runs = [_write_run(tmp_path, v, 1, {"warm_p50": p, "warm_p95": p, "rtf": p, "cold_s": p})
            for v, p in (("V1", 0.1), ("V3", 0.2))]
    (tmp_path / "manifest-prompt.json").write_text(json.dumps(
        {"protocol_sha256": "p", "source_sha": "s", "orders": [["V1", "V3"]], "sweeps": 1, "runs": runs}))
    monkeypatch.setattr(vs, "load_phrases", lambda: [["a", "a"]])

    pd.cmd_report(argparse.Namespace(out=str(tmp_path)))

    report = (tmp_path / "report-layer1.md").read_text(encoding="utf-8")
    assert "difference: first" not in report
    assert "no consistent difference" in report
    assert "smoke run" in report and "1 of 6" in report


# ------------------------------------------------------------ P5-19: unlaunchable variant

def _unlaunchable(tmp_path):
    missing = tmp_path / "no-such-engine.exe"
    return {"id": "V1", "cmd": [str(missing)], "binary": str(missing), "build": {}}


def test_a_variant_that_cannot_launch_is_recorded_as_unavailable_with_its_launch_error(tmp_path):
    rec = pd.run_variant(_unlaunchable(tmp_path), str(tmp_path / "run"), sweeps=1, extra_env={}, host={},
                         source_sha="abc", sample_interval_s=0.05)

    assert rec["exit_code"] is None
    assert rec["unavailable"].startswith("unavailable: ")
    assert "no-such-engine.exe" in rec["unavailable"]
    saved = json.load(open(tmp_path / "run" / "run.json"))
    assert saved["unavailable"] == rec["unavailable"]


def test_a_run_with_an_unlaunchable_variant_still_writes_the_manifest(tmp_path, monkeypatch):
    wav = tmp_path / "voice.wav"
    spans = tmp_path / "voice.spans.json"
    wav.write_bytes(b"a")
    spans.write_text("[]")
    (tmp_path / "perf-protocol.md").write_text("protocol")
    (tmp_path / "out").mkdir()
    (tmp_path / "out" / "prompt.txt").write_text("LARIS.", encoding="utf-8")
    (tmp_path / "variants.json").write_text(json.dumps([_unlaunchable(tmp_path)]))
    monkeypatch.setattr(pd, "check_corpus", lambda w, s: {"audio_sha256": "a", "spans_sha256": "s"})
    args = argparse.Namespace(protocol=str(tmp_path / "perf-protocol.md"), variants=str(tmp_path / "variants.json"),
                              wav=str(wav), spans=str(spans), out=str(tmp_path / "out"), table="prompt",
                              blocks=1, sweeps=1, repo=str(tmp_path))

    pd.cmd_run(args)

    manifest = json.load(open(tmp_path / "out" / "manifest-prompt.json"))
    assert manifest["runs"][0]["variant"] == "V1"
    assert manifest["runs"][0]["unavailable"].startswith("unavailable: ")
    assert manifest["runs"][0]["exit_code"] is None
    assert manifest["blocks_run"] == 1
    assert manifest["protocol_blocks"] == 6
    assert manifest["claims_allowed"] is False


# ------------------------------------------------------------ P5-22: provenance

def test_a_missing_source_sha_keeps_the_reason_it_could_not_be_read(tmp_path):
    engine = tmp_path / "engine.py"
    engine.write_text(FAKE_ENGINE)
    variant = {"id": "V1", "cmd": [sys.executable, str(engine)], "binary": str(engine), "build": {}}

    pd.run_variant(variant, str(tmp_path / "run"), sweeps=1, extra_env={}, host={}, source_sha=None,
                   source_sha_reason="git: not a repository", sample_interval_s=0.05)

    raw = json.load(open(tmp_path / "run" / "result.json"))
    assert raw["source_sha"] is None
    assert raw["source_sha_unavailable"] == "git: not a repository"


def test_git_head_of_a_folder_that_is_no_repository_returns_a_reason(tmp_path):
    sha, reason = pd.git_head_with_reason(str(tmp_path))

    assert sha is None
    assert reason


def test_a_python_variant_record_names_the_engine_package_the_model_and_the_prompt(tmp_path):
    engine = tmp_path / "engine.py"
    engine.write_text(textwrap.dedent('''\
        import json, sys, time
        args = sys.argv[1:]
        out = args[args.index("--out") + 1]
        print(f"READY {int(time.time() * 1000)}", flush=True)
        sweep = [{"start_s": 0.0, "audio_s": 1.0, "hyp": "x", "lang": "pt", "stt_s": 0.1}]
        json.dump({"schema": 2, "config": "fake", "backend": "CUDA (faster-whisper)", "results": sweep,
                   "sweeps": [sweep],
                   "engine_package": {"faster-whisper": "1.2.0", "ctranslate2": "4.5.0"},
                   "model": {"id": "large-v3-turbo", "path": "m", "sha256": "ab" * 32}}, open(out, "w"))
        '''))
    prompt = tmp_path / "prompt.txt"
    prompt.write_text("LARIS, VOXIS.", encoding="utf-8")
    variant = {"id": "V3", "kind": "python", "cmd": [sys.executable, str(engine)], "python": sys.executable,
               "binary": sys.executable, "build": {}}

    pd.run_variant(variant, str(tmp_path / "run"), sweeps=1, extra_env={"STT_PROMPT_FILE": str(prompt)}, host={},
                   source_sha="abc", sample_interval_s=0.05)

    raw = json.load(open(tmp_path / "run" / "result.json"))
    assert raw["engine_package"] == {"faster-whisper": "1.2.0", "ctranslate2": "4.5.0"}
    assert raw["model"]["sha256"] == "ab" * 32
    assert raw["prompt_sha256"] == hashlib.sha256(prompt.read_bytes()).hexdigest()
    assert raw["python"]["version"]
    assert raw["python"]["executable"] == sys.executable


def test_a_native_variant_record_carries_the_model_file_hash(tmp_path):
    engine = tmp_path / "engine.py"
    engine.write_text(FAKE_ENGINE)
    model = tmp_path / "ggml-x.bin"
    model.write_bytes(b"weights")
    variant = {"id": "V1", "kind": "native", "cmd": [sys.executable, str(engine)], "binary": str(engine),
               "model": str(model), "build": {}}

    pd.run_variant(variant, str(tmp_path / "run"), sweeps=1, extra_env={}, host={}, source_sha="abc",
                   sample_interval_s=0.05)

    raw = json.load(open(tmp_path / "run" / "result.json"))
    assert raw["model"] == {"path": str(model), "sha256": hashlib.sha256(b"weights").hexdigest()}


def test_voice_py_reports_the_python_package_versions_and_the_model_identity(tmp_path, monkeypatch):
    import numpy as np
    import soundfile as sf
    import voice

    wav = tmp_path / "v.wav"
    sf.write(str(wav), np.zeros(16000, dtype=np.float32), 16000)
    spans = tmp_path / "s.json"
    spans.write_text("[[0, 16000]]")
    out = tmp_path / "r.json"
    monkeypatch.setattr(voice, "load", lambda engine, model, opts: (lambda audio: ("x", "pt")))
    monkeypatch.setattr(voice, "vram_mib", lambda: None)
    monkeypatch.setattr(voice, "engine_packages", lambda engine: {"faster-whisper": "9.9"})
    monkeypatch.setattr(voice, "model_identity", lambda engine, model: {
        "id": model, "path": None, "path_unavailable": "not cached", "sha256": None,
        "sha256_unavailable": "not cached"})
    monkeypatch.setattr(sys, "argv", ["voice.py", "turbo-pt-b5", str(wav), str(spans), str(tmp_path),
                                      "--sweeps", "1", "--out", str(out)])

    voice.main()

    r = json.load(open(out))
    assert r["engine_package"] == {"faster-whisper": "9.9"}
    assert r["model"]["id"] == "large-v3-turbo"
    assert r["model"]["sha256"] is None
