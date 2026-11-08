"""One STT config over a fixed sample: WER/CER, latency, VRAM.

usage: bench.py <config-name> <out-dir> [coraa|ami] [--sweeps N] [--out <file.json>]
Each config runs in its own process so VRAM and load time are its own.
Datasets and local model copies live in $STT_BENCH_HOME (see README.md).
"""
from __future__ import annotations

import io
import json
import os
import random
import re
import subprocess
import sys
import time
import unicodedata

import numpy as np
import soundfile as sf

BENCH_HOME = os.path.expanduser(os.environ.get("STT_BENCH_HOME", "~/.cache/voxis-bench"))
PER_SOURCE = int(os.environ.get("PER_SOURCE", "100"))
SEED = 7
# the owner's vocabulary as LARIS's INITIAL_PROMPT would carry it
VOCABULARY_PROMPT = ("LARIS, VOXIS, MIRIS, tray, hands-free, Super C, orb, sink, PipeWire, wpctl, "
                     "diff, commit, push, pull request, master, README, BACKLOG, REST, JSON, venv, "
                     "pytest, cargo test, Codex, Claude, faster-whisper, Obsidian, vault.")
# the same words as running text: a list primes Whisper to continue the list
VOCABULARY_SENTENCE = ("Ditado técnico sobre o LARIS, o VOXIS e o MIRIS: o tray, o orb, o hands-free "
                       "com o Super C, o sink do PipeWire e o wpctl; rodar o pytest e o cargo test no "
                       "venv, fazer commit e push, abrir um pull request contra a master, revisar o "
                       "diff com o Codex ou o Claude, atualizar o README e o BACKLOG, um endpoint REST "
                       "que retorna JSON, o faster-whisper, e o vault no Obsidian.")

CONFIGS = {
    # name: (engine, model, options)
    "turbo-pt-b5": ("fw", "large-v3-turbo", {"language": "pt", "beam_size": 5}),
    "turbo-pt-b1": ("fw", "large-v3-turbo", {"language": "pt", "beam_size": 1}),
    "turbo-auto-b5": ("fw", "large-v3-turbo", {"language": None, "beam_size": 5}),
    "turbo-en-b5": ("fw", "large-v3-turbo", {"language": "en", "beam_size": 5}),
    # detection limited to the two meeting languages: the larger of p(pt), p(en)
    "turbo-pten-b5": ("fw", "large-v3-turbo", {"language": "pt|en", "beam_size": 5}),
    "turbo-pt-b5-prompt": ("fw", "large-v3-turbo", {"language": "pt", "beam_size": 5, "prompt": True}),
    # the prompt is the file named by STT_PROMPT_FILE: the perf driver passes
    # prompt.txt, the text of dictate.rs DEFAULT_PROMPT, to every engine
    "turbo-pt-b5-promptfile": ("fw", "large-v3-turbo", {"language": "pt", "beam_size": 5, "prompt": "file"}),
    "turbo-pt-b5-sentence": ("fw", "large-v3-turbo", {"language": "pt", "beam_size": 5, "prompt": "sentence"}),
    "large-v3-pt-b5": ("fw", "large-v3", {"language": "pt", "beam_size": 5}),
    # TheStage AI's turbo fine-tune (cc-by-4.0), converted with
    # ct2-transformers-converter --quantization float16 (see README)
    "thestage-pt-b5": ("fw", "models/thestage-ct2", {"language": "pt", "beam_size": 5}),
    "thestage-pt-b5-prompt": ("fw", "models/thestage-ct2", {"language": "pt", "beam_size": 5, "prompt": True}),
    "thestage-en-b5": ("fw", "models/thestage-ct2", {"language": "en", "beam_size": 5}),
    "medium-pt-b5": ("fw", "medium", {"language": "pt", "beam_size": 5}),
    "small-pt-b5": ("fw", "small", {"language": "pt", "beam_size": 5}),
    # ORT 1.31 refuses external weights behind HF cache symlinks: local copies
    "parakeet-v3": ("onnx", "nemo-parakeet-tdt-0.6b-v3", {"path": "models/parakeet-v3"}),
    "canary-v2-pt": ("onnx", "nemo-canary-1b-v2", {"path": "models/canary-v2", "language": "pt"}),
}


def sample(dataset: str) -> list[dict]:
    """coraa: 100 random utterances from each of CORAA's five corpora (pt-BR).
    ami: 300 random utterances of one AMI test shard (English meetings,
    headset mics)."""
    import pyarrow.parquet as pq  # only the CORAA/AMI samples need it, not voice.py
    rng = random.Random(SEED)
    if dataset == "ami":
        rows = pq.read_table(os.path.join(BENCH_HOME, "data", "ami-ihm-test-0.parquet")).to_pylist()
        picked = rng.sample([r for r in rows if r["text"].strip()], 3 * PER_SOURCE)
        for r in picked:
            r["dataset"] = "AMI"
    else:
        by_source: dict[str, list[dict]] = {}
        for i in range(6):
            for r in pq.read_table(os.path.join(BENCH_HOME, "data", f"test-{i}.parquet")).to_pylist():
                if r["text"].strip():
                    by_source.setdefault(r["dataset"], []).append(r)
        picked = []
        for source in sorted(by_source):
            picked += rng.sample(by_source[source], PER_SOURCE)
    out = []
    for n, r in enumerate(picked):
        audio, sr = sf.read(io.BytesIO(r["audio"]["bytes"]), dtype="float32")
        assert sr == 16000, sr
        if audio.ndim > 1:
            audio = audio.mean(axis=1)
        out.append({"id": n, "source": r["dataset"], "ref": r["text"], "audio": audio})
    return out


def split_flags(argv: list[str]) -> tuple[list[str], int, str | None]:
    """`--sweeps N` and `--out <file>` taken out, positionals kept in order."""
    pos, sweeps, out = [], 1, None
    it = iter(argv)
    for a in it:
        if a == "--sweeps":
            sweeps = int(next(it))
        elif a == "--out":
            out = next(it)
        else:
            pos.append(a)
    return pos, max(sweeps, 1), out


def prompt_text(option) -> str | None:
    """The initial prompt a config asks for; "file" reads STT_PROMPT_FILE."""
    if option == "file":
        with open(os.environ["STT_PROMPT_FILE"], encoding="utf-8") as f:
            return f.read() or None
    return {True: VOCABULARY_PROMPT, "sentence": VOCABULARY_SENTENCE}.get(option)


def ready() -> None:
    """The cold-start mark the perf driver reads, printed after the warm-up."""
    print(f"READY {int(time.time() * 1000)}", flush=True)


def vram_mib() -> int | None:
    q = subprocess.run(
        ["nvidia-smi", "--query-compute-apps=pid,used_memory", "--format=csv,noheader,nounits"],
        capture_output=True, text=True).stdout
    for line in q.splitlines():
        pid, mem = [x.strip() for x in line.split(",")]
        # Windows (WDDM) reports per-process memory as [N/A]
        if int(pid) == os.getpid() and mem.isdigit():
            return int(mem)
    return None  # unavailable is not zero


def load(engine: str, model: str, opts: dict):
    if engine == "fw":
        from faster_whisper import WhisperModel
        import nvidia.cublas, nvidia.cudnn  # noqa: E401 - same preload as stt_faster
        import ctypes
        if sys.platform == "win32":
            # the Windows wheels ship DLLs in bin/, found through the DLL search path
            for pkg in (nvidia.cublas, nvidia.cudnn):
                bin_dir = os.path.join(list(pkg.__path__)[0], "bin")
                os.add_dll_directory(bin_dir)
                os.environ["PATH"] = bin_dir + os.pathsep + os.environ["PATH"]
        else:
            for pkg, names in ((nvidia.cublas, ("libcublasLt.so.12", "libcublas.so.12")),
                               (nvidia.cudnn, ("libcudnn.so.9",))):
                for name in names:
                    ctypes.CDLL(os.path.join(list(pkg.__path__)[0], "lib", name), mode=ctypes.RTLD_GLOBAL)
        local = os.path.join(BENCH_HOME, model)
        m = WhisperModel(local if os.path.isdir(local) else model, device="cuda", compute_type="float16")

        def run(audio):
            language = opts["language"]
            if language == "pt|en":
                probs = dict(m.detect_language(audio)[2])
                language = "pt" if probs.get("pt", 0) >= probs.get("en", 0) else "en"
            prompt = prompt_text(opts.get("prompt"))
            extra = {"initial_prompt": prompt} if prompt else {}
            segs, info = m.transcribe(audio, language=language, beam_size=opts["beam_size"],
                                      task="transcribe", condition_on_previous_text=False, **extra)
            return "".join(s.text for s in segs).strip(), info.language
        return run
    import onnxruntime as ort
    ort.preload_dlls()
    import onnx_asr
    m = onnx_asr.load_model(model, os.path.join(BENCH_HOME, opts["path"]), providers=["CUDAExecutionProvider"])

    def run(audio):
        kw = {"language": opts["language"]} if "language" in opts else {}
        return m.recognize(audio, sample_rate=16000, **kw).strip(), opts.get("language", "-")
    return run


def main() -> None:
    args, sweeps, out_file = split_flags(sys.argv[1:])
    name, out_dir = args[0], args[1]
    dataset = args[2] if len(args) > 2 else "coraa"
    engine, model, opts = CONFIGS[name]
    data = sample(dataset)
    t0 = time.perf_counter()
    run = load(engine, model, opts)
    load_s = time.perf_counter() - t0
    t0 = time.perf_counter()
    run(np.zeros(8000, dtype=np.float32))  # warm, like laris does at boot
    warmup_s = time.perf_counter() - t0
    ready()
    all_sweeps = []
    for _ in range(sweeps):
        results = []
        for item in data:
            t = time.perf_counter()
            hyp, lang = run(item["audio"])
            results.append({"id": item["id"], "source": item["source"], "ref": item["ref"], "hyp": hyp,
                            "lang": lang, "audio_s": len(item["audio"]) / 16000,
                            "stt_s": time.perf_counter() - t})
        all_sweeps.append(results)
    vram = vram_mib()
    meta = {"config": name, "engine": engine, "model": model, "opts": opts,
            "load_s": load_s, "warmup_s": warmup_s, "vram_mib": vram}
    if vram is None:
        meta["vram_mib_unavailable"] = "nvidia-smi has no per-process value for this pid"
    with open(out_file or os.path.join(out_dir, f"{name}.json"), "w") as f:
        json.dump({"schema": 2, "meta": meta, "results": all_sweeps[0], "sweeps": all_sweeps}, f, ensure_ascii=False)
    print(json.dumps(meta))


if __name__ == "__main__":
    main()
