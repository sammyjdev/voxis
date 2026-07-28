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

    @app.websocket("/ws/voice")
    async def ws_voice(websocket: WebSocket, token: str | None = None):
        await websocket.accept()
        if not laris_auth.token_valid(token, websocket.app.state.token):
            await websocket.send_json({"error": "invalid or missing session token"})
            await websocket.close()
            return

        voice_queue = get_voice_event_queue(websocket.app)
        try:
            while True:
                try:
                    event = await asyncio.to_thread(voice_queue.get, timeout=0.5)
                except thread_queue.Empty:
                    continue
                await websocket.send_json(asdict(event))
        except WebSocketDisconnect:
            return

    return app
