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

Download one installer from the
[latest release](https://github.com/sammyjdev/voxis/releases/latest):

| Installer | Pick it when |
|-----------|--------------|
| `VOXIS_<version>_x64_cuda-setup.exe` | you have an NVIDIA GPU (GTX 16xx, RTX 20xx or newer). Fastest. About 430 MB, because it carries NVIDIA's cuBLAS, redistributed under the NVIDIA CUDA EULA. |
| `VOXIS_<version>_x64_vulkan-setup.exe` | any other GPU (AMD, Intel) or an NVIDIA card without CUDA. About 9 MB. |

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
- Session recordings and the auth token are written owner-only (`0600`).
- Dependencies are scanned continuously (cargo-audit + npm audit, see
  `.github/workflows/security-audit.yml`); 0 known vulnerabilities in the
  shipped tray.
- Releases are unsigned - see INSTALL.md.

## Status

- Dictation: push-to-talk with a live orb, transcribing through
  `whisper-large-v3-turbo-q4`.
- Meetings: two-channel recording ("Eu"/"Eles") gives speaker separation for
  free; notes route to the vault by user-defined categories.
