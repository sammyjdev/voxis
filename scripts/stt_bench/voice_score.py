"""Score voice.py output against voice-phrases.tsv.

WER over the whole read text (span boundaries do not need to match phrase
boundaries) and term recall: the share of technical terms that come out
recognisable, after the same normalisation as score.py.
"""
from __future__ import annotations

import glob
import json
import os
import sys

import jiwer

from score import normalize, pct


def main() -> None:
    out_dir = sys.argv[1]
    here = os.path.dirname(os.path.abspath(__file__))
    phrases = [line.rstrip("\n").split("\t")
               for line in open(os.path.join(here, "voice-phrases.tsv")) if line.strip()]
    ref = normalize(" ".join(p[0] for p in phrases), lenient=True)
    terms = [t for p in phrases for t in p[1].split("|")]
    rows = []
    for path in sorted(glob.glob(f"{out_dir}/*.json")):
        d = json.load(open(path))
        res = d["results"]
        raw = " ".join(r["hyp"] for r in res)
        hyp = normalize(raw, lenient=True)
        missed = [t for t in terms if normalize(t) not in hyp]
        rows.append((d["config"], 100 * jiwer.wer(ref, hyp), 100 * (len(terms) - len(missed)) / len(terms),
                     pct([r["stt_s"] for r in res], .5), pct([r["stt_s"] for r in res], .9), missed, raw))
    rows.sort(key=lambda r: (-r[2], r[1]))
    print(f"{len(phrases)} phrases, {len(terms)} terms.\n")
    print("| config | WER | terms right | p50 s | p90 s | missed terms |")
    print("|---|---|---|---|---|---|")
    for name, w, recall, p50, p90, missed, _ in rows:
        print(f"| {name} | {w:.1f} | {recall:.0f}% | {p50:.3f} | {p90:.3f} | {', '.join(missed) or '-'} |")
    print()
    for name, *_, raw in rows:
        print(f"### {name}\n\n{raw}\n")


if __name__ == "__main__":
    main()
