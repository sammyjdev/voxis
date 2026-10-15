"""Split a dictation recording into utterances the way LARIS's hands-free loop
does: Silero VADIterator over 512-sample chunks, 400 ms of silence closes an
utterance, PREROLL_MS of audio from before the VAD fired leads it, and an
utterance with under 400 ms of speech after the VAD fired is dropped.
Run with laris's venv, which has silero + torch.

usage: segment.py <wav> <spans.json>
"""
import json
import sys
import wave

import numpy as np
import torch
from silero_vad import VADIterator, load_silero_vad

from laris import config

CHUNK = config.CHUNK_SAMPLES

with wave.open(sys.argv[1], "rb") as w:  # pw-record writes s16 mono
    assert (w.getframerate(), w.getnchannels()) == (16000, 1)
    audio = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float32) / 32768

preroll = config.PREROLL_MS * config.SAMPLE_RATE // 1000 // CHUNK
min_samples = config.MIN_SPEECH_DURATION_MS * config.SAMPLE_RATE // 1000
max_chunks = int(config.MAX_SPEECH_DURATION_S * config.SAMPLE_RATE / CHUNK)
vad = VADIterator(load_silero_vad(), threshold=config.VAD_THRESHOLD,
                  sampling_rate=config.SAMPLE_RATE, min_silence_duration_ms=config.VAD_MIN_SILENCE_MS)
spans, start, lead = [], None, 0
for i in range(len(audio) // CHUNK):
    event = vad(torch.from_numpy(audio[i * CHUNK:(i + 1) * CHUNK]), return_seconds=False)
    if event and "start" in event:
        lead = min(preroll, i)
        start = i - lead
        continue
    if start is not None:
        if (event and "end" in event) or i + 1 - start >= max_chunks:
            if (i + 1 - start - lead) * CHUNK >= min_samples:
                spans.append((start * CHUNK, (i + 1) * CHUNK))
            start = None
            vad.reset_states()
json.dump(spans, open(sys.argv[2], "w"))
print(len(spans), "spans,", round(sum(e - s for s, e in spans) / 16000, 1), "s of audio")
