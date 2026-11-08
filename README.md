<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="brand/voxis-wordmark-dark.svg">
    <img src="brand/voxis-wordmark-light.svg" alt="VOXIS" width="360">
  </picture>
</p>

<p align="center">
  Native macOS voice tools for dictation and meeting notes. The "O" is the orb
  that reacts while VOXIS listens.
</p>

---

VOXIS keeps speech recognition on your machine. LARIS turns speech into text
in the app you already have focused. MIRIS records meetings, transcribes them,
and writes notes to an Obsidian vault. The tray app is the shared control
surface.

**macOS on Apple Silicon, and Windows.** On macOS speech recognition runs on
`mlx-whisper`, which requires Apple Silicon. On Windows it runs on whisper.cpp
in a native binary; see [Windows](#windows).

## Windows

Windows is at a release candidate: tag
[`v0.2.0-rc3`](https://github.com/sammyjdev/voxis/releases/tag/v0.2.0-rc3)
(release candidate, not published). Its draft release holds one installer per
edition:

| Installer | Size at rc3 | Pick it when |
|-----------|-------------|--------------|
| `VOXIS_<version>_x64_cuda-setup.exe` | 523,385,745 bytes | you have an NVIDIA GPU. The CUDA 13.3.1 build targets GTX 16xx, RTX 20xx and newer cards (the generations as NVIDIA states them for the architectures it is compiled for). It carries NVIDIA's cuBLAS, redistributed under the NVIDIA CUDA EULA. Whether it is faster than the Vulkan edition is not measured yet. |
| `VOXIS_<version>_x64_vulkan-setup.exe` | 9,641,882 bytes | any other GPU (AMD, Intel) or an NVIDIA card without CUDA. |

Verified configuration: Windows 11 x64 with an NVIDIA GeForce RTX 4070 Ti, the CUDA 13.3.1 edition, and Vulkan edition transcription on that same host. Not tested: Windows 10, ARM64, AMD and Intel GPUs, other NVIDIA generations, and other audio devices.

- It installs for the current user, with no admin prompt, and adds VOXIS to
  the Start menu. To switch editions, install the other one over it.
- The first launch downloads the speech models (1.6 GB) into
  `%LOCALAPPDATA%\voxis\models`. Dictation turns on when the download ends;
  the tray tooltip shows the progress.
- Builds are unsigned, so SmartScreen says "Windows protected your PC": choose
  **More info**, then **Run anyway**.
- **Iniciar com o Windows** in the tray menu starts VOXIS at logon.
- Meeting minutes need `anthropic_api_key` in `%APPDATA%\voxis\secrets.toml`;
  without it a meeting keeps only its transcript. Without Google credentials
  in `%APPDATA%\voxis`, the minutes stay in `ata.md` in the session folder
  instead of going to Google Drive.

What Windows does differently from macOS:

- **Dictation key**: `f13` by default; `f5` to `f15` can be chosen in
  **Configurações...**. F13 needs a keyboard or a key remapper that sends it.
  Hold the key to talk; tap it twice for hands-free, where an energy gate, not
  Silero, ends each utterance.
- **Language**: Portuguese (pt-BR) only, for dictation and meetings; the
  settings window shows it as fixed text.
- **Speech model**: `ggml-large-v3-turbo` with the Silero VAD
  (`ggml-silero-v5.1.2`), run by whisper.cpp.
- **Notes**: a meeting's minutes go to `ata.md` in its session folder or to a
  Google Doc in the Drive folder `VOXIS Atas`, not to an Obsidian vault. The
  vault, LLM command, categories, STT model and noise reduction settings are
  shown disabled with "Não disponível no Windows".
- **Paste**: dictation puts the text on the clipboard, sends Ctrl+V and then
  restores the clipboard's previous text. Non-text clipboard contents (an
  image, copied files) are replaced and not restored. Windows silently drops
  keys sent into a window running as administrator: when VOXIS detects such a
  window it skips the paste and leaves the text on the clipboard for you to
  paste; a window that blocks input in a way VOXIS does not detect loses the
  text when the previous clipboard text comes back.
- **Where your data stays**: tray meetings keep audio, transcript and notes in
  `%USERPROFILE%\.miris\sessions\<id>`; `meetrec meeting` runs keep theirs in
  `%USERPROFILE%\.voxis\meetings`; logs are in `%USERPROFILE%\.voxis\logs` and
  `%USERPROFILE%\.config\voice-at\laris.log`. Nothing deletes them
  automatically, and nothing moves or compresses them: each dictation start
  appends to `laris.log` after a `--- voxis run <time> ---` line, with no
  rotation. To remove a
  recording, delete its folder; to remove logs, delete the files while VOXIS
  is closed. Dictation logs written by builds before this change hold the
  dictated text in three places: `%USERPROFILE%\.voxis\logs\dictate-*.log`,
  `%USERPROFILE%\.config\voice-at\laris.log` (the tray captures dictate's
  output there) and `%USERPROFILE%\.config\voice-at\laris.log.1` (the
  previous run, kept by those builds). Delete all three to remove it.

## Family

| Component | Purpose |
|-----------|---------|
| `packages/laris` | Local dictation with an orb overlay. |
| `packages/miris` | Meeting recording, transcription, and Obsidian notes. |
| `apps/tray` | Tauri v2 controls for dictation, meetings, themes, and settings. |
| `brand/` | Wordmark, mark, and menu bar SVG assets. |

## Install

Full steps per component: [docs/INSTALL.md](docs/INSTALL.md).

```bash
git clone https://github.com/sammyjdev/voxis.git ~/code/voxis
# dictation
cd ~/code/voxis/packages/laris && python3.11 -m venv .venv && \
  .venv/bin/pip install -r requirements.txt && .venv/bin/python -m laris
# meetings
cd ~/code/voxis/packages/miris && python3.11 -m venv .venv && \
  .venv/bin/pip install -r requirements.txt -r ../laris/requirements.txt && \
  make -C helper && .venv/bin/miris doctor
```

The tray app installs from the
[Releases page](https://github.com/sammyjdev/voxis/releases). Builds are
unsigned. See [docs/INSTALL.md](docs/INSTALL.md) for the one-time Gatekeeper
step.

Dictation is always on while the tray runs: `cmd_r` is the control, and the
model loads once at startup instead of on every use. Starting VOXIS at login
means it is ready before you press the key - see INSTALL.md.

## Privacy & security

- **Local by default.** Audio, transcripts, and notes stay on your machine.
  MIRIS sends meeting text only to the LLM CLI you configure for summaries
  (`agy` by default).
- **On Windows**, when `anthropic_api_key` exists the meeting transcript goes
  to the Anthropic API to write the minutes, and when both Google files exist
  the minutes go to Google Drive (folder `VOXIS Atas`). Nothing else leaves a
  Windows machine; dictation never leaves it.
- On macOS, session recordings and the auth token are written owner-only
  (`0600`). Windows sets no such permission: the files have your profile
  folder's permissions.
- Dependencies are scanned on every push and weekly by
  `.github/workflows/security-audit.yml`: cargo-audit over
  `apps/tray/src-tauri/Cargo.lock` (the tray) and npm audit over
  `apps/tray/package-lock.json`. A finding fails that check. The Windows
  `meetrec` binary is not covered: `spikes/windows-rust/Cargo.lock` is not
  scanned yet.
- Releases are unsigned - see INSTALL.md.

## Status

- Dictation: push-to-talk with a live orb, transcribing through
  `whisper-large-v3-turbo-q4`.
- Meetings: two-channel recording ("Eu"/"Eles") gives speaker separation for
  free; notes route to the vault by user-defined categories.
