"""Split a dictation recording into utterances the way laris does (silero VAD,
400 ms of silence closes one). Run with laris's venv, which has silero + torch.

usage: segment.py <wav> <spans.json>
"""
import json
import sys

import wave

import numpy as np
import torch
from silero_vad import get_speech_timestamps, load_silero_vad

with wave.open(sys.argv[1], "rb") as w:  # pw-record writes s16 mono
    sr, ch = w.getframerate(), w.getnchannels()
    audio = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float32) / 32768
assert sr == 16000 and ch == 1, (sr, ch)
stamps = get_speech_timestamps(torch.from_numpy(audio), load_silero_vad(), sampling_rate=sr,
                               threshold=0.5, min_silence_duration_ms=400, min_speech_duration_ms=400)
spans = [(t["start"], t["end"]) for t in stamps]
json.dump(spans, open(sys.argv[2], "w"))
print(len(spans), "spans,", round(sum(e - s for s, e in spans) / sr, 1), "s of speech")
