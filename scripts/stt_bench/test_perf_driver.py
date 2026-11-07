"""Tests for perf_driver.py and voice_score.py (layer 1 harness, plan criteria 69-79, 87).

Run: <venv>/python -m pytest scripts/stt_bench -q
Only the driver's pure rules are exercised here, with fake engine processes;
no model, GPU or recording is used.
"""
from __future__ import annotations

import hashlib
import json
import os
import sys
import textwrap
import wave

import pytest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import perf_driver as pd  # noqa: E402
import voice_score as vs  # noqa: E402


# ------------------------------------------------------------ corpus (70)

def test_a_corpus_whose_audio_hash_differs_stops_with_corpus_mismatch(tmp_path):
    wav = tmp_path / "voice.wav"
    spans = tmp_path / "voice.spans.json"
    wav.write_bytes(b"not the owner's recording")
    spans.write_text("[[0, 16000]]")

    with pytest.raises(pd.CorpusMismatch) as e:
        pd.check_corpus(str(wav), str(spans))

    assert "corpus mismatch" in str(e.value)
    assert "voice.wav" in str(e.value)


def test_the_corpus_check_accepts_files_matching_the_pinned_hashes(tmp_path):
    wav = tmp_path / "a.wav"
    spans = tmp_path / "s.json"
    wav.write_bytes(b"audio")
    spans.write_bytes(b"spans")

    got = pd.check_corpus(str(wav), str(spans), audio_sha256=hashlib.sha256(b"audio").hexdigest(),
                          spans_sha256=hashlib.sha256(b"spans").hexdigest())

    assert got == {"audio_sha256": hashlib.sha256(b"audio").hexdigest(),
                   "spans_sha256": hashlib.sha256(b"spans").hexdigest()}


def test_the_pinned_corpus_hashes_are_the_ones_the_plan_names():
    assert pd.AUDIO_SHA256 == "879813cb7d985c288be6f2df6d7b0502495054aeca4e2c42a9fe9eda2ce4dd7c"
    assert pd.SPANS_SHA256 == "26e7cd855321dfe36520ea60a9ee9049b50bb42a9389027899dfb996a67fa525"


# ------------------------------------------------------------ blocks (76)

def test_six_blocks_use_every_ordering_of_three_variants_once():
    orders = pd.block_orders(["V1", "V2", "V3"])

    assert len(orders) == 6
    assert len({tuple(o) for o in orders}) == 6
    for v in ["V1", "V2", "V3"]:
        assert [o.index(v) for o in orders].count(0) == 2
        assert [o.index(v) for o in orders].count(2) == 2


# ------------------------------------------------------------ backend (46, 47)

def test_the_backend_is_read_from_the_whisper_cpp_stderr_line():
    stderr = "whisper_init_with_params_no_state: use gpu = 1\nwhisper_backend_init_gpu: using CUDA0 backend\n"

    assert pd.parse_backend(stderr) == ("CUDA0", None)


def test_a_gpu_init_failure_is_recorded_as_the_cpu_backend():
    stderr = "whisper_backend_init_gpu: no GPU found\nwhisper_model_load: CPU total size = 1623.92 MB\n"

    assert pd.parse_backend(stderr) == ("CPU", None)


def test_a_stderr_without_a_backend_line_gives_null_with_a_reason():
    value, reason = pd.parse_backend("nothing useful\n")

    assert value is None
    assert reason and "backend line" in reason


# ------------------------------------------------------------ prompt (71)

def test_the_shipped_default_prompt_is_extracted_from_dictate_rs():
    source = textwrap.dedent('''\
        const MAX_UTTERANCE_S: u32 = 30;
        /// bench.py VOCABULARY_PROMPT
        const DEFAULT_PROMPT: &str = "LARIS, VOXIS, MIRIS, tray, pull request, \\
        master, README, vault.";
        ''')

    assert pd.extract_default_prompt(source) == "LARIS, VOXIS, MIRIS, tray, pull request, master, README, vault."


# ------------------------------------------------------------ resources (75)

def test_a_per_process_gpu_counter_shown_as_na_is_unavailable_not_zero():
    csv = "1234, [N/A]\n999, 812\n"

    assert pd.parse_compute_apps(csv, 1234) == (None, "nvidia-smi reports [N/A] per process (WDDM)")
    assert pd.parse_compute_apps(csv, 999) == (812, None)


def test_a_process_missing_from_nvidia_smi_is_unavailable_with_a_reason():
    value, reason = pd.parse_compute_apps("", 42)

    assert value is None
    assert "not listed" in reason


def test_a_resource_summary_never_turns_a_missing_counter_into_zero():
    samples = [{"cpu_s": 1.0, "rss_mib": 100.0, "vram_mib": None, "vram_reason": "n/a"},
               {"cpu_s": 2.5, "rss_mib": 180.0, "vram_mib": None, "vram_reason": "n/a"}]

    out = pd.summarize_samples(samples, peak_wset_mib=None)

    assert out["cpu_s"] == 2.5
    assert out["peak_rss_mib"] == 180.0
    assert out["vram_peak_mib"] is None
    assert out["vram_peak_mib_unavailable"] == "n/a"


# ------------------------------------------------------------ stats (74)

def schema2(sweeps, ready_ms=None, spawn_ms=None):
    return {"schema": 2, "config": "x", "sweeps": [
        [{"start_s": 0, "audio_s": a, "hyp": "h", "lang": "pt", "stt_s": s} for a, s in sweep] for sweep in sweeps]}


def test_process_stats_give_warm_percentiles_over_all_sweeps_and_rtf_per_sweep():
    run = schema2([[(2.0, 0.2), (2.0, 0.4)], [(2.0, 0.3), (2.0, 0.5)]])

    s = pd.process_stats(run, spawn_ms=1000, ready_ms=4500)

    assert s["warm_p50"] == pytest.approx(0.35)
    assert s["warm_p95"] == pytest.approx(0.485)
    assert s["rtf_per_sweep"] == [pytest.approx(0.15), pytest.approx(0.2)]
    assert s["rtf"] == pytest.approx(0.175)
    assert s["cold_s"] == pytest.approx(3.5)
    assert s["sweeps"] == 2


def test_cold_time_is_unavailable_when_the_process_never_printed_ready():
    s = pd.process_stats(schema2([[(1.0, 0.1)]]), spawn_ms=1000, ready_ms=None)

    assert s["cold_s"] is None


def test_runs_are_summarized_with_count_min_median_and_max():
    assert pd.across_runs([3.0, 1.0, 2.0, None]) == {"n": 3, "min": 1.0, "median": 2.0, "max": 3.0}


# ------------------------------------------------------------ claim rule (77)

def test_a_difference_is_claimed_only_when_every_block_has_the_same_sign():
    a = [0.20, 0.21, 0.19, 0.20, 0.22, 0.20]
    b = [0.25, 0.26, 0.24, 0.27, 0.25, 0.26]

    assert pd.claim(a, b) == "difference: first lower in all 6 blocks"
    assert pd.claim(b, a) == "difference: first higher in all 6 blocks"


def test_one_block_with_the_other_sign_means_no_consistent_difference():
    a = [0.20, 0.21, 0.19, 0.20, 0.22, 0.30]
    b = [0.25, 0.26, 0.24, 0.27, 0.25, 0.26]

    assert pd.claim(a, b) == "no consistent difference"


def test_a_tie_or_a_missing_block_means_no_consistent_difference():
    assert pd.claim([0.2] * 6, [0.2] * 6) == "no consistent difference"
    assert pd.claim([0.1, 0.1, 0.1, 0.1, 0.1, None], [0.2] * 6) == "no consistent difference"


# ------------------------------------------------------------ silence (79)

def test_generated_silence_and_noise_clips_are_ten_seconds_of_16_khz_mono(tmp_path):
    clips = pd.write_control_clips(str(tmp_path))

    for name in ("silence", "noise"):
        with wave.open(clips[name]["path"]) as w:
            assert (w.getframerate(), w.getnchannels(), w.getnframes()) == (16000, 1, 160000)
        assert clips[name]["sha256"] == hashlib.sha256(open(clips[name]["path"], "rb").read()).hexdigest()
    assert clips["silence"]["sha256"] != clips["noise"]["sha256"]
    assert pd.write_control_clips(str(tmp_path / "again"))["noise"]["sha256"] == clips["noise"]["sha256"]


def test_a_control_run_reports_every_non_empty_output():
    runs = {"V1": {"silence": "", "noise": "Obrigado."}, "V3": {"silence": " ", "noise": ""}}

    assert pd.non_empty_controls(runs) == [("V1", "noise", "Obrigado.")]


# ------------------------------------------------------------ fake end-to-end run (73, 76)

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


def test_a_fake_variant_run_writes_raw_json_with_exit_code_backend_and_hashes(tmp_path):
    engine = tmp_path / "engine.py"
    engine.write_text(FAKE_ENGINE)
    variant = {"id": "V1", "cmd": [sys.executable, str(engine)], "binary": str(engine),
               "build": {"cuda_toolkit": "13.5", "cudaarchs": "89"}}

    rec = pd.run_variant(variant, str(tmp_path / "run"), sweeps=3, extra_env={}, host={"cpu": "x"},
                         source_sha="abc", sample_interval_s=0.05)

    raw = json.load(open(tmp_path / "run" / "result.json"))
    assert rec["exit_code"] == 0
    assert raw["backend"] == "CUDA0"
    assert len(raw["sweeps"]) == 3
    assert raw["host"] == {"cpu": "x"}
    assert raw["source_sha"] == "abc"
    assert raw["binary_sha256"] == hashlib.sha256(engine.read_bytes()).hexdigest()
    assert raw["build"]["cudaarchs"] == "89"
    assert raw["cold_s"] is not None and raw["cold_s"] >= 0
    assert (tmp_path / "run" / "stderr.log").read_text().startswith("whisper_backend_init_gpu")
    for k in ("cpu_s", "peak_rss_mib", "vram_peak_mib"):
        assert k in raw
        assert raw[k] is not None or raw.get(f"{k}_unavailable")


def test_a_variant_that_fails_is_recorded_with_its_exit_code_and_error(tmp_path):
    engine = tmp_path / "engine.py"
    engine.write_text("import sys\nprint('cudaErrorNoDevice', file=sys.stderr)\nsys.exit(3)\n")
    variant = {"id": "V2", "cmd": [sys.executable, str(engine)], "binary": str(engine), "build": {}}

    rec = pd.run_variant(variant, str(tmp_path / "run"), sweeps=1, extra_env={}, host={}, source_sha="abc",
                         sample_interval_s=0.05)

    assert rec["exit_code"] == 3
    assert rec["unavailable"] == "unavailable: cudaErrorNoDevice"


# ------------------------------------------------------------ voice_score (78)

def test_a_schema_1_file_and_a_schema_2_file_read_the_same_hypotheses(tmp_path):
    res = [{"start_s": 0, "audio_s": 1, "hyp": "abre um pull request", "lang": "pt", "stt_s": 0.2}]
    one = tmp_path / "one.json"
    two = tmp_path / "two.json"
    one.write_text(json.dumps({"config": "a", "vram_mib": 0, "results": res}))
    two.write_text(json.dumps({"schema": 2, "config": "b", "results": res, "sweeps": [res, res]}))

    assert vs.load_result(str(one))["sweeps"] == [res]
    assert vs.load_result(str(two))["sweeps"] == [res, res]


def test_scoring_reports_wer_cer_terms_empty_outputs_repeats_and_omissions():
    phrases = [["Abre um pull request contra a master.", "pull request|master"],
               ["Faz o commit e não dá push ainda.", "commit|push"]]
    hyps = ["Abre um pull request contra a master.", "", "faz o commit e push push push push push push"]

    s = vs.score(phrases, hyps)

    assert s["empty_outputs"] == 1
    assert s["wer"] > 0 and s["cer"] > 0
    assert s["term_recall"] == 100.0
    assert s["omitted_words"] == ["não", "dá", "ainda"]
    assert s["repeated_ngrams"] >= 1
    assert s["spans"] == 3 and s["reference_lines"] == 2
    assert "3 spans against 2 reference lines" in s["note"]


def test_identical_text_scores_zero_errors():
    phrases = [["Testa o hands-free com o Super C.", "hands free|super c"]]

    s = vs.score(phrases, ["Testa o hands-free com o Super C."])

    assert (s["wer"], s["cer"], s["empty_outputs"], s["repeated_ngrams"], s["omitted_words"]) == (0.0, 0.0, 0, 0, [])


# ------------------------------------------------------------ voice.py and bench.py (87)

def test_voice_py_writes_schema_2_with_every_sweep_and_prints_ready(tmp_path, monkeypatch, capsys):
    import numpy as np
    import soundfile as sf
    import voice

    wav = tmp_path / "v.wav"
    sf.write(str(wav), np.zeros(32000, dtype=np.float32), 16000)
    spans = tmp_path / "s.json"
    spans.write_text("[[0, 16000], [16000, 32000]]")
    out = tmp_path / "r.json"
    monkeypatch.setattr(voice, "load", lambda engine, model, opts: (lambda audio: ("abre um pull request", "pt")))
    monkeypatch.setattr(voice, "vram_mib", lambda: None)
    monkeypatch.setattr(sys, "argv", ["voice.py", "turbo-pt-b5", str(wav), str(spans), str(tmp_path),
                                      "--sweeps", "3", "--out", str(out)])

    voice.main()

    r = json.load(open(out))
    assert r["schema"] == 2 and len(r["sweeps"]) == 3 and r["results"] == r["sweeps"][0]
    assert len(r["results"]) == 2 and {"load_s", "warmup_s"} <= set(r)
    assert r["vram_mib"] is None and r["vram_mib_unavailable"]
    assert capsys.readouterr().out.startswith("READY ")


def test_the_promptfile_config_reads_its_prompt_from_stt_prompt_file(tmp_path, monkeypatch):
    import bench

    p = tmp_path / "prompt.txt"
    p.write_text("LARIS, VOXIS, MIRIS.", encoding="utf-8")
    monkeypatch.setenv("STT_PROMPT_FILE", str(p))

    assert bench.CONFIGS["turbo-pt-b5-promptfile"][2] == {"language": "pt", "beam_size": 5, "prompt": "file"}
    assert bench.prompt_text("file") == "LARIS, VOXIS, MIRIS."
    assert bench.prompt_text(None) is None


def test_bench_flags_are_split_from_the_positionals():
    import bench

    assert bench.split_flags(["a", "--sweeps", "10", "b", "--out", "x.json"]) == (["a", "b"], 10, "x.json")


# ------------------------------------------------------------ protocol first (69)

def test_a_run_without_a_protocol_file_runs_nothing(tmp_path, monkeypatch, capsys):
    import argparse

    ran = []
    monkeypatch.setattr(pd, "check_corpus", lambda wav, spans: {"audio_sha256": "a", "spans_sha256": "s"})
    monkeypatch.setattr(pd, "run_variant", lambda *a, **k: ran.append(a))
    args = argparse.Namespace(protocol=str(tmp_path / "perf-protocol.md"), variants=str(tmp_path / "variants.json"),
                              wav="voice.wav", spans="voice.spans.json", out=str(tmp_path), table="prompt",
                              blocks=6, sweeps=10, repo=str(tmp_path))

    with pytest.raises(SystemExit) as e:
        pd.cmd_run(args)

    assert e.value.code == 1
    assert "no protocol" in capsys.readouterr().err
    assert ran == []
    assert not (tmp_path / "manifest-prompt.json").exists()
