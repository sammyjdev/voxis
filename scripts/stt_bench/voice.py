"""Run one config over the owner's dictation recording, span by span.

usage: voice.py <config-name> <wav> <spans.json> <out-dir>
spans.json comes from segment.py (silero, laris's 400 ms silence rule), so each
span is one dictated utterance, the unit laris transcribes.
"""
from __future__ import annotations

import json
import os
import sys
import time

import numpy as np
import soundfile as sf

from bench import CONFIGS, load, vram_mib


def main() -> None:
    name, wav, spans_path, out_dir = sys.argv[1:5]
    audio, sr = sf.read(wav, dtype="float32")
    assert sr == 16000, sr
    if audio.ndim > 1:
        audio = audio.mean(axis=1)
    spans = json.load(open(spans_path))
    engine, model, opts = CONFIGS[name]
    run = load(engine, model, opts)
    run(np.zeros(8000, dtype=np.float32))
    results = []
    for start, end in spans:
        t = time.perf_counter()
        hyp, lang = run(audio[start:end])
        results.append({"start_s": start / sr, "audio_s": (end - start) / sr, "hyp": hyp, "lang": lang,
                        "stt_s": time.perf_counter() - t})
    with open(os.path.join(out_dir, f"{name}.json"), "w") as f:
        json.dump({"config": name, "vram_mib": vram_mib(), "results": results}, f, ensure_ascii=False)


if __name__ == "__main__":
    main()
