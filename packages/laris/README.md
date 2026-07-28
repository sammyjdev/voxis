# LARIS

Local Audio Recognition & Input System - the mute transcriber core behind
[claude-code-ops](https://github.com/sammyjdev/claude-code-ops)'s voice
input: push-to-talk (hold a key) or hands-free (double-tap) dictation into
whatever app is focused, via local `mlx-whisper` STT. No TTS, no LLM - see
[KIRIS](https://github.com/sammyjdev/kiris) for the voice-assistant layer
built on top.

## Running standalone

```bash
python3.11 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
python -m laris
```

Standalone mode is dictation-only (no tmux session targeting - that needs a
`VoiceSession` wired to a real session backend, which is what
`claude-code-ops` does).

## Orb themes

The desktop orb is themeable: 3 archetypes (waveform - default, reactor,
faceted) x 4 palettes (blade - default, noir, acid, ember), plus position
presets (center by default) and size. Config lives in
`~/.config/voice-at/orb-theme.json` and can be changed live:

```bash
curl -X POST "http://127.0.0.1:8421/theme?token=$TOK" \
     -H 'Content-Type: application/json' \
     -d '{"archetype": "waveform", "palette": "blade", "position": "center"}'
```

Visual dev/smoke without the daemon: open `/overlay?demo=1` on the overlay
server (fake voice cycle, no mic needed).

## Testing

```bash
pytest -q
```
