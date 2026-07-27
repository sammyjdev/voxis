# LARIS: honest startup state and always-on dictation

Date: 2026-07-27
Status: design agreed; spec pending review; not implemented

## Problem

Turning dictation on shows "Ditado ✓" in the tray menu several seconds before
push-to-talk actually responds. During that window the UI asserts something
false, and a user who holds `cmd_r` gets silence with no explanation. This cost
a full debugging session: the silence was indistinguishable from a broken mic.

Measured on an M-series Mac with the model already cached:

| Step | Cost |
|------|------|
| `get_backend()` -> `Transcriber.__init__` -> `import mlx_whisper` | 1.4 s |
| `backend.warm()` (model load + Metal compile) | 2.0 s |
| **Total window** | **3.6 s** |

On a fresh install the model is *downloaded* (442 MB), so the window is minutes.

### Root cause

`take_snapshot` (`apps/tray/src-tauri/src/lib.rs:68`) decides the menu state from
`laris::alive()`, which only proves the HTTP port answered. But uvicorn binds
8421 on a background thread in `__main__.py` *before* `daemon.run()` calls the
blocking `warm()`, and `ptt.start()` runs only after it. The tray measures
"the web server is up" while the question that matters is "is push-to-talk
listening?".

A second cost compounds it: the tray's "Ditado" menu item is a process
switch - `daemon.owns_running() { stop() } else { spawn() }`
(`lib.rs:266-286`). The model is loaded and thrown away on every toggle, so the
3.6 s is paid *every single time*, not once.

## Decisions

### Dictation is always on

The toggle is removed. `cmd_r` becomes the only control. Recording a meeting
(MIRIS) stays optional - that is the switch that earns its place. Nothing about
dictation needs a second on/off when the push-to-talk key is already one.

This removes the arm/disarm endpoint that an earlier draft of this design
proposed: with no toggle, there is nothing to arm.

### The daemon persists

`spawn()` moves from the toggle to the tray's `setup()`; `stop()` is reserved
for "Sair". The model loads once per system session instead of once per toggle.

Consequence, accepted deliberately: 276 MB stays resident while VOXIS runs, and
"Sair" becomes the only way to release it. Measured cost of the loaded model:

| Stage | RSS |
|-------|-----|
| baseline python | 14 MB |
| after `warm()` | 290 MB |
| **model cost** | **276 MB** (1.7% of 16 GB) |

### VOXIS starts at login

macOS already does this: System Settings > General > Login Items. No code.
Packaging it as a product feature (`tauri-plugin-autostart` plus a Settings
toggle) is deferred - it is already tracked in `docs/BACKLOG.md:71`.

With VOXIS at login and a persistent daemon, the 3.6 s window moves to login
time, where nobody is dictating.

### The tray reads readiness, not liveness

`/theme` gains a `ready` field, set only after `ptt.start()`. The tray reads it
in the 2 s poll it already runs (it already reads `ptt_trusted` from the same
payload) and shows "Carregando modelo..." instead of "Ditado".

### A failed load must not become limbo

Moving `warm()` off the main path means an exception there would die silently
and leave the menu on "Carregando modelo..." forever - trading one lie for
another. The load reports failure through `/theme`; the tray surfaces it and
notifies.

### The orb "loading" state is cut

An earlier round of this design added a `loading` phase to the orb so `cmd_r`
during the window would show *something*. With the daemon persistent and VOXIS
at login, that window stops existing in normal use. Cutting it. It costs one
line in `PHASE_CAPTIONS` (`frontend/voice-orb.js:23`) if it ever comes back.

### The STT model stays whisper-large-v3-turbo-q4

Candidates evaluated, all required to run locally on Apple Silicon - sending
dictation audio to a cloud model would break the product's stated "local by
default" guarantee:

| Model | Size | Latency | Verdict |
|-------|------|---------|---------|
| whisper-large-v3-turbo-q4 | 442 MB | 0.71 s / 4 s audio | **keep** |
| whisper-large-v3-turbo f16 | 1.5 GB | 0.95 s | already benchmarked 2026-07-11: quality tied, 1.2 GB more RAM |
| parakeet-tdt-0.6b-v3 | 2.51 GB | RTFx 3332 | only genuinely new candidate; trained on *European* Portuguese by NVIDIA's own model card |
| Voxtral-Mini-4B-Realtime | 4 B | unmeasured | latency likely outside the dictation envelope |
| Phi-4 Multimodal | 5.6 B | n/a | won the pt-BR paper below, but **no MLX port with the audio tower exists** |

Dictation is the most latency-sensitive STT use there is: the text must land
when the key is released. That envelope excludes multi-billion-parameter
multimodal models regardless of their accuracy.

Decision rests on viability plus the absence of any complaint about
transcription quality - every reported problem has been infrastructure. It does
**not** rest on having measured Parakeet, which we did not. Revisit only if
quality becomes a real complaint.

### initial_prompt, without a measurement harness

Whisper's `initial_prompt` is unused today. A 2026 ICAART study on pt-BR
(Instituto Eldorado, CORAA ASR v1.1 + Common Voice, ~7k samples each) measured
error by sentence length: sentences of 11+ words score **3-4x lower WER** than
sentences of 1-5 words. The primary use here is dictating prompts - short,
imperative sentences, squarely in the worst-performing band. The cause is
missing context, which is exactly what `initial_prompt` supplies.

Shipping it behind a reversible setting without a WER harness is a deliberate
call: low risk, user-disableable, but the gain will not be measurable. If
quality ever becomes a complaint, CORAA ASR v1.1 (290 h of pt-BR, dev split on
HuggingFace) is the instrument - it needs no recording effort.

## Scope

1. Remove the dictation toggle; `cmd_r` is the only control
2. Daemon persists: `spawn()` in `setup()`, `stop()` only on "Sair"
3. VOXIS at login via macOS Login Items (no code)
4. `ready` in `/theme`; tray shows "Carregando modelo..."; load failure surfaces
   as an error, never as permanent "loading"
5. Model, language and denoise state in the boot log and in Settings
6. `initial_prompt` behind a setting

Out of scope: switching STT models, the Parakeet comparison, packaging autostart
as a product feature.

## Testing

| Behaviour | Seam |
|-----------|------|
| `ready` is false until `ptt.start()` has run | laris daemon |
| a failing model load surfaces an error instead of staying "loading" | laris daemon |
| `/theme` carries `ready` and the error field | laris server |
| the tray renders "Carregando modelo..." off `ready` | tray snapshot |
| quitting still stops the daemon; closing a window does not | tray |
| `initial_prompt` reaches `mlx_whisper.transcribe` when set | transcriber |

## Related fixes (applied and verified on hardware 2026-07-27, outside this design)

- `audio_capture.py:85` - PortAudio is blind to USB hotplug; re-initialize on a
  failed open. Root cause of the "orb blinks, records nothing" report.
- `laris.rs:125` - spawn python with `-u`; a file-redirected stdout is
  block-buffered, so the `[Voice]` prints never reached the log that exists to
  capture them.
- `lib.rs` - `should_prevent_exit`: the tray owns no permanent window, so
  closing Settings asked the runtime to exit and took dictation with it.

## Known debt

Cleared 2026-07-27:

- venv console scripts in both `packages/laris` and `packages/miris` carried
  shebangs pointing at the pre-move `~/code/voxis` path, so `pip install`
  failed. Paths rewritten in place (76 files); rebuilding the venvs would have
  meant re-downloading torch, mlx and DeepFilterNet for a string substitution.
  Suites re-run green afterwards: laris 180, miris 34.
- An orphaned uvicorn from 2026-07-16 (PID 35876) was still resident, no longer
  listening on anything. Killed.

Open, deliberately:

- `settings.rs:59` defaults `laris_dir` to `~/code/voxis/packages/laris`, which
  is what `README.md` instructs users to clone. It is not wrong - this machine
  moved the repo, and `~/.config/voice-at/tray.json` is the override built for
  exactly that. Changing the compiled default would swap one machine-specific
  path for another and break anyone following the docs.
- Consequence for scope item 2: once `spawn()` moves to `setup()`, a failure
  there must notify. Today the toggle surfaces it (`lib.rs:283`); the setup path
  has no such handler, and a bad `laris_dir` would fail silently at boot.

## Sources

- CORAA ASR: https://arxiv.org/abs/2110.15731 - corpus at https://github.com/nilc-nlp/CORAA
- pt-BR multimodal ASR comparison (ICAART 2026): https://www.scitepress.org/Papers/2026/146373/146373.pdf
- parakeet-tdt-0.6b-v3 model card: https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3
