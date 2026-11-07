"""Run one config over the owner's dictation recording, span by span.

usage: voice.py <config-name> <wav> <spans.json> <out-dir> [--sweeps N] [--out <file.json>]
spans.json comes from segment.py (silero, laris's 400 ms silence rule), so each
span is one dictated utterance, the unit laris transcribes.
Config turbo-pt-b5-promptfile reads its prompt from STT_PROMPT_FILE.

Result schema 2, the same as stt-bench's: `results` is the first sweep,
`sweeps` every sweep, `load_s` the model load, `warmup_s` the warm-up pass,
and a `READY <unix_ms>` stdout line after the warm-up. Resource and host
fields are null with a reason: perf_driver.py samples them from outside.
"""
from __future__ import annotations

import json
import os
import sys
import time

import numpy as np
import soundfile as sf

from bench import CONFIGS, load, ready, split_flags, vram_mib

BY_DRIVER = "sampled by scripts/stt_bench/perf_driver.py"


def main() -> None:
    args, sweeps, out_file = split_flags(sys.argv[1:])
    name, wav, spans_path, out_dir = args[:4]
    audio, sr = sf.read(wav, dtype="float32")
    assert sr == 16000, sr
    if audio.ndim > 1:
        audio = audio.mean(axis=1)
    spans = json.load(open(spans_path))
    engine, model, opts = CONFIGS[name]
    t0 = time.perf_counter()
    run = load(engine, model, opts)
    load_s = time.perf_counter() - t0
    t0 = time.perf_counter()
    run(np.zeros(8000, dtype=np.float32))
    warmup_s = time.perf_counter() - t0
    ready()
    all_sweeps = []
    for _ in range(sweeps):
        results = []
        for start, end in spans:
            t = time.perf_counter()
            hyp, lang = run(audio[start:end])
            results.append({"start_s": start / sr, "audio_s": (end - start) / sr, "hyp": hyp, "lang": lang,
                            "stt_s": time.perf_counter() - t})
        all_sweeps.append(results)
    vram = vram_mib()
    out = {"schema": 2, "config": name,
           "backend": "CUDA (faster-whisper, CTranslate2 float16)" if engine == "fw" else "CUDA (onnxruntime)",
           "host": None, "host_unavailable": "recorded by scripts/stt_bench/perf_driver.py",
           "vram_mib": vram, "cpu_s": None, "cpu_s_unavailable": BY_DRIVER,
           "peak_rss_mib": None, "peak_rss_mib_unavailable": BY_DRIVER,
           "vram_peak_mib": None, "vram_peak_mib_unavailable": BY_DRIVER,
           "load_s": load_s, "warmup_s": warmup_s, "results": all_sweeps[0], "sweeps": all_sweeps}
    if vram is None:
        out["vram_mib_unavailable"] = "nvidia-smi has no per-process value for this pid"
    with open(out_file or os.path.join(out_dir, f"{name}.json"), "w") as f:
        json.dump(out, f, ensure_ascii=False)


if __name__ == "__main__":
    main()
