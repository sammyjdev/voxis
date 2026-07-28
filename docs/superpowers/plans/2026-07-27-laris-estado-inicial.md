# LARIS Honest Startup State Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the tray report dictation readiness truthfully, keep the model
loaded across the whole session instead of reloading it on every toggle, and
give Whisper the vocabulary context that short dictated sentences lack.

**Architecture:** The laris daemon gains a `ready` flag on `app.state`, flipped
to `True` only after `PushToTalk.start()` returns, and exposed through the
existing `/theme` endpoint. The tray reads it in the 2s poll it already runs and
renders a status line instead of a toggle - the dictation on/off switch is
removed entirely, since `cmd_r` is already the control. The daemon spawns once
in `setup()` and is stopped only by "Sair".

**Tech Stack:** Python 3.11 (FastAPI, pytest) for laris; Rust (Tauri v2.11.5,
cargo test) for the tray; TypeScript for the settings window.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-27-laris-estado-inicial-design.md`
- STT model stays `mlx-community/whisper-large-v3-turbo-q4`. Do not change
  `config.STT_MODEL`.
- The orb gains no `loading` phase - that was cut in the design.
- `warm()` stays blocking on the daemon's main path. An earlier draft moved it
  to a thread; with the orb state cut there is no reason to, and a thread would
  turn a load failure into a silent hang.
- `save_laris_settings` must keep its `stop()` + `spawn()` cycle: laris reads
  settings once at import, so applying them requires a restart.
- Menu strings are pt-BR, matching the existing menu. Code, comments and commit
  messages are English.
- Every task ends green: `pytest tests/ -q` in `packages/laris`, `cargo test
  --lib` in `apps/tray/src-tauri`.
- Run python via `.venv/bin/python` - the venv's `activate` is not sourced.

## File Structure

| File | Responsibility | Change |
|------|----------------|--------|
| `packages/laris/laris/daemon.py` | PTT voice loop | Modify: `run()` takes `on_ready` |
| `packages/laris/laris/__main__.py` | Standalone entrypoint | Modify: seed `ready=False`, pass `on_ready`, log the model |
| `packages/laris/laris/server.py` | HTTP surface | Modify: `/theme` returns `ready` |
| `packages/laris/laris/transcriber.py` | mlx-whisper STT | Modify: pass `initial_prompt` |
| `packages/laris/laris/config.py` | Config layer | Modify: add `INITIAL_PROMPT` |
| `packages/laris/laris/settings.py` | Settings file reader | Modify: add key to `KNOWN_KEYS` |
| `apps/tray/src-tauri/src/lib.rs` | Tray, menu, poll loop | Modify: `Snapshot.laris_ready`, status item, spawn in setup |
| `apps/tray/src-tauri/src/commands.rs` | Tauri commands | Modify: allow `INITIAL_PROMPT` |
| `apps/tray/src/main.ts` | Settings window | Modify: prompt field |

---

### Task 1: Daemon reports readiness

**Files:**
- Modify: `packages/laris/laris/daemon.py:122-150`
- Modify: `packages/laris/laris/__main__.py:56-115`
- Modify: `packages/laris/laris/server.py:47-58`
- Test: `packages/laris/tests/test_daemon.py`, `packages/laris/tests/test_server.py`

**Interfaces:**
- Produces: `daemon.run(..., on_ready: Callable[[], None] | None = None)` - called
  once, after `PushToTalk.start()` returns and before the PTT loop begins.
- Produces: `GET /theme` response gains `"ready": bool`. Absent field means an
  old daemon; consumers must default to `True`.

- [ ] **Step 1: Write the failing test for `on_ready` ordering**

Add to `packages/laris/tests/test_daemon.py`:

```python
def test_run_signals_ready_only_after_push_to_talk_started(monkeypatch):
    """The tray shows "Ditado ativo" off this signal. Firing it before
    ptt.start() would reintroduce the exact lie this work removes: a menu
    claiming dictation works while cmd_r does nothing."""
    from laris import daemon

    events = []

    class _FakeBackend:
        def warm(self):
            events.append("warm")

        def transcribe(self, audio, sample_rate):
            return ""

        def type_text(self, text):
            pass

        def make_overlay(self, url):
            return MagicMock()

        def make_ducker(self):
            return MagicMock()

    class _FakePtt:
        def __init__(self, key):
            self.pressed = MagicMock()
            self.hands_free = MagicMock()

        def start(self):
            events.append("ptt.start")

    class _StopTheLoop(Exception):
        pass

    def _stop(*_args, **_kwargs):
        raise _StopTheLoop

    monkeypatch.setattr("laris.platform.get_backend", lambda: _FakeBackend())
    monkeypatch.setattr("laris.hotkey.PushToTalk", _FakePtt)
    monkeypatch.setattr(daemon, "_wait_for_ptt", _stop)

    try:
        daemon.run(
            make_voice_session=lambda: MagicMock(),
            on_ready=lambda: events.append("ready"),
        )
    except _StopTheLoop:
        pass

    assert events == ["warm", "ptt.start", "ready"]
```

- [ ] **Step 2: Run it and watch it fail**

Run: `.venv/bin/python -m pytest tests/test_daemon.py::test_run_signals_ready_only_after_push_to_talk_started -v`
Expected: FAIL with `TypeError: run() got an unexpected keyword argument 'on_ready'`

- [ ] **Step 3: Add the parameter to `run()`**

In `packages/laris/laris/daemon.py`, change the signature:

```python
def run(
    make_voice_session: Callable[[], VoiceSession],
    make_assistant: Callable[[], AssistantProtocol | None] = lambda: None,
    agent_phrase_matcher: Callable[[str], str | None] | None = None,
    kiris_dispatch: Callable[[str], bool] | None = None,
    voice_queue=None,
    overlay_url: str = "",
    on_ready: Callable[[], None] | None = None,
) -> None:
```

Then, immediately after the existing `ptt.start()` line and before the
`vad = None` line, add:

```python
    # only now is dictation real: the model is loaded and the key listener is
    # attached. The tray polls this to stop claiming "Ditado" during the ~3.6s
    # it takes to get here.
    if on_ready is not None:
        on_ready()
```

- [ ] **Step 4: Run the test and watch it pass**

Run: `.venv/bin/python -m pytest tests/test_daemon.py::test_run_signals_ready_only_after_push_to_talk_started -v`
Expected: PASS

- [ ] **Step 5: Write the failing test for `/theme`**

Add to `packages/laris/tests/test_server.py`:

```python
def test_theme_reports_ready_false_until_the_daemon_says_otherwise():
    from fastapi.testclient import TestClient
    from laris.server import build_app

    app = build_app(token="t")
    app.state.ready = False
    client = TestClient(app)

    assert client.get("/theme?token=t").json()["ready"] is False

    app.state.ready = True
    assert client.get("/theme?token=t").json()["ready"] is True


def test_theme_defaults_ready_to_true_when_unset():
    """An older daemon never sets the flag; treating that as "not ready"
    would leave the tray stuck on "Carregando modelo..." forever."""
    from fastapi.testclient import TestClient
    from laris.server import build_app

    client = TestClient(build_app(token="t"))

    assert client.get("/theme?token=t").json()["ready"] is True
```

- [ ] **Step 6: Run and watch it fail**

Run: `.venv/bin/python -m pytest tests/test_server.py -k ready -v`
Expected: FAIL with `KeyError: 'ready'`

- [ ] **Step 7: Add `ready` to the `/theme` payload**

In `packages/laris/laris/server.py`, in the `get_theme` return dict, add one
entry after `ptt_trusted`:

```python
        return {
            "theme": orb_theme.load_theme(),
            "palettes": orb_theme.PALETTES,
            "ptt_trusted": getattr(app.state, "ptt_trusted", True),
            # False while the model loads. Same default-permissive rule as
            # ptt_trusted: an old daemon that never sets it reads as ready.
            "ready": getattr(app.state, "ready", True),
        }
```

- [ ] **Step 8: Run and watch it pass**

Run: `.venv/bin/python -m pytest tests/test_server.py -k ready -v`
Expected: PASS (2 tests)

- [ ] **Step 9: Wire the entrypoint**

In `packages/laris/laris/__main__.py`, right after the existing
`app.state.ptt_trusted = PushToTalk.check_trusted()` line, add:

```python
    # uvicorn binds the port before the model loads, so /theme answers long
    # before dictation works. Start explicitly not-ready.
    app.state.ready = False
```

Then in the `daemon.run(...)` call at the end of `main()`, add the argument:

```python
        overlay_url=f"http://127.0.0.1:{LARIS_PORT}/overlay?token={laris_token}",
        on_ready=lambda: setattr(app.state, "ready", True),
    )
```

- [ ] **Step 10: Run the whole suite**

Run: `.venv/bin/python -m pytest tests/ -q`
Expected: PASS, 183 tests (180 existing + 3 new)

- [ ] **Step 11: Commit**

```bash
git add packages/laris/laris/daemon.py packages/laris/laris/__main__.py \
        packages/laris/laris/server.py packages/laris/tests/test_daemon.py \
        packages/laris/tests/test_server.py
git commit -m "feat(laris): report readiness only once push-to-talk is listening

uvicorn binds :8421 before the model import and roughly 3.6s before
ptt.start() attaches the key listener, so a live port proved nothing about
dictation working. Expose an explicit ready flag over /theme, flipped after
PushToTalk.start() returns.

Absent field defaults to true so an older daemon does not read as permanently
loading."
```

---

### Task 2: Tray reads readiness instead of liveness

**Files:**
- Modify: `apps/tray/src-tauri/src/lib.rs:34-42` (`Snapshot`), `:58-96` (`take_snapshot`)
- Test: `apps/tray/src-tauri/src/lib.rs` (`mod tests`)

**Interfaces:**
- Consumes: `GET /theme` field `ready` from Task 1.
- Produces: `Snapshot.laris_ready: bool` and
  `fn dictation_status_label(alive: bool, ready: bool) -> &'static str`, used by
  `build_menu` in Task 3.

- [ ] **Step 1: Write the failing test**

Add inside `mod tests` in `apps/tray/src-tauri/src/lib.rs`:

```rust
#[test]
fn dictation_label_distinguishes_loading_from_ready() {
    // The whole point of this work: "Ditado" must not appear while the model
    // is still loading, because cmd_r does nothing during that window.
    assert_eq!(dictation_status_label(true, true), "Ditado ativo");
    assert_eq!(dictation_status_label(true, false), "Carregando modelo...");
    assert_eq!(dictation_status_label(false, false), "Ditado indisponível");
    // a dead daemon is never ready, whatever a stale payload claims
    assert_eq!(dictation_status_label(false, true), "Ditado indisponível");
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test --lib dictation_label`
Expected: FAIL - `cannot find function 'dictation_status_label' in this scope`

- [ ] **Step 3: Add the field and the label function**

In `apps/tray/src-tauri/src/lib.rs`, add a field to `Snapshot` after
`ptt_trusted`:

```rust
    laris_ready: bool, // false = daemon up, model still loading
```

Add this function next to `should_warn_untrusted`:

```rust
/// Liveness is not readiness: uvicorn answers on :8421 seconds before the
/// model is loaded and the key listener attached.
fn dictation_status_label(alive: bool, ready: bool) -> &'static str {
    match (alive, ready) {
        (false, _) => "Ditado indisponível",
        (true, false) => "Carregando modelo...",
        (true, true) => "Ditado ativo",
    }
}
```

In `take_snapshot`, read the flag next to the existing `ptt_trusted` read -
before `theme_body` is consumed by the `theme` binding:

```rust
    let laris_ready = theme_body
        .as_ref()
        .and_then(|b| b.get("ready")?.as_bool())
        .unwrap_or(true);
```

Add `laris_ready,` to the `Snapshot { .. }` literal returned at the end of
`take_snapshot`.

- [ ] **Step 4: Run and watch it pass**

Run: `cargo test --lib dictation_label`
Expected: PASS

- [ ] **Step 5: Run the whole suite**

Run: `cargo test --lib`
Expected: PASS, 28 tests

- [ ] **Step 6: Commit**

```bash
git add apps/tray/src-tauri/src/lib.rs
git commit -m "feat(tray): track laris readiness separately from liveness

take_snapshot decided the menu state from laris::alive(), which only proves
the HTTP port answered. Read the ready flag the daemon now publishes."
```

---

### Task 3: Replace the dictation toggle with a status line

**Files:**
- Modify: `apps/tray/src-tauri/src/lib.rs:99-147` (`build_menu`), `:266-287` (`handle_menu`)
- Test: `apps/tray/src-tauri/src/lib.rs` (`mod tests`)

**Interfaces:**
- Consumes: `dictation_status_label` and `Snapshot.laris_ready` from Task 2.

Dictation is always on. `cmd_r` is the control, so a second on/off switch buys
nothing and costs a full model reload per flip.

- [ ] **Step 1: Write the failing test**

Add inside `mod tests`:

```rust
#[test]
fn hands_free_is_only_offered_once_the_model_is_loaded() {
    // toggling hands-free against a daemon that has not finished warming
    // silently does nothing, which is the failure mode being removed
    assert!(!hands_free_enabled(true, false));
    assert!(hands_free_enabled(true, true));
    assert!(!hands_free_enabled(false, true));
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test --lib hands_free_is_only`
Expected: FAIL - `cannot find function 'hands_free_enabled' in this scope`

- [ ] **Step 3: Add the helper**

Next to `dictation_status_label` in `apps/tray/src-tauri/src/lib.rs`:

```rust
fn hands_free_enabled(alive: bool, ready: bool) -> bool {
    alive && ready
}
```

- [ ] **Step 4: Run and watch it pass**

Run: `cargo test --lib hands_free_is_only`
Expected: PASS

- [ ] **Step 5: Swap the toggle for a status item**

In `build_menu`, replace the `dictation` binding:

```rust
    // status, not a switch: dictation is always on and cmd_r is the control.
    // id "noop" is already the menu's inert id (see the processing branch).
    let dictation = MenuItem::with_id(
        app,
        "noop",
        dictation_status_label(snap.laris_alive, snap.laris_ready),
        false,
        None::<&str>,
    )?;
```

Change the `hands_free` binding's enabled argument from `snap.laris_alive` to
the helper:

```rust
    let hands_free = CheckMenuItem::with_id(
        app, "hands_free", "Hands-free",
        hands_free_enabled(snap.laris_alive, snap.laris_ready),
        snap.hands_free, None::<&str>,
    )?;
```

Change the theme submenu's enabled argument on the same rule:

```rust
    let theme_menu = Submenu::with_id_and_items(
        app, "theme", "Tema do orb",
        hands_free_enabled(snap.laris_alive, snap.laris_ready),
        &refs,
    )?;
```

- [ ] **Step 6: Delete the toggle handler**

In `handle_menu`, delete the entire `"dictation" => { ... }` arm
(`lib.rs:266-287`), including its `std::thread::spawn` body. Removing it makes
the `laris::read_peer`/`peers_path` imports possibly unused in that scope -
if the compiler warns, remove only the imports your deletion orphaned.

- [ ] **Step 7: Build and run the suite**

Run: `cargo test --lib`
Expected: PASS, 29 tests, no warnings about unused imports

- [ ] **Step 8: Commit**

```bash
git add apps/tray/src-tauri/src/lib.rs
git commit -m "feat(tray): dictation is always on; the menu shows state, not a switch

The Ditado item was a process switch - spawn on check, SIGTERM on uncheck - so
the ~3.6s model load was paid on every single toggle. cmd_r is already the
control, so the switch bought nothing.

It now renders the daemon's actual state, and hands-free plus the theme submenu
are enabled only once the model is loaded, instead of accepting clicks that
silently did nothing."
```

---

### Task 4: Daemon persists for the whole session

**Files:**
- Modify: `apps/tray/src-tauri/src/lib.rs:342-380` (`setup`)
- Test: manual - the tray's app lifecycle has no unit-test seam

**Interfaces:**
- Consumes: nothing new.
- Produces: the daemon is running before the first poll tick.

- [ ] **Step 1: Spawn the daemon during setup**

In `lib.rs`, inside `.setup(|app| { ... })`, immediately after
`app.manage(AppState { ... });`, add:

```rust
            // The model costs ~3.6s and 276MB to load. Loading it once per
            // session beats reloading it per toggle; "Sair" is what releases it.
            {
                let state = app.state::<AppState>();
                let mut daemon = state.daemon.lock().unwrap();
                if let Err(e) = daemon.spawn() {
                    // a bad laris_dir would otherwise fail silently at boot -
                    // the old code path surfaced this from the menu handler
                    notify(app.handle(), &format!("Falha ao iniciar ditado: {e}"));
                }
            }
```

- [ ] **Step 2: Verify it compiles and the suite is green**

Run: `cargo test --lib`
Expected: PASS, 29 tests

- [ ] **Step 3: Verify on the real app**

```bash
cd apps/tray && npm run tauri build -- --bundles app
osascript -e 'tell application "VOXIS" to quit'; pkill -f "VOXIS.app/Contents/MacOS/tray"; pkill -f "\-m laris"
rm -rf /Applications/VOXIS.app
cp -R src-tauri/target/release/bundle/macos/VOXIS.app /Applications/VOXIS.app
open -a VOXIS
```

Then confirm all three:
1. `pgrep -fl "\-m laris"` shows the daemon within seconds, without touching the menu
2. The menu shows "Carregando modelo..." at first, then "Ditado ativo"
3. `grep "\[Hotkey\]" ~/.config/voice-at/laris.log` shows push-to-talk active

- [ ] **Step 4: Commit**

```bash
git add apps/tray/src-tauri/src/lib.rs
git commit -m "feat(tray): start the laris daemon at tray startup

The daemon used to be spawned by the dictation toggle, which no longer exists.
Starting it in setup() loads the model once per session instead of once per
toggle, and a spawn failure notifies instead of failing silently at boot."
```

---

### Task 5: Boot log states the active model

**Files:**
- Modify: `packages/laris/laris/__main__.py:99-106`
- Test: none - this is a print statement; asserting on log text pins prose

- [ ] **Step 1: Extend the boot line**

In `packages/laris/laris/__main__.py`, replace the existing `print(...)` that
reports the listening daemon with:

```python
    print(
        f"[Laris] daemon ouvindo em http://127.0.0.1:{LARIS_PORT} - "
        f"panel={'on' if panel_url else 'off'}, "
        f"kiris={'on' if kiris_online else 'off'}, "
        f"PTT: {config.DICTATE_KEY}"
    )
    # answering "which model is running?" should not require reading config.py
    print(
        f"[Laris] STT: {config.STT_MODEL} "
        f"(lang={config.STT_LANGUAGE or 'auto'}, "
        f"denoise={config.NOISE_REDUCTION})"
    )
```

- [ ] **Step 2: Verify by hand**

```bash
cd packages/laris && .venv/bin/python -u -m laris 2>&1 | head -12
```

Expected: a line reading
`[Laris] STT: mlx-community/whisper-large-v3-turbo-q4 (lang=pt, denoise=on)`.
Stop it with Ctrl-C.

- [ ] **Step 3: Run the suite**

Run: `.venv/bin/python -m pytest tests/ -q`
Expected: PASS, 183 tests

- [ ] **Step 4: Commit**

```bash
git add packages/laris/laris/__main__.py
git commit -m "feat(laris): log the active STT model, language and denoise at boot

Answering 'which model is running?' meant reading config.py and reasoning about
the settings-file override layer. The log now says it outright."
```

---

### Task 6: initial_prompt for domain vocabulary

**Files:**
- Modify: `packages/laris/laris/config.py:35-40`
- Modify: `packages/laris/laris/settings.py:8`
- Modify: `packages/laris/laris/transcriber.py:51-63`
- Modify: `apps/tray/src-tauri/src/commands.rs:9-14`, `:24-33`
- Modify: `apps/tray/src/main.ts:52-63`, `:97-103`
- Test: `packages/laris/tests/test_transcriber.py` (create), `apps/tray/src-tauri/src/commands.rs`

**Interfaces:**
- Produces: `config.INITIAL_PROMPT: str` (empty string = feature off) and the
  settings key `INITIAL_PROMPT`, accepted by `save_laris_settings`.

An ICAART 2026 study on pt-BR measured 3-4x higher WER on 1-5 word sentences
than on 11+ word ones. Dictated prompts sit in that band, and the cause is
missing context - which is what `initial_prompt` supplies.

- [ ] **Step 1: Write the failing test**

Create `packages/laris/tests/test_transcriber.py`:

```python
from unittest.mock import MagicMock

import numpy as np
import pytest


def _transcriber(monkeypatch, prompt):
    """Build a Transcriber with mlx_whisper stubbed out (importing the real
    one costs 1.6s and requires Apple Silicon)."""
    from laris import config, transcriber

    fake_mlx = MagicMock()
    fake_mlx.transcribe.return_value = {"text": " ok "}
    monkeypatch.setattr(config, "INITIAL_PROMPT", prompt)

    t = transcriber.Transcriber.__new__(transcriber.Transcriber)
    t._mlx_whisper = fake_mlx
    t.model = "test-model"
    return t, fake_mlx


def test_initial_prompt_is_forwarded_when_configured(monkeypatch):
    t, fake_mlx = _transcriber(monkeypatch, "PortAudio, worktree, AXON")

    t.transcribe(np.zeros(16000, dtype=np.float32))

    assert fake_mlx.transcribe.call_args.kwargs["initial_prompt"] == (
        "PortAudio, worktree, AXON"
    )


def test_no_initial_prompt_is_sent_when_unset(monkeypatch):
    """Passing an empty string is not the same as passing nothing: Whisper
    would prime on it and can echo it into the transcript."""
    t, fake_mlx = _transcriber(monkeypatch, "")

    t.transcribe(np.zeros(16000, dtype=np.float32))

    assert "initial_prompt" not in fake_mlx.transcribe.call_args.kwargs
```

- [ ] **Step 2: Run it and watch it fail**

Run: `.venv/bin/python -m pytest tests/test_transcriber.py -v`
Expected: FAIL - `AttributeError: <module 'laris.config'> does not have the attribute 'INITIAL_PROMPT'`

- [ ] **Step 3: Add the config key**

In `packages/laris/laris/config.py`, after the `STT_LANGUAGE` definition:

```python
# Primes Whisper's decoder with domain vocabulary. Dictated prompts are short,
# and an ICAART 2026 pt-BR study measured 3-4x higher WER on 1-5 word sentences
# than on 11+ word ones - the cause is missing context. Empty = not sent.
INITIAL_PROMPT: str = _setting("INITIAL_PROMPT", "")
```

In `packages/laris/laris/settings.py`, extend `KNOWN_KEYS`:

```python
KNOWN_KEYS = (
    "ASSISTANT_MODE", "DICTATE_KEY", "STT_MODEL", "NOISE_REDUCTION",
    "INITIAL_PROMPT",
)
```

- [ ] **Step 4: Forward it in the transcriber**

In `packages/laris/laris/transcriber.py`, replace the body of `transcribe`:

```python
    def transcribe(self, audio: np.ndarray, sample_rate: int = config.SAMPLE_RATE) -> str:
        if len(audio) == 0:
            return ""
        # omitted entirely when unset: an empty initial_prompt still primes the
        # decoder and can leak into the transcript
        extra = (
            {"initial_prompt": config.INITIAL_PROMPT}
            if config.INITIAL_PROMPT
            else {}
        )
        result = self._mlx_whisper.transcribe(
            audio.astype(np.float32),
            path_or_hf_repo=self.model,
            language=config.STT_LANGUAGE or None,  # "" -> auto-detect
            task="transcribe",
            word_timestamps=False,   # do NOT change: known memory leak with True
            fp16=True,
            condition_on_previous_text=False,
            **extra,
        )
        return (result.get("text") or "").strip()
```

- [ ] **Step 5: Run and watch it pass**

Run: `.venv/bin/python -m pytest tests/test_transcriber.py -v`
Expected: PASS (2 tests)

- [ ] **Step 6: Write the failing tray test**

`apps/tray/src-tauri/src/commands.rs` has no test module yet. Append one to the
end of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_prompt_is_an_accepted_setting() {
        assert!(valid_value("INITIAL_PROMPT", &serde_json::json!("PortAudio, AXON")));
        // empty is how the user turns it off, so it must round-trip
        assert!(valid_value("INITIAL_PROMPT", &serde_json::json!("")));
        assert!(!valid_value("INITIAL_PROMPT", &serde_json::json!(42)));
        assert!(KNOWN_LARIS_KEYS.contains(&"INITIAL_PROMPT"));
    }
}
```

- [ ] **Step 7: Run and watch it fail**

Run: `cargo test --lib initial_prompt_is_an_accepted`
Expected: FAIL - the assertion on `valid_value` returns false

- [ ] **Step 8: Accept the key in the tray**

In `apps/tray/src-tauri/src/commands.rs`:

```rust
const KNOWN_LARIS_KEYS: [&str; 5] = [
    "ASSISTANT_MODE",
    "DICTATE_KEY",
    "STT_MODEL",
    "NOISE_REDUCTION",
    "INITIAL_PROMPT",
];
```

And in `valid_value`, add an arm before the `_ => false` catch-all:

```rust
        // empty is meaningful: it is how the user disables priming
        "INITIAL_PROMPT" => s.chars().count() <= 900,
```

- [ ] **Step 9: Run and watch it pass**

Run: `cargo test --lib initial_prompt_is_an_accepted`
Expected: PASS

- [ ] **Step 10: Add the settings field**

In `apps/tray/src/main.ts`, alongside the existing `#stt-model` read (line ~62):

```ts
  $<HTMLTextAreaElement>("#initial-prompt").value = s.laris.INITIAL_PROMPT ?? "";
```

And in the save payload, next to `STT_MODEL` (line ~101):

```ts
          INITIAL_PROMPT: $<HTMLTextAreaElement>("#initial-prompt").value.trim(),
```

Add the control to `apps/tray/index.html`, directly after the existing
`<input type="text" id="stt-model" spellcheck="false" />` at line 21, matching
the surrounding label/field markup:

```html
<label for="initial-prompt">Vocabulário (initial prompt)</label>
<textarea id="initial-prompt" rows="3" spellcheck="false"
  placeholder="PortAudio, worktree, AXON, Tauri"></textarea>
```

- [ ] **Step 11: Verify end to end**

```bash
cd apps/tray && npm run build && npm run tauri build -- --bundles app
```

Install, open Configurações, set the vocabulary field, save. Then confirm laris
picked it up:

```bash
grep INITIAL_PROMPT ~/.config/voice-at/laris-settings.json
```

Dictate a sentence containing one of the terms and check the transcript in
`~/.config/voice-at/laris.log`.

- [ ] **Step 12: Run both suites**

Run: `.venv/bin/python -m pytest tests/ -q` and `cargo test --lib`
Expected: PASS, 185 laris + 30 tray

- [ ] **Step 13: Commit**

```bash
git add packages/laris/laris/config.py packages/laris/laris/settings.py \
        packages/laris/laris/transcriber.py packages/laris/tests/test_transcriber.py \
        apps/tray/src-tauri/src/commands.rs apps/tray/src/main.ts
git commit -m "feat(laris): optional initial_prompt to prime domain vocabulary

An ICAART 2026 pt-BR study measured 3-4x higher WER on 1-5 word sentences than
on 11+ word ones; dictated prompts sit in that band and the cause is missing
context. initial_prompt supplies it.

Omitted entirely when empty - passing an empty string still primes the decoder
and can leak into the transcript.

No WER harness backs this, which is why it is a reversible setting rather than a
default. CORAA ASR v1.1 is the instrument if it ever needs measuring."
```

---

### Task 7: Document autostart

**Files:**
- Modify: `docs/INSTALL.md`
- Modify: `docs/BACKLOG.md:71`

macOS already provides this. Shipping a `tauri-plugin-autostart` dependency to
replace two clicks is not worth it until the app is distributed to people who
will not find Login Items.

- [ ] **Step 1: Document the manual step**

Add to `docs/INSTALL.md`, in the macOS section:

```markdown
### Start VOXIS at login (macOS)

System Settings > General > Login Items > "+" > `/Applications/VOXIS.app`.

VOXIS loads the dictation model at startup (~3.6s, 276 MB resident), so with
this enabled the model is ready before you ever press the dictation key.
```

- [ ] **Step 2: Note the tradeoff in the backlog**

Replace the existing line at `docs/BACKLOG.md:71`:

```markdown
- [ ] **Tray autostart on login** (macOS/Windows). v1.1 nicety. macOS users can
      already do this via Login Items (documented in INSTALL.md); this item is
      only about shipping it as an in-app toggle, which costs a
      `tauri-plugin-autostart` dependency.
```

- [ ] **Step 3: Commit**

```bash
git add docs/INSTALL.md docs/BACKLOG.md
git commit -m "docs: how to start VOXIS at login, and why it is not in-app yet"
```

---

## Verification

After every task:

```bash
cd packages/laris && .venv/bin/python -m pytest tests/ -q
cd apps/tray/src-tauri && cargo test --lib
```

Final acceptance, on the real app:

1. Launch VOXIS. The daemon starts without touching the menu.
2. The menu reads "Carregando modelo..." and becomes "Ditado ativo".
3. There is no dictation on/off switch.
4. `cmd_r` transcribes; `~/.config/voice-at/laris.log` shows the `[Voice]` line.
5. Closing Configurações leaves VOXIS running; "Sair" quits and stops the daemon.
6. The boot log states the model, language and denoise state.
