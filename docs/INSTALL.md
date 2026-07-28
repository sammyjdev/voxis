# Installing VOXIS

VOXIS runs on **macOS with Apple Silicon only**. Dictation (`laris`) and
meeting recording (`miris`) install from source into Python venvs; they use
`mlx-whisper` and Core Audio. The **tray** (`apps/tray`) is the shared control
surface and starts the dictation daemon for you.

A Windows port lives in its own fork:
[Allyhgf/voxis-windows](https://github.com/Allyhgf/voxis-windows).

---

## Prerequisites (macOS)

- macOS 14.2 or newer (Core Audio process tap), Apple Silicon
- Python 3.11 (`python3.11 --version`)
- Xcode Command Line Tools for the audio helper: `xcode-select --install`
- Node.js 20+ and Rust stable if you want to build the tray from source
- An LLM CLI for meeting summaries. Default is the
  [Antigravity CLI](https://antigravity.google) (`agy`) logged in; any
  prompt-in / text-out CLI works - set it in `~/.miris/config.toml`.

```bash
git clone https://github.com/sammyjdev/voxis.git ~/code/voxis
cd ~/code/voxis
```

---

## 1. LARIS - dictation engine

```bash
cd ~/code/voxis/packages/laris
python3.11 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

Run it (hold Right-Cmd to talk, double-tap for hands-free):

```bash
python -m laris
```

First launch, macOS will prompt for **Accessibility** / **Input Monitoring**
(the global hotkey) and **Microphone**. Grant both.

Orb themes live in `~/.config/voice-at/orb-theme.json` and can be changed live
via the tray or the `/theme` endpoint - see `packages/laris/README.md`.

---

## 2. MIRIS - meeting recorder

MIRIS reuses LARIS for transcription, so install it into the MIRIS venv too.

```bash
cd ~/code/voxis/packages/miris
python3.11 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt -r ../laris/requirements.txt
make -C helper                       # builds helper/audiotap (needs swiftc)
```

Put the CLI on your PATH (optional but recommended):

```bash
ln -sf "$PWD/.venv/bin/miris" ~/.local/bin/miris
```

Verify - this also triggers the **System Audio Recording** permission prompt:

```bash
miris doctor
```

Use it:

```bash
miris start     # begin recording (detached; survives closing the terminal)
miris status
miris stop      # stop, transcribe, summarize, save the note to your vault
```

Config is created on first run at `~/.miris/config.toml` - vault root,
category-to-folder map, and `llm_command`. Raw recordings stay under
`~/.miris/sessions/<timestamp>/` until you delete them; they are created
`0600` (owner-only).

Tip: use a headset. Without one the mic also picks up the speakers, so your
channel echoes the other side.

---

## 3. VOXIS Tray (optional)

### From a release (recommended)

Download `VOXIS_<version>_aarch64.dmg` from the
[Releases page](https://github.com/sammyjdev/voxis/releases).

### From source

```bash
cd ~/code/voxis/apps/tray
npm install
npm run tauri build        # bundle lands in src-tauri/target/release/bundle/
```

The tray drives the engines through their existing interfaces (the `miris`
CLI, the LARIS overlay server, config files). It reads the absolute paths of
the engines from `~/.config/voice-at/tray.json`; the defaults assume
`~/code/voxis` and `~/.local/bin/miris`, so adjust that file if you cloned
elsewhere.

### Start VOXIS at login (macOS)

System Settings > General > Login Items > "+" > `/Applications/VOXIS.app`.

VOXIS loads the dictation model at startup (~3.6s, 276 MB resident), so with
this enabled the model is ready before you ever press the dictation key.

---

## Unsigned builds

VOXIS is **not code-signed or notarized** (that needs a paid Apple
developer certificate). The app is safe; the OS just cannot verify the
publisher. First launch:

- **macOS**: Gatekeeper blocks it ("damaged" or "cannot be verified"). After
  moving VOXIS to Applications, clear the quarantine flag once:

  ```bash
  xattr -cr /Applications/VOXIS.app
  ```

  On macOS 15 (Sequoia) and newer, right-click > Open is no longer enough -
  use the `xattr` command, or go to System Settings > Privacy & Security >
  "Open Anyway".

---

## Uninstall / reset

- Tray: delete `/Applications/VOXIS.app`.
- Engines: remove the `.venv` folders and the `~/.local/bin/miris` symlink.
- State and config: `~/.config/voice-at/`, `~/.miris/`, and any notes already
  written into your vault.
