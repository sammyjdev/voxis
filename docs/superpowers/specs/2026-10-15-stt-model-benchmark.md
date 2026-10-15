# STT model benchmark on Linux (2026-10-15)

Which speech-to-text model LARIS (dictation) and MIRIS (meetings) should run
on the Linux box, judged on speed, accuracy and handling of real speech.
Harness: `scripts/stt_bench/` (how to rerun it is in its README).

## Setup

- RTX 4070 Ti 12 GB, driver 595.91 (CUDA 13.2). The LARIS daemon held its own
  2.3 GB of VRAM throughout, as in daily use.
- faster-whisper 1.2.1 / CTranslate2 4.8.3, float16 on CUDA. Parakeet and
  Canary through onnx-asr 0.12.0 on onnxruntime-gpu 1.31 (CUDA provider).
- **pt-BR:** CORAA v1.1 test split, 100 random utterances from each of its five
  corpora (ALIP, C-ORAL-BRASIL I, NURC-Recife, SP2010, TEDx), 500 in all,
  33 min, median 2.9 s. Real speakers, mostly spontaneous, some noisy.
- **English:** AMI meeting corpus, headset mics, 300 random utterances of one
  test shard, 10.6 min. Stands in for the English half of bilingual meetings.
- One utterance per call, the way both apps call the model: LARIS sends each
  VAD utterance, MIRIS each VAD span of a channel.

Metrics: WER and CER after lowercasing and stripping punctuation, with digits
spelled out in words (references spell numbers). "Lenient" WER also drops
filled pauses and immediate word repeats ("e e", "né", "uh"). The references
keep those, Whisper leaves them out on purpose, and nobody wants them typed.
Latency is the warm transcribe time of 2-8 s utterances (the dictation range).
RTF is transcribe time over audio time.

## pt-BR (CORAA, 500 utterances)

| config | WER lenient | WER strict | CER | WER 1-5 words | WER 11+ words | p50 s | p90 s | RTF | VRAM MiB | wrong language | empty |
|---|---|---|---|---|---|---|---|---|---|---|---|
| large-v3-turbo, pt, beam 5 (LARIS today) | **20.3** | 21.7 | 12.3 | 40.9 | 16.5 | 0.130 | 0.158 | 0.035 | 2292 | 0 | 0 |
| large-v3, pt, beam 5 | **20.3** | 21.9 | 12.5 | 37.6 | 17.1 | 0.234 | 0.329 | 0.062 | 3956 | 0 | 0 |
| parakeet-tdt-0.6b-v3 | 20.9 | 22.1 | 12.3 | 41.7 | 18.1 | **0.034** | 0.038 | 0.009 | 3730 | 0 | 6 |
| large-v3-turbo, auto-detect (MIRIS before this change) | 21.7 | 23.2 | 13.4 | 49.6 | 16.9 | 0.221 | 0.248 | 0.056 | 2292 | **59** | 0 |
| large-v3-turbo, pt, beam 1 | 21.7 | 23.0 | 13.3 | 43.4 | 18.1 | 0.113 | 0.130 | 0.030 | 2292 | 0 | 0 |
| large-v3-turbo, detect between pt and en only | 21.7 | 23.2 | 13.0 | 46.7 | 17.1 | 0.217 | 0.248 | 0.056 | 2292 | 45 | 0 |
| medium, pt, beam 5 | 23.3 | 24.6 | 13.9 | 49.2 | 18.1 | 0.150 | 0.212 | 0.045 | 2132 | 0 | 14 |
| canary-1b-v2, pt | 25.0 | 30.1 | 16.8 | 52.3 | 18.6 | 0.084 | 0.119 | 0.022 | 5500 | 0 | 1 |
| small, pt, beam 5 | 29.4 | 30.6 | 17.2 | 63.6 | 20.5 | 0.077 | 0.114 | 0.030 | 852 | 0 | 15 |

Lenient WER by corpus:

| config | ALIP | C-ORAL | NURC-Recife | SP2010 | TEDx |
|---|---|---|---|---|---|
| large-v3-turbo, pt | 29.9 | 20.5 | 21.7 | 17.9 | 16.3 |
| large-v3, pt | 28.9 | 18.3 | 23.3 | 19.1 | 17.2 |
| parakeet-v3 | 26.8 | 17.4 | 22.5 | 20.2 | 20.5 |
| large-v3-turbo, auto | 35.6 | 20.5 | 24.1 | 19.4 | 16.3 |

## English (AMI, 300 utterances)

| config | WER lenient | WER strict | CER | WER 1-5 words | WER 11+ words | p50 s | wrong language |
|---|---|---|---|---|---|---|---|
| parakeet-tdt-0.6b-v3 | **18.4** | 18.0 | 12.2 | 46.6 | 11.5 | 0.034 | 0 (6 empty) |
| large-v3-turbo, en | 24.1 | 25.8 | 17.9 | 48.9 | 17.5 | 0.122 | 0 |
| large-v3-turbo, detect between pt and en | 24.3 | 26.0 | 18.1 | 50.3 | 17.5 | 0.210 | 2 |
| large-v3-turbo, auto-detect | 25.1 | 26.7 | 19.2 | 54.5 | 17.5 | 0.209 | 14 |
| large-v3-turbo, pt | 65.6 | 66.1 | 48.0 | 78.1 | 64.1 | 0.122 | 300 |

## Findings

**Dictation (LARIS): keep large-v3-turbo, pt, beam 5.** It ties the best
pt-BR accuracy (20.3) at 0.13 s per utterance and 2.3 GB of VRAM.
- large-v3 gains nothing overall (better on 1-5 words, worse on 11+) and
  costs 1.8x the time and 1.7 GB more.
- Beam 1 saves 17 ms for 1.4 points of WER.
- medium and small are worse and return empty text on 3% of utterances.
- Canary is worse on pt-BR.
- Parakeet v3 is the only real challenger: within 0.6 points and 4x faster.
  Its 0.1 s is small next to the 400 ms of silence the VAD waits for before it
  closes an utterance. Taking it would mean a second STT backend
  (onnxruntime-gpu on its CUDA 13 libraries here, parakeet-mlx on the Mac),
  and it returned empty text on 6 utterances.

**Meetings (MIRIS): Whisper's per-utterance language guess was the real
defect.** With `STT_LANGUAGE=""`, 59 of 500 pt-BR utterances (12%) came back
in another language: English (30), Spanish, Russian, Italian, German, French,
Japanese, Korean and more. Most were short (median 1.2 s), for example "ele
disse que" -> "Он говорит, что...". Restricting the guess to pt and en still
picked English for 45 of them. The same guess over ~30 s of consecutive
speech was right in 40 of 40 windows in pt-BR and in 40 of 40 in English.
MIRIS now guesses once per channel, over the first 30 s of its speech, and
transcribes the whole channel in that language. Expected effect: pt-BR
21.7 -> 20.3, English 25.1 -> 24.1, with no foreign-script lines. Live: a
tray recording of a 150 s pt-BR conversation played through the speakers
had 11 lines in English, Russian or Romanian on the "Eles" channel. The same
recording, transcribed again with the change, had none. The "Eu" channel held
only ~3 s of room noise and stayed in English: that is too little speech to
guess a language from.

**Parakeet v3 is the candidate for English meetings.** It reaches 18.4 WER on
AMI against 24.1 for turbo locked to English (24% fewer errors), equals
turbo in pt-BR, and needs no language decision at all. Meetings are processed
after the fact, so its speed matters less than its English accuracy. Same
cost as above: a second backend on both platforms. Worth doing only if
English meetings are common.

**Short utterances are hard for every model** (37-64% WER on 1-5 words), in
line with the ICAART 2026 pt-BR study cited in the LARIS design. CORAA's
short segments are disfluent fragments of conversation, so this overstates
the error on dictated commands. The dictation test with the owner's voice
(`scripts/stt_bench/voice-phrases.tsv`, 20 phrases with technical terms)
measures that case.

## Owner's voice: live dictation (2026-10-15)

The owner read the 20 phrases of `scripts/stt_bench/voice-phrases.tsv` in
one hands-free session, through the real LARIS pipeline (large-v3-turbo, pt,
beam 5, Silero VAD, paste). Each phrase became exactly one utterance.

| metric | value |
|---|---|
| WER (lenient) / CER | 12.6% / 4.1% |
| technical terms right (whole words) | 30 of 42 |
| phrases exactly right | 6 of 20 |
| transcribe time | 0.11-0.14 s per 1.2-4.2 s utterance (0.23 s for the first) |

Portuguese, numbers ("40%", "2.500", "três da tarde") and widely used terms
(pytest, commit, push, pull request, Kafka, Spring Boot, JSON, PipeWire,
faster-whisper, DeepFilterNet, Obsidian) came out right. The misses are
project names and short English words inside Portuguese: LARIS -> "Larys",
VOXIS -> "Voxys", tray -> "trade", hands-free -> "Hunts Free", sink ->
"sync", diff -> "DIF", REST -> "resting", README -> "Redmi", cargo test ->
"CargoTest", venv do laris -> "Venvidularis", vault -> "Valt". One phrase
lost its first word ("Qual a latência" -> "a latência"); see "The VAD clipped the
first word" below. The 20 utterances were also pasted glued ("falhas.Faz o commit"):
hands-free now puts a space before every utterance after the first of a
session.

**`INITIAL_PROMPT` is not a free fix.** The obvious lever for those misses is
LARIS's `INITIAL_PROMPT` (empty until then). With the owner's vocabulary as
the prompt, turbo got worse on CORAA: 20.3 -> 21.2 WER overall and 40.9 ->
46.5 on 1-5 word utterances. On fragments and fillers the prompt leaks into
the output: "é" -> "LARIS, MIRIS, MIRIS, MIRIS,", "eh" -> "REST, JSON,
venv.". The next section measures it on the owner's own voice.

## Owner's voice: one recording, every model (2026-10-15)

The owner read the same 20 phrases again, with hands-free on (the spacing
and pre-roll fixes live) and `pw-record` keeping the audio. The recording
was cut the way LARIS cuts it (`segment.py` replays the hands-free loop,
pre-roll included) and fed to each config. Plain turbo here matches what
LARIS pasted live.

| config | WER | technical terms right | transcribe p50 |
|---|---|---|---|
| **turbo + vocabulary prompt (list)** | **4.4%** | **40/42** | 0.12 s |
| turbo + vocabulary prompt (running text) | 5.7% | 36/42 | 0.13 s |
| turbo (LARIS before the prompt) | 11.3% | 31/42 | 0.12 s |
| large-v3 | 13.2% | 30/42 | 0.22 s |
| medium | 15.1% | 29/42 | 0.13 s |
| Parakeet v3 | 17.6% | 25/42 | 0.04 s |
| Canary v2 | 25.2% | 21/42 | 0.08 s |

The prompt is the list in `scripts/stt_bench/bench.py:VOCABULARY_PROMPT`. It
holds the test's own terms, so 4.4% is what it does for words it lists,
not for new ones. Terms it does not list (Kafka, Spring Boot, DeepFilterNet,
endpoint, GPU) came out right with and without it.

The price, measured on CORAA's 500 real pt-BR utterances:

| prompt | CORAA WER | utterances given project words nobody said |
|---|---|---|
| none | 20.3% | 0 |
| list | 21.2% | 9 (4 of them nothing but prompt words) |
| running text | 23.2% | 5 |

The leaks are on fillers and fragments: "aham" -> "MIRIS, MIRIS, MIRIS,
MIRIS, MIRIS, MIRIS,", "uh uh" -> "MIRIS, MIRIS, MIRIS, MIRIS,". Without a
prompt, the same fragments already come out as "Obrigado." or "Tchau.". In
hands-free, a cough or an "aham" can paste the prompt's words. The running
text leaks less but loses on both sets.

**Decision (2026-10-15): the list prompt is on for dictation, off for
meetings.** The owner's dictation gains far more than fillers cost. LARIS
reads it from `~/.config/voice-at/laris-settings.json` (the tray's
"Configurações" field). MIRIS clears it: on the recorded pt-BR conversation
it raised WER from 17.7% to 19.3%, and meetings are where fillers live.

**Model choice holds on the owner's voice:** turbo beats large-v3, medium,
Parakeet and Canary. Parakeet and Canary mangle English jargon ("Pai
Teste", "Fester Whisper", "Jason").

## The VAD clipped the first word (2026-10-15)

LARIS started each utterance at the 32 ms chunk where Silero's VADIterator
fired. That chunk is already a few chunks into the word. Replaying the
150 s pt-BR conversation used for the MIRIS test through LARIS's VAD loop
(`scripts/stt_bench/preroll.py`), then transcribing with turbo:

| audio before the VAD fired | WER |
|---|---|
| none (LARIS until now) | 20.6-21.4% (float16 on the GPU varies a little between runs) |
| 100 ms | 17.7% |
| 200 ms | 17.7% |
| 300 ms | 17.7% |

Recovered first words: "Fez um" -> "Você fez um?" (said: "cê fez um"),
"Uma gravidez" -> "É uma gravidez", "Onde eu morava" -> "Bom, onde eu
morava". LARIS now keeps 200 ms (`PREROLL_MS`). The 400 ms minimum that
drops blips counts only the speech after the VAD fired. The real
`stream_utterances` with the change, fed from the same recording: 16.6%.
