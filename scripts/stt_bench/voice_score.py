"""Score voice.py / stt-bench output against voice-phrases.tsv.

WER and CER over the whole read text (span boundaries do not need to match
phrase boundaries), term recall (the share of technical terms that come out
recognisable), empty outputs, repeated 3-grams and the reference words the
hypothesis omits, after the same normalisation as score.py. Reads result
schema 1 (one pass) and schema 2 (per-sweep arrays); quality is scored on the
first sweep, latency (p50, p95) over every sweep.

usage: voice_score.py <out-dir>
"""
from __future__ import annotations

import glob
import json
import os
import sys
from collections import Counter

import jiwer

from score import normalize, pct

HERE = os.path.dirname(os.path.abspath(__file__))


def load_phrases(path: str = os.path.join(HERE, "voice-phrases.tsv")) -> list[list[str]]:
    with open(path, encoding="utf-8") as f:
        return [line.rstrip("\n").split("\t") for line in f if line.strip()]


def load_result(path: str) -> dict:
    """A result file as {config, sweeps}; schema 1 holds a single sweep."""
    with open(path, encoding="utf-8") as f:
        d = json.load(f)
    sweeps = d.get("sweeps") if d.get("schema") == 2 and d.get("sweeps") else [d["results"]]
    return {"config": d["config"], "sweeps": sweeps, "raw": d}


def _ngrams(words: list[str], n: int = 3) -> Counter:
    return Counter(tuple(words[i:i + n]) for i in range(len(words) - n + 1))


def score(phrases: list[list[str]], hyps: list[str]) -> dict:
    """One normalisation and one reference for every variant: the phrases'
    text joined, against the spans' hypotheses joined."""
    ref = normalize(" ".join(p[0] for p in phrases), lenient=True)
    raw = " ".join(hyps)
    hyp_strict = normalize(raw)
    hyp = normalize(raw, lenient=True)
    terms = [t for p in phrases for t in p[1].split("|")]
    # whole words: "resting" is not "rest", "venvidularis" is not "venv"
    missed = [t for t in terms if f" {normalize(t)} " not in f" {hyp} "]
    out = jiwer.process_words(ref, hyp or "_")
    omitted: list[str] = []
    for chunk in out.alignments[0]:
        if chunk.type == "delete":
            omitted += out.references[0][chunk.ref_start_idx:chunk.ref_end_idx]
    ref_counts = _ngrams(ref.split())
    # a 3-gram the hypothesis repeats more often than the reference does: the
    # looping failure ("push push push"); counted before the lenient pass,
    # which folds immediate word repeats
    repeated = sum(1 for g, c in _ngrams(hyp_strict.split()).items() if c > 1 and c > ref_counts.get(g, 0))
    note = ""
    if len(hyps) != len(phrases):
        note = (f"{len(hyps)} spans against {len(phrases)} reference lines: "
                "scored as concatenated text, nothing dropped")
    return {
        "wer": 100 * out.wer,
        "cer": 100 * jiwer.cer(ref, hyp or "_"),
        "term_recall": 100 * (len(terms) - len(missed)) / len(terms) if terms else 0.0,
        "missed_terms": missed,
        "empty_outputs": sum(1 for h in hyps if not h.strip()),
        "repeated_ngrams": repeated,
        "omitted_words": omitted,
        "spans": len(hyps),
        "reference_lines": len(phrases),
        "note": note,
        "text": raw,
    }


def score_file(path: str, phrases: list[list[str]]) -> dict:
    r = load_result(path)
    first = r["sweeps"][0]
    s = score(phrases, [x["hyp"] for x in first])
    times = [x["stt_s"] for sweep in r["sweeps"] for x in sweep]
    s.update(config=r["config"], p50=pct(times, .5), p95=pct(times, .95), sweeps=len(r["sweeps"]),
             sweeps_with_other_text=sum(1 for sw in r["sweeps"][1:]
                                        if [x["hyp"] for x in sw] != [x["hyp"] for x in first]))
    return s


def main() -> None:
    out_dir = sys.argv[1]
    phrases = load_phrases()
    terms = [t for p in phrases for t in p[1].split("|")]
    rows = [score_file(p, phrases) for p in sorted(glob.glob(f"{out_dir}/*.json"))]
    rows.sort(key=lambda r: (-r["term_recall"], r["wer"]))
    print(f"{len(phrases)} phrases, {len(terms)} terms.\n")
    print("| config | WER | CER | terms right | empty | repeated 3-grams | omitted words | p50 s | p95 s | missed terms |")
    print("|---|---|---|---|---|---|---|---|---|---|")
    for r in rows:
        print(f"| {r['config']} | {r['wer']:.1f} | {r['cer']:.1f} | {r['term_recall']:.0f}% | {r['empty_outputs']} | "
              f"{r['repeated_ngrams']} | {len(r['omitted_words'])} | {r['p50']:.3f} | {r['p95']:.3f} | "
              f"{', '.join(r['missed_terms']) or '-'} |")
    print()
    for r in rows:
        if r["note"]:
            print(f"{r['config']}: {r['note']}")
    for r in rows:
        print(f"\n### {r['config']}\n\n{r['text']}\n")


if __name__ == "__main__":
    main()
