"""LARIS's standalone HTTP surface for the desktop overlay orb: /overlay
serves the orb page, /ws/voice streams VoiceEvent to it. Built as a factory
(build_app) so each daemon run gets its own token and its own event queue -
no module-level globals, unlike the panel's server.py (which only ever runs
one instance per process)."""
from __future__ import annotations

import asyncio
import queue as thread_queue
import sys
from dataclasses import asdict
from pathlib import Path

from fastapi import FastAPI, HTTPException, WebSocket, WebSocketDisconnect
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

from laris import auth as laris_auth
from laris.events import VoiceEvent
from laris import orb_theme

def _frontend_dir() -> Path:
    return Path(getattr(sys, "_MEIPASS", Path(__file__).parent.parent)) / "frontend"


FRONTEND_DIR = _frontend_dir()


def get_voice_event_queue(app: FastAPI) -> "thread_queue.Queue[VoiceEvent]":
    return app.state.voice_events


def build_app(token: str, voice_queue: "thread_queue.Queue[VoiceEvent] | None" = None) -> FastAPI:
    """voice_queue: pass the SAME queue instance the PTT daemon loop pushes
    into so this app's /ws/voice actually streams live events instead of an
    unconnected queue nobody writes to. Defaults to a fresh, disconnected
    queue for standalone testing."""
    app = FastAPI()
    app.state.token = token
    app.state.voice_events = voice_queue if voice_queue is not None else thread_queue.Queue()
    app.mount("/static", StaticFiles(directory=FRONTEND_DIR), name="static")

    @app.get("/overlay")
    def overlay_page():
        return FileResponse(FRONTEND_DIR / "overlay.html")

    @app.get("/theme")
    def get_theme(token: str | None = None):
        if not laris_auth.token_valid(token, app.state.token):
            raise HTTPException(status_code=403, detail="invalid token")
        # ptt_trusted rides the poll the tray already makes each tick: it lets
        # the tray surface a missing-permission daemon instead of PTT dying
        # silently. Default True so old daemons that never set it don't nag.
        return {
            "theme": orb_theme.load_theme(),
            "palettes": orb_theme.PALETTES,
            "ptt_trusted": getattr(app.state, "ptt_trusted", True),
            # False while the model loads. Same default-permissive rule as
            # ptt_trusted: an old daemon that never sets it reads as ready.
            "ready": getattr(app.state, "ready", True),
            # Linux has no orb, so the tray icon is the only sign the toggle
            # left dictation listening. macOS has no toggle trigger: False.
            "dictating": bool(getattr(app.state, "toggle_trigger", None)
                              and app.state.toggle_trigger.hands_free.is_set()),
        }

    @app.post("/theme")
    def post_theme(updates: dict, token: str | None = None):
        if not laris_auth.token_valid(token, app.state.token):
            raise HTTPException(status_code=403, detail="invalid token")
        try:
            theme = orb_theme.save_theme(updates)
        except orb_theme.ThemeValidationError as exc:
            raise HTTPException(status_code=422, detail=str(exc)) from exc
        return {"theme": theme, "palettes": orb_theme.PALETTES}

    @app.post("/toggle")
    def toggle(token: str | None = None):
        """Flips hands-free. The Linux trigger: GNOME Wayland gives no global
        key events, so a desktop shortcut runs `laris toggle`, which calls this.
        The daemon wires app.state.toggle_trigger only where that trigger exists."""
        if not laris_auth.token_valid(token, app.state.token):
            raise HTTPException(status_code=403, detail="invalid token")
        trigger = getattr(app.state, "toggle_trigger", None)
        if trigger is None:
            raise HTTPException(status_code=409, detail="this daemon has no toggle trigger")
        return {"hands_free": trigger.toggle()}

    @app.websocket("/ws/voice")
    async def ws_voice(websocket: WebSocket, token: str | None = None):
        # reject BEFORE the handshake: accepting first still fires onopen on the
        # client, which reads it as a good connection and resets its reconnect
        # backoff, so a stale token reconnects in a tight loop. 1008 = policy
        # violation.
        if not laris_auth.token_valid(token, websocket.app.state.token):
            await websocket.close(code=1008, reason="invalid or missing session token")
            return
        await websocket.accept()

        voice_queue = get_voice_event_queue(websocket.app)
        # The orb only listens, so sending is the sole thing that ever notices
        # it left - and while idle we never send. Without this the handler of a
        # closed socket outlives it, takes the next event off the shared queue,
        # and dies on the send: the reconnected orb never sees that event.
        gone = asyncio.create_task(websocket.receive())
        try:
            while not gone.done():
                try:
                    event = await asyncio.to_thread(voice_queue.get, timeout=0.5)
                except thread_queue.Empty:
                    continue
                await websocket.send_json(asdict(event))
        except (WebSocketDisconnect, RuntimeError):
            # RuntimeError: uvicorn raises it (not WebSocketDisconnect) when the
            # send races a client that already closed.
            pass
        finally:
            gone.cancel()

    return app
