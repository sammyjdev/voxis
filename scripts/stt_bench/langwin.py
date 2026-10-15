"""Language detection over ~30 s of one channel's speech instead of one
utterance: 40 random windows per dataset, each built from consecutive
utterances. usage: langwin.py"""
import random

import numpy as np
from faster_whisper import WhisperModel

from bench import CONFIGS, load, sample

load(*CONFIGS["turbo-pt-b5"])  # preloads the CUDA libraries the way bench does
model = WhisperModel("large-v3-turbo", device="cuda", compute_type="float16")
for dataset in ("coraa", "ami"):
    data = sample(dataset)
    rng = random.Random(1)
    hits: dict[str, int] = {}
    for _ in range(40):
        i = rng.randrange(len(data) - 40)
        buf = []
        while sum(len(b) for b in buf) < 30 * 16000 and i < len(data):
            buf.append(data[i]["audio"])
            i += 1
        lang = model.detect_language(np.concatenate(buf)[: 30 * 16000])[0]
        hits[lang] = hits.get(lang, 0) + 1
    print(dataset, hits)
