# Windows native binary - developer notes

`meetrec` is the native Windows binary the tray runs for meetings and
dictation; `stt-bench` is the whisper.cpp side of the speech benchmark;
`keyprobe` prints key codes. User-facing notes are in the root `README.md`
and `docs/INSTALL.md`.

## Build and test

Run from the repo root in Git Bash or `cmd`. The scripts find Visual Studio's
C++ tools; a short target folder keeps ggml's nested CMake paths under
`MAX_PATH`.

```bat
scripts\windows\meetrec-test.cmd            :: native tests, CUDA build
spikes\windows-rust\build-stt-bench.cmd     :: stt-bench + meetrec, Vulkan
cargo test --release --manifest-path apps\tray\src-tauri\Cargo.toml
node --test apps/tray/tests/settings.test.ts
bash scripts/windows/check-windows-contract.sh
```

## Commands

The tray drives `start`, `stop`, `process <id>`, `recover <id>`,
`devices --json`, `models fetch [--verify]` and `dictate`; `meeting [mic]` is
the interactive meeting command. These are developer commands, documented
only here:

- `meetrec record <dir> [mic] [secs]` - records `mic.wav` and `system.wav`
  into `<dir>`; refuses a folder that already holds either file.
- `meetrec transcribe <dir> <model.bin> <vad.bin> [lang]` - writes
  `transcript.md`.
- `meetrec summarize <dir>` - writes the minutes from `transcript.md`.

`miris.recorder` and `miris.pipeline` are internal markers the tray matches
on, not commands.

## Git Bash rewrites `/S` and `/v`

Git Bash (MSYS) turns an argument that looks like a POSIX path into a Windows
path before the program sees it, so an installer's `/S` becomes `S:/` and the
silent install opens the interactive wizard instead. Run such commands with
the conversion off, or double the slash:

```bash
MSYS2_ARG_CONV_EXCL='*' ./VOXIS_0.2.0-rc3_x64_vulkan-setup.exe /S
./VOXIS_0.2.0-rc3_x64_vulkan-setup.exe //S
```

The same applies to `/v` and to `cmd.exe /c`.

## Performance harness (layer 1)

`scripts/stt_bench/perf_driver.py` runs `stt-bench` (CUDA and Vulkan builds)
and `voice.py` (faster-whisper) on the scripted corpus in 6 counterbalanced
blocks and keeps raw schema 2 JSON, stderr, exit codes, hashes, the build
record, the host record and CPU, working-set and GPU memory samples. It is an
engine measurement without VAD, not product latency; it states no target.
See its docstring for `prepare`, `run`, `controls` and `report`, and
`.specs/features/windows-port/plan.md` S10 for the protocol rules.
