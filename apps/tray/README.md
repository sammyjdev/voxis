# VOXIS Tray

macOS tray app (Tauri v2) for LARIS dictation and MIRIS meeting recording.

It spawns the LARIS daemon at startup and stops it on "Sair". Dictation has no
on/off switch: `cmd_r` is the control, so the menu shows the daemon's state
("Carregando modelo..." while the model loads, then "Ditado ativo") instead of
a toggle that would reload the model on every flip.

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

The tray uses the LARIS and MIRIS installations in this repository, and
disables controls for any engine that is unavailable.
