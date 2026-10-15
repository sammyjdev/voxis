"""Standalone entrypoint: `python -m laris`. Reads peers.json, builds an
HTTP-backed VoiceSession + Kiris-HTTP dispatch callback when peers are
online, or no-op stubs when offline (pure dictation regardless).

Boots the orb overlay server on 127.0.0.1:8421 in a background thread and
runs the PTT voice loop on the calling thread."""
from __future__ import annotations

import importlib.util
import queue
import socket
import sys
import threading
import time
import urllib.request

import uvicorn

from laris import config, peer_discovery
from laris.auth import generate_token
from laris.peer_client import KirisClient, PanelClient
from laris.server import build_app
from laris.state_machine import VoiceSession


LARIS_PORT = 8421


def _wait_for_port(host: str, port: int, timeout: float = 5.0) -> bool:
    """TCP-connect poll - race-protection for Overlay.show() loading the orb
    before uvicorn binds 8421. Returns True on bind, False on timeout."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=0.2):
                return True
        except OSError:
            time.sleep(0.05)
    return False


def _daemon_already_running(http_get=urllib.request.urlopen) -> str | None:
    """URL of a laris daemon that is already up, or None.

    A second daemon fails to bind :8421 but carries on regardless: uvicorn dies
    on its background thread while the voice loop keeps its own keyboard
    listener attached. Two listeners on one key records and types every
    utterance twice. Probing the recorded peer (not just the port) keeps an
    unrelated process squatting on 8421 from blocking dictation entirely."""
    url = peer_discovery.peer_url("laris")
    token = peer_discovery.peer_token("laris")
    if not url or not token:
        return None
    try:
        with http_get(f"{url}/theme?token={token}", timeout=1.0) as resp:
            return url if 200 <= resp.status < 300 else None
    except Exception:  # noqa: BLE001 - anything but a clean 2xx means "not ours"
        return None


def _make_voice_session() -> VoiceSession:
    panel_url = peer_discovery.peer_url("panel")
    panel_token = peer_discovery.peer_token("panel")
    kiris_url = peer_discovery.peer_url("kiris")
    panel = PanelClient(panel_url=panel_url, panel_token=panel_token)
    kiris = KirisClient(kiris_url=kiris_url)
    return VoiceSession(
        get_sessions=panel.get_sessions,
        send_text=panel.send_text,
        send_enter=panel.send_enter,
        spawn_and_target=panel.spawn_and_target,
        end_session=panel.end_session,
        on_feedback=kiris.speak if kiris.kiris_url else None,
    )


def _toggle() -> int:
    """`laris toggle`: flip the running daemon's hands-free mode. A GNOME
    custom shortcut runs this, so the reason for a failure goes to stderr
    (the shortcut's journal) and the exit code says it failed."""
    url = peer_discovery.peer_url("laris")
    token = peer_discovery.peer_token("laris")
    if not url or not token:
        print("[Laris] no laris daemon recorded in peers.json - start it first",
              file=sys.stderr)
        return 1
    request = urllib.request.Request(f"{url}/toggle?token={token}", method="POST")
    try:
        with urllib.request.urlopen(request, timeout=2.0):
            return 0
    except Exception as exc:  # noqa: BLE001 - any failure means the toggle did not land
        print(f"[Laris] daemon at {url} did not answer the toggle: {exc}", file=sys.stderr)
        return 1


def main(argv: list[str] | None = None) -> None:
    argv = sys.argv[1:] if argv is None else argv
    if argv == ["toggle"]:
        raise SystemExit(_toggle())

    running = _daemon_already_running()
    if running is not None:
        raise SystemExit(
            f"[Laris] another daemon is already running on {running} - "
            "stop it before starting a second one"
        )

    laris_token = generate_token()
    peer_discovery.write_peer("laris", f"http://127.0.0.1:{LARIS_PORT}", token=laris_token)

    voice_queue = queue.Queue()
    app = build_app(token=laris_token, voice_queue=voice_queue)
    # probe once at boot: macOS applies permission changes only on restart, so
    # a cached value is exact for this process's lifetime and the tray reads it
    # over /theme to warn when push-to-talk lacks Accessibility/Input Monitoring
    trigger = None
    if sys.platform.startswith("linux"):
        # no global key events on Wayland: a desktop shortcut runs `laris
        # toggle`, whose /toggle call flips this same event the loop waits on
        from laris.platform.linux import ToggleTrigger
        trigger = ToggleTrigger()
        app.state.toggle_trigger = trigger
        trigger_label, stt_model = trigger.label, config.STT_MODEL_FASTER
        # requirements-linux.txt leaves DeepFilterNet (import name `df`) out,
        # and audio_capture then passes audio through untouched
        denoise = config.NOISE_REDUCTION
        if denoise == "on" and importlib.util.find_spec("df") is None:
            denoise = "off (DeepFilterNet not installed)"
    else:
        from laris.hotkey import PushToTalk
        app.state.ptt_trusted = PushToTalk.check_trusted()
        trigger_label, stt_model = f"PTT: {config.DICTATE_KEY}", config.STT_MODEL
        denoise = config.NOISE_REDUCTION
    # uvicorn binds the port before the model loads, so /theme answers long
    # before dictation works. Start explicitly not-ready.
    app.state.ready = False
    threading.Thread(
        # access_log=False: the token rides in the query string, and the
        # default access log would write it to the daemon's stdout/log
        target=lambda: uvicorn.run(app, host="127.0.0.1", port=LARIS_PORT,
                                   access_log=False),
        daemon=True,
    ).start()

    if not _wait_for_port("127.0.0.1", LARIS_PORT):
        print(f"[Laris] WARN: overlay server on :{LARIS_PORT} didn't bind within 5s; orb may fail")

    # Deferred import: kiris package may not be installed (pure-dictation case).
    # If missing, agent_phrase_matcher disabled -> "kiris ..." flows through as
    # plain dictation instead of being routed.
    try:
        from kiris.phrase_matching import match_agent_phrase
        kiris_matcher_available = True
    except ImportError:
        kiris_matcher_available = False

    kiris_online = (
        kiris_matcher_available
        and not peer_discovery.peer_offline("kiris")
    )
    kiris_url = peer_discovery.peer_url("kiris")
    kiris_client = KirisClient(kiris_url=kiris_url)

    panel_url = peer_discovery.peer_url("panel")
    print(
        f"[Laris] daemon ouvindo em http://127.0.0.1:{LARIS_PORT} - "
        f"panel={'on' if panel_url else 'off'}, "
        f"kiris={'on' if kiris_online else 'off'}, "
        f"{trigger_label}"
    )
    # answering "which model is running?" should not require reading config.py
    print(
        f"[Laris] STT: {stt_model} "
        f"(lang={config.STT_LANGUAGE or 'auto'}, "
        f"denoise={denoise})"
    )

    from laris import daemon

    daemon.run(
        make_voice_session=_make_voice_session,
        make_assistant=lambda: None,
        agent_phrase_matcher=match_agent_phrase if kiris_online else None,
        kiris_dispatch=kiris_client.ask if kiris_online else None,
        voice_queue=voice_queue,
        overlay_url=f"http://127.0.0.1:{LARIS_PORT}/overlay?token={laris_token}",
        on_ready=lambda: setattr(app.state, "ready", True),
        trigger=trigger,
    )


if __name__ == "__main__":
    main()
