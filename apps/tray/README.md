# VOXIS Tray

Cross-platform tray app (Tauri v2) for LARIS dictation and MIRIS meeting
recording.

Windows release installers bundle a frozen LARIS push-to-talk engine, so they
need no system Python. The `small` Whisper model downloads on first use. CPU
int8 is included; CUDA is a manual upgrade. MIRIS remains macOS-only.

## Dev

    npm install
    npm run tauri dev

## Build installers

    npm run tauri build        # bundles under src-tauri/target/release/bundle/

Unsigned: macOS Gatekeeper requires right-click > Open on first launch.
Signing/notarization is documented as out of scope for v1.

## Config files

| File | Owner | Purpose |
|------|-------|---------|
| ~/.config/voice-at/tray.json | tray | absolute paths: miris_bin, laris_dir, miris_home |
| ~/.config/voice-at/laris-settings.json | tray | laris overrides (file > env > default), read at daemon start |
| ~/.config/voice-at/peers.json | laris | overlay URL + token (read-only for the tray) |
| ~/.miris/config.toml | user/tray | vault, categories, llm_command (comments preserved) |

On macOS, the tray uses the local LARIS and MIRIS installations in this
repository. On Windows, it starts the bundled LARIS engine. The tray disables
controls for any engine that is unavailable.
