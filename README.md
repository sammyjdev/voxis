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

**macOS on Apple Silicon only.** Speech recognition runs on `mlx-whisper`,
which requires Apple Silicon, and the tray drives macOS-specific menu bar and
permission APIs. A Windows port lives in its own fork:
[Allyhgf/voxis-windows](https://github.com/Allyhgf/voxis-windows).

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
