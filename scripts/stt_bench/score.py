"""Score every <out-dir>/*.json from bench.py into one markdown table.

CORAA references are lowercase, unpunctuated, numbers spelled out, and keep
disfluencies. "strict" compares after normalising case, punctuation and
numbers; "lenient" also drops filled pauses and immediate word repeats, which
Whisper leaves out on purpose and a dictation user does not want typed.
"""
from __future__ import annotations

import glob
import json
import re
import statistics
import sys
import unicodedata

import jiwer
from num2words import num2words

FILLERS = {"ah", "ahn", "eh", "éh", "hum", "hm", "hmm", "uhum", "uh", "ã", "ãh", "ô", "né", "aham"}
EN_FILLERS = {"uh", "um", "mm", "hmm", "mhm", "huh", "ah", "eh", "er", "erm", "oh"}  # "um" is an article in pt
LANG = "pt_BR"  # num2words language; score.py <dir> en switches to English


def _num(match: re.Match) -> str:
    raw = match.group(0)
    try:
        if "," in raw:
            whole, frac = raw.replace(".", "").split(",", 1)
            return f"{num2words(int(whole), lang=LANG)} vírgula {num2words(int(frac), lang=LANG)}"
        return num2words(int(raw.replace(".", "")), lang=LANG)
    except (ValueError, NotImplementedError):
        return raw


def normalize(text: str, lenient: bool = False) -> str:
    text = unicodedata.normalize("NFC", text).lower()
    text = re.sub(r"(\d+)\s*%", r"\1 por cento", text)
    text = re.sub(r"(\d+)[ºo°]\b", lambda m: num2words(int(m.group(1)), lang=LANG, to="ordinal"), text)
    text = re.sub(r"(\d+)ª", lambda m: num2words(int(m.group(1)), lang=LANG, to="ordinal")[:-1] + "a", text)
    text = re.sub(r"\d{1,3}(?:\.\d{3})+(?:,\d+)?|\d+,\d+|\d+", _num, text)
    text = re.sub(r"[^\w\s]|_", " ", text)
    words = text.split()
    if lenient:
        fillers = EN_FILLERS if LANG == "en" else FILLERS
        words = [w for w in words if w not in fillers]
        words = [w for i, w in enumerate(words) if i == 0 or w != words[i - 1]]
    return " ".join(words)


def pct(xs: list[float], p: float) -> float:
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(round(p * (len(xs) - 1))))]


def wer(pairs: list[tuple[str, str]], lenient: bool = False) -> float:
    refs = [normalize(r, lenient) or "_" for r, _ in pairs]
    hyps = [normalize(h, lenient) for _, h in pairs]
    return 100 * jiwer.wer(refs, hyps)


def cer(pairs: list[tuple[str, str]]) -> float:
    return 100 * jiwer.cer([normalize(r) or "_" for r, _ in pairs], [normalize(h) for _, h in pairs])


def main() -> None:
    global LANG
    out_dir = sys.argv[1]
    if len(sys.argv) > 2 and sys.argv[2] == "en":
        LANG = "en"
    rows = []
    sources: list[str] = []
    for path in sorted(glob.glob(f"{out_dir}/*.json")):
        data = json.load(open(path))
        meta, res = data["meta"], data["results"]
        pairs = [(r["ref"], r["hyp"]) for r in res]
        short = [(r["ref"], r["hyp"]) for r in res if len(r["ref"].split()) <= 5]
        long = [(r["ref"], r["hyp"]) for r in res if len(r["ref"].split()) >= 11]
        dictation = [r["stt_s"] for r in res if 2 <= r["audio_s"] <= 8]
        by_source = {}
        for s in sorted({r["source"] for r in res}):
            by_source[s] = wer([(r["ref"], r["hyp"]) for r in res if r["source"] == s], lenient=True)
        sources = list(by_source)
        expected = "en" if LANG == "en" else "pt"
        not_pt = sum(1 for r in res if r["lang"] not in (expected, "-"))
        rows.append({
            "config": meta["config"],
            "wer": wer(pairs), "wer_len": wer(pairs, lenient=True), "cer": cer(pairs),
            "wer_short": wer(short, lenient=True), "wer_long": wer(long, lenient=True),
            "p50": pct(dictation, .5), "p90": pct(dictation, .9),
            "rtf": sum(r["stt_s"] for r in res) / sum(r["audio_s"] for r in res),
            "load": meta["load_s"], "vram": meta["vram_mib"], "not_pt": not_pt,
            "n": len(res), "by_source": by_source,
            "empty": sum(1 for r in res if not r["hyp"].strip()),
        })
    rows.sort(key=lambda r: r["wer_len"])
    n = rows[0]["n"] if rows else 0
    print(f"{n} utterances. WER/CER in %, lower is better. Latency on 2-8 s utterances, GPU warm.\n")
    print("| config | WER lenient | WER strict | CER | WER 1-5 words | WER 11+ words | p50 s | p90 s | RTF | load s | VRAM MiB | wrong lang | empty |")
    print("|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for r in rows:
        print(f"| {r['config']} | {r['wer_len']:.1f} | {r['wer']:.1f} | {r['cer']:.1f} | {r['wer_short']:.1f} | "
              f"{r['wer_long']:.1f} | {r['p50']:.3f} | {r['p90']:.3f} | {r['rtf']:.4f} | {r['load']:.1f} | "
              f"{r['vram']} | {r['not_pt']} | {r['empty']} |")
    print("\nWER lenient by source:\n")
    print("| config | " + " | ".join(sources) + " |")
    print("|---|" + "---|" * len(sources))
    for r in rows:
        print(f"| {r['config']} | " + " | ".join(f"{r['by_source'][s]:.1f}" for s in sources) + " |")


if __name__ == "__main__":
    main()
