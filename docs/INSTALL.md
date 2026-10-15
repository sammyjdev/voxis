# Installing VOXIS

VOXIS runs on **macOS with Apple Silicon**. Dictation (`laris`) and
meeting recording (`miris`) install from source into Python venvs; they use
`mlx-whisper` and Core Audio. The **tray** (`apps/tray`) is the shared control
surface and starts the dictation daemon for you.

Dictation, the tray and meeting recording also run on **Linux (GNOME on
Wayland)**. See [Linux](#linux-gnome-on-wayland).

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
category-to-folder map, and `llm_command`. Give `llm_command` the CLI's full
path (`which agy`): the tray, started from the Dock or at login, does not have
your shell's PATH, and `miris doctor` warns when the command is a bare name.
Meeting language is detected once per channel, over its first 30 s of
speech, so pt-BR and English meetings both work. Raw recordings stay under
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
the engines from `~/.config/voice-at/tray.json`. Without it, a tray built
from source runs LARIS from the checkout it was built in (once that checkout
has its `.venv`); a release download assumes `~/code/voxis`. MIRIS defaults
to `~/.local/bin/miris`. Adjust that file if yours differ.

### Start VOXIS at login (macOS)

System Settings > General > Login Items > "+" > `/Applications/VOXIS.app`.

VOXIS loads the dictation model at startup (~3.6s, 276 MB resident), so with
this enabled the model is ready before you ever press the dictation key.

---

## Linux (GNOME on Wayland)

Dictation, the tray and MIRIS (section 6). Tested on Ubuntu 26.04,
GNOME 50 on Wayland, PipeWire, an NVIDIA RTX 4070 Ti.

Wayland gives no app global key events, so there is no push-to-talk. A GNOME
shortcut toggles hands-free: silero-vad cuts each utterance on silence,
faster-whisper transcribes it (CUDA float16 with an NVIDIA GPU, CPU int8
otherwise) and the text is pasted into the focused app with Shift+Insert.

Set the checkout path once; every command below uses it:

```bash
VOXIS_DIR="$HOME/code/voxis"   # wherever you cloned
```

### System packages

```bash
sudo apt install wl-clipboard libportaudio2 python3-gi gir1.2-webkit2-4.1
# only to build the tray
sudo apt install build-essential libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev
```

Also: [uv](https://docs.astral.sh/uv/), Node.js 20+ and Rust stable. For the
GPU, only the NVIDIA driver: the CUDA 12 cuBLAS and cuDNN 9 libraries come
from pip wheels, no CUDA toolkit needed.

- `wl-clipboard`: the text reaches the app through the clipboard.
- `python3-gi` and `gir1.2-webkit2-4.1` (GTK 3, WebKit2 4.1): the orb. It runs
  on the system `/usr/bin/python3`, not the venv (PyGObject does not build
  there), as an X11 popup under XWayland so it stays on top without taking
  focus. Without them dictation still works, with no orb.

### 1. LARIS venv (Python 3.12)

```bash
cd "$VOXIS_DIR/packages/laris"
uv venv --python 3.12 .venv
# CPU torch first: only silero-vad needs torch, and the default PyPI wheel
# pulls gigabytes of CUDA libraries faster-whisper never uses
uv pip install --python .venv torch torchaudio \
  --index-url https://download.pytorch.org/whl/cpu
uv pip install --python .venv -r requirements-linux.txt -e .
```

Use Python 3.12, the version the port runs on, not the system 3.14.
`requirements-linux.txt` brings `nvidia-cublas-cu12` and `nvidia-cudnn-cu12`;
LARIS preloads them before the first inference. Check the GPU is seen:

```bash
.venv/bin/python -c "import ctranslate2; print(ctranslate2.get_cuda_device_count())"
```

`0` means LARIS falls back to CPU, slower but working. The first boot
downloads `large-v3-turbo` (~1.6 GB) into `~/.cache/huggingface`.
DeepFilterNet is not in the Linux requirements, so audio is not denoised.

### 2. Access to /dev/uinput

Wayland lets no client inject keys, so the paste keystroke goes through a
uinput virtual keyboard. Your user needs read-write on `/dev/uinput`:

```bash
getfacl /dev/uinput   # expect a line "user:<you>:rw-"
```

On the reference machine that access comes **only** from the `steam-devices`
package: its rule `/usr/lib/udev/rules.d/60-steam-input.rules` tags uinput
`uaccess`, so logind grants the logged-in user an ACL. Remove that package
and the paste stops: the text stays on the clipboard and `laris.log` says
`paste keystroke failed ... check rw on /dev/uinput`.

Without `steam-devices`, install the same rule on its own:

```bash
echo 'KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"' \
  | sudo tee /etc/udev/rules.d/70-voxis-uinput.rules
sudo udevadm control --reload-rules
sudo udevadm trigger --sysname-match=uinput
```

`uaccess` lets any process of the logged-in user send input events, which is
the trade-off `steam-devices` already makes. Do not add yourself to the
`input` group instead: that also lets every process read every keyboard.

### 3. The GNOME shortcut

Settings > Keyboard > View and Customize Shortcuts > Custom Shortcuts > "+":

- **Name:** `VOXIS`
- **Command:** the venv Python, by absolute path, for example
  `/home/you/code/voxis/packages/laris/.venv/bin/python -m laris toggle`.
  GNOME does not run it through your shell, and Ubuntu has no `python` on
  PATH.
- **Shortcut:** Super+C.

The same from a terminal (this **replaces** any custom shortcuts you already
have; use Settings if you do):

```bash
KEY=/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/custom0/
SCHEMA=org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:$KEY
gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "['$KEY']"
gsettings set "$SCHEMA" name 'VOXIS'
gsettings set "$SCHEMA" command "$VOXIS_DIR/packages/laris/.venv/bin/python -m laris toggle"
gsettings set "$SCHEMA" binding '<Super>c'
```

Tap once to start listening, tap again to stop. Tap, do not hold: GNOME
repeats a held shortcut, so a toggle less than 1 s after the previous one is
ignored.

If the shortcut does nothing but `.venv/bin/python -m laris toggle` works
from a terminal, something else grabbed the same keys (a leftover GNOME
extension did once). Check `gnome-extensions list --enabled`.

### 4. The tray

```bash
cd "$VOXIS_DIR/apps/tray"
npm install
npx tauri build --no-bundle   # binary: src-tauri/target/release/tray
```

`--no-bundle`: there is no Linux package yet, the binary runs where it was
built. It runs LARIS from that same checkout's venv (step 1), so no config is
needed. To run another checkout, set it in `~/.config/voice-at/tray.json`:

```json
{ "laris_dir": "/home/you/code/voxis/packages/laris" }
```

The tray starts the daemon (`<laris_dir>/.venv/bin/python -u -m laris`) and
logs it to `~/.config/voice-at/laris.log`. Its icon turns red while dictation
listens; showing it needs the AppIndicator extension, enabled by default on
Ubuntu. Do not also run `python -m laris` by hand: a second daemon refuses to
start while one holds port 8421.

### 5. Start at login

An XDG autostart entry. `Exec` takes an absolute path (no `~`), so this
heredoc expands `$VOXIS_DIR` when it writes the file:

```bash
mkdir -p ~/.config/autostart
cat > ~/.config/autostart/voxis-tray.desktop <<EOF
[Desktop Entry]
Type=Application
Name=VOXIS
Comment=VOXIS tray: owns the LARIS dictation daemon
Exec=$VOXIS_DIR/apps/tray/src-tauri/target/release/tray
Icon=$VOXIS_DIR/apps/tray/src-tauri/icons/128x128.png
Terminal=false
NoDisplay=true
X-GNOME-Autostart-enabled=true
EOF
desktop-file-validate ~/.config/autostart/voxis-tray.desktop
```

GNOME runs it as the user unit `app-voxis\x2dtray@autostart.service`. To
restart the tray (after a rebuild, say) and read its output:

```bash
systemctl --user restart 'app-voxis\x2dtray@autostart.service'
journalctl --user -u 'app-voxis\x2dtray@autostart.service'
```

The entry runs the binary inside `target/release`, so a rebuild changes what
starts at the next login.

### 6. MIRIS - meeting recorder

The microphone is one channel; the other is everything the speakers play,
recorded by `pw-record` (package `pipewire-bin`, installed with PipeWire on
Ubuntu) from the default output's monitor. Transcription runs faster-whisper,
like dictation, in a venv of its own:

```bash
cd "$VOXIS_DIR/packages/miris"
uv venv --python 3.12 .venv
uv pip install --python .venv torch torchaudio \
  --index-url https://download.pytorch.org/whl/cpu
uv pip install --python .venv -r requirements-linux.txt
mkdir -p ~/.local/bin
ln -sf "$PWD/.venv/bin/miris" ~/.local/bin/miris
```

`~/.local/bin/miris` is where the tray looks for it. The first run creates
`~/.miris/config.toml`: set `vault_root` to your notes vault and give
`llm_command` the CLI's full path, for example
`["/home/you/.local/bin/agy", "--model", "Gemini 3.8 Flash (Medium)", "-p"]`
(the same file as on macOS, see section 2). The tray started at login has no
`~/.local/bin` on its PATH, so a bare `agy` fails every meeting with "No such
file or directory". Then:

```bash
miris doctor
```

expects `[ok]` for `pw-record`, the vault, the LLM command and both capture
channels, and no `[WARN] llm_command`. Start and stop from the tray's "Gravar reunião", or with `miris
start` / `miris stop`.

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
- Linux: also `~/.config/autostart/voxis-tray.desktop`, the VOXIS custom
  shortcut, and `/etc/udev/rules.d/70-voxis-uinput.rules` if you added it.
