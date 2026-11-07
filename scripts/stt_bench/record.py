"""Record the owner's voice-phrases.tsv in one take, where pw-record is missing
(Windows). Shows one phrase at a time; read it, then press Enter for the next.
Writes s16 mono 16 kHz, the format segment.py expects.

usage: record.py <out.wav> [mic-name-substring]
The optional name picks the microphone (e.g. "fifine"); default is the system
default input. The MME entry is used: Windows resamples it to 16 kHz.
"""
import os
import sys
import threading
import wave

import numpy as np
import sounddevice as sd

RATE = 16000


def main() -> None:
    out = sys.argv[1]
    here = os.path.dirname(os.path.abspath(__file__))
    phrases = [line.split("\t")[0] for line in open(os.path.join(here, "voice-phrases.tsv"), encoding="utf-8")
               if line.strip()]
    chunks: list[np.ndarray] = []
    lock = threading.Lock()

    def callback(indata, frames, time, status):  # noqa: ARG001
        with lock:
            chunks.append(indata.copy())

    device = None
    if len(sys.argv) > 2:
        want = sys.argv[2].lower()
        apis = sd.query_hostapis()
        matches = [i for i, d in enumerate(sd.query_devices())
                   if d["max_input_channels"] > 0 and want in d["name"].lower()
                   and apis[d["hostapi"]]["name"] == "MME"]
        if not matches:
            raise SystemExit(f"nenhum microfone com '{sys.argv[2]}' no nome")
        device = matches[0]
    print(f"Microfone: {sd.query_devices(device, kind='input')['name']}")
    print("Leia cada frase em voz normal, como num ditado. Enter passa para a proxima.\n")
    input("Enter para comecar a gravar...")
    with sd.InputStream(samplerate=RATE, channels=1, dtype="int16", device=device, callback=callback):
        for i, phrase in enumerate(phrases, 1):
            input(f"[{i:>2}/{len(phrases)}] {phrase}\n        (leia, pause, Enter) ")
    with lock:
        audio = np.concatenate(chunks)
    with wave.open(out, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(audio.tobytes())
    print(f"\n{len(audio) / RATE:.1f} s gravados em {out}")


if __name__ == "__main__":
    main()
