<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="brand/voxis-wordmark-dark.svg">
    <img src="brand/voxis-wordmark-light.svg" alt="VOXIS" width="360">
  </picture>
</p>

<p align="center">
  Local voice tools for dictation and meeting notes. The "O" is the orb that
  reacts while VOXIS listens.
</p>

---

VOXIS keeps speech recognition on your machine. LARIS turns speech into text
in the app you already have focused. MIRIS records meetings, transcribes them,
and writes notes to an Obsidian vault. The tray app is the shared control
surface.

## Family

| Component | Purpose | Availability |
|-----------|---------|--------------|
| `packages/laris` | Local dictation with an orb overlay. | macOS standalone, Windows in the VOXIS installer. |
| `packages/miris` | Meeting recording, transcription, and Obsidian notes. | macOS on Apple Silicon. |
| `apps/tray` | Tauri v2 controls for dictation, meetings, themes, and settings. | Windows, macOS, and Linux. |
| `brand/` | Wordmark, mark, and menu bar SVG assets. | Shared assets. |

On Windows, the installer bundles LARIS push-to-talk dictation and needs no
system Python. On macOS, LARIS and MIRIS run from their own Python
environments. Linux currently ships the tray only.

## Install

Full steps per component and per OS: [docs/INSTALL.md](docs/INSTALL.md).

### Windows

Install the `.msi` or `-setup.exe` from the
[Releases page](https://github.com/sammyjdev/voxis/releases). The installer
includes the tray and LARIS. First use downloads the `small` Whisper model.
CPU int8 is included. CUDA is a manual upgrade.

### macOS, Apple Silicon

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
unsigned. See [docs/INSTALL.md](docs/INSTALL.md) for the one-time
Gatekeeper or SmartScreen step.

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

- Windows ships LARIS push-to-talk dictation in the tray installer. MIRIS is
  still macOS-only.
- Meetings: two-channel recording ("Eu"/"Eles") gives speaker separation for
  free; notes route to the vault by user-defined categories.
