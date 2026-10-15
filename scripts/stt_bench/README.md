# STT benchmark

Measures speech-to-text configs on real pt-BR and English speech, one
utterance per call like LARIS and MIRIS. Results and decisions:
`docs/superpowers/specs/2026-10-15-stt-model-benchmark.md`. Needs an NVIDIA
GPU. Nothing here runs in CI.

Data, model copies and the venv live in `$STT_BENCH_HOME` (default
`~/.cache/voxis-bench`), never in the repo: CORAA is CC BY-NC-ND.

## Setup

```bash
B=~/.cache/voxis-bench; mkdir -p $B/data $B/models
uv venv --python 3.12 $B/.venv
uv pip install --python $B/.venv 'faster-whisper==1.2.1' nvidia-cublas-cu12 'nvidia-cudnn-cu12==9.*' \
  pyarrow soundfile jiwer num2words numpy 'onnx-asr[hub]' 'onnxruntime-gpu[cuda,cudnn]'
# pt-BR: CORAA v1.1 test split (2.6 GB)
for i in 0 1 2 3 4 5; do curl -L -o $B/data/test-$i.parquet \
  "https://huggingface.co/datasets/Racoci/CORAA-v1.1/resolve/main/data/test-0000$i-of-00006.parquet"; done
# English: one AMI test shard (240 MB)
curl -L -o $B/data/ami-ihm-test-0.parquet \
  https://huggingface.co/datasets/edinburghcstr/ami/resolve/main/ihm/test-00000-of-00004.parquet
# onnxruntime 1.31 refuses weights behind HF cache symlinks: real local copies
$B/.venv/bin/python -c 'from huggingface_hub import snapshot_download as d
d("istupakov/parakeet-tdt-0.6b-v3-onnx", local_dir="'$B'/models/parakeet-v3")
d("istupakov/canary-1b-v2-onnx", local_dir="'$B'/models/canary-v2")'
```

If `onnxruntime` (CPU) ends up installed next to `onnxruntime-gpu`, remove it
and reinstall the GPU package with `--reinstall-package onnxruntime-gpu`: the
two share one module directory.

## Run

```bash
B=~/.cache/voxis-bench; cd scripts/stt_bench
for c in turbo-pt-b5 turbo-auto-b5 large-v3-pt-b5 parakeet-v3; do $B/.venv/bin/python bench.py $c $B/full; done
$B/.venv/bin/python score.py $B/full            # pt-BR table
for c in turbo-en-b5 turbo-auto-b5 parakeet-v3; do $B/.venv/bin/python bench.py $c $B/ami ami; done
$B/.venv/bin/python score.py $B/ami en          # English table
$B/.venv/bin/python langwin.py                  # language guess over 30 s windows
```

Configs are in `bench.py:CONFIGS`. `PER_SOURCE=2` gives a 10-utterance smoke
run.

## The owner's voice

`voice-phrases.tsv` holds 20 dictation-style phrases, each with the technical
terms that must come out right. Record them in one go, one phrase at a time
with a pause between them, hands-free off (it would paste what it hears):

```bash
timeout 150 pw-record --rate 16000 --channels 1 --format s16 $B/voice.wav
```

Then split it the way LARIS does (Silero, 400 ms of silence) and score:

```bash
cd scripts/stt_bench
../../packages/laris/.venv/bin/python segment.py $B/voice.wav $B/voice.spans.json
mkdir -p $B/voice; for c in turbo-pt-b5 large-v3-pt-b5 parakeet-v3; do
  $B/.venv/bin/python voice.py $c $B/voice.wav $B/voice.spans.json $B/voice; done
$B/.venv/bin/python voice_score.py $B/voice
```

## First-word clipping

`preroll.py` replays a recording through LARIS's VAD loop and transcribes
each utterance as LARIS cuts it and with some audio from before the VAD
fired. It runs in laris's venv:

```bash
../../packages/laris/.venv/bin/python preroll.py $B/voice.wav 200 $B/preroll-200
```
