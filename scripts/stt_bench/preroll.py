"""Does LARIS clip the first word? Replays a recording through LARIS's own VAD
loop (Silero VADIterator, 512-sample chunks, 400 ms closing silence) and
transcribes each utterance as LARIS cuts it (from the chunk where the VAD
fires) and with PREROLL_MS of the audio before it. Run with laris's venv.

usage: preroll.py <wav> [preroll_ms] [out_prefix]
With out_prefix, writes <out_prefix>.laris.txt and <out_prefix>.pre.txt for
voice_score-style scoring.
"""
import sys
import wave

import numpy as np
import torch
from silero_vad import VADIterator, load_silero_vad

from laris import config
from laris.stt_faster import FasterWhisperTranscriber

CHUNK = config.CHUNK_SAMPLES

with wave.open(sys.argv[1], "rb") as w:  # pw-record: s16 mono 16 kHz
    assert (w.getframerate(), w.getnchannels()) == (16000, 1)
    audio = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float32) / 32768
preroll_chunks = round(int(sys.argv[2]) * 16 / CHUNK) if len(sys.argv) > 2 else 6

vad = VADIterator(load_silero_vad(), threshold=config.VAD_THRESHOLD,
                  sampling_rate=config.SAMPLE_RATE, min_silence_duration_ms=config.VAD_MIN_SILENCE_MS)
chunks = [audio[i:i + CHUNK] for i in range(0, len(audio) - CHUNK + 1, CHUNK)]
utterances, start = [], None
for i, chunk in enumerate(chunks):
    event = vad(torch.from_numpy(chunk), return_seconds=False)
    if event and "start" in event:
        start = i
    elif start is not None and event and "end" in event:
        utterances.append((start, i + 1))
        start = None
        vad.reset_states()

stt = FasterWhisperTranscriber()
stt.warm()
min_samples = config.MIN_SPEECH_DURATION_MS * config.SAMPLE_RATE // 1000  # as in stream_utterances
texts = {"laris": [], "pre": []}
for s, e in utterances:
    if (e - s) * CHUNK < min_samples:
        continue
    as_laris = stt.transcribe(np.concatenate(chunks[s:e]))
    padded = stt.transcribe(np.concatenate(chunks[max(0, s - preroll_chunks):e]))
    texts["laris"].append(as_laris)
    texts["pre"].append(padded)
    mark = " " if as_laris == padded else "*"
    print(f"{mark} {s * CHUNK / 16000:6.1f}s  laris: {as_laris}\n{'':10}+pre:  {padded}")
if len(sys.argv) > 3:
    for name, lines in texts.items():
        with open(f"{sys.argv[3]}.{name}.txt", "w") as f:
            f.write("\n".join(lines) + "\n")
