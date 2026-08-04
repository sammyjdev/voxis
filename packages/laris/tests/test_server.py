from fastapi.testclient import TestClient


def test_frontend_dir_uses_pyinstaller_resource_dir(monkeypatch, tmp_path):
    import laris.server as server

    monkeypatch.setattr(server.sys, "_MEIPASS", str(tmp_path), raising=False)

    assert server._frontend_dir() == tmp_path / "frontend"


def test_overlay_route_serves_html():
    from laris.server import build_app

    app = build_app(token="test-token")
    response = TestClient(app).get("/overlay")

    assert response.status_code == 200
    assert "voice-orb.js" in response.text


def test_ws_voice_rejects_missing_token():
    """A missing token is refused during the handshake, not after it. The old
    behaviour accepted the socket and sent an error frame, which the client
    read as a successful connection."""
    import pytest
    from starlette.websockets import WebSocketDisconnect

    from laris.server import build_app

    app = build_app(token="test-token")
    with pytest.raises(WebSocketDisconnect) as exc:
        with TestClient(app).websocket_connect("/ws/voice"):
            pass

    assert exc.value.code == 1008


def test_ws_voice_streams_queued_events():
    from laris.events import VoiceEvent
    from laris.server import build_app, get_voice_event_queue

    app = build_app(token="test-token")
    get_voice_event_queue(app).put(VoiceEvent(phase="recording", amplitude=0.5))

    with TestClient(app).websocket_connect("/ws/voice?token=test-token") as ws:
        data = ws.receive_json()
        assert data == {"phase": "recording", "amplitude": 0.5}


def test_a_closed_ws_does_not_eat_the_next_orb_event():
    """The orb only listens, so sending is the sole thing that notices it left -
    and while idle nothing is sent. A closed socket's handler outlived it, took
    the next event off the shared queue and died on the send, so the event never
    reached the reconnected orb. Needs a real server: TestClient cancels the
    handler task on exit, which hides the leak entirely."""
    import threading
    import time

    import uvicorn
    from websockets.sync.client import connect

    from laris.events import VoiceEvent
    from laris.server import build_app, get_voice_event_queue

    app = build_app(token="test-token")
    queue = get_voice_event_queue(app)
    server = uvicorn.Server(
        uvicorn.Config(app, host="127.0.0.1", port=0, log_level="error")
    )
    threading.Thread(target=server.run, daemon=True).start()
    try:
        while not server.started:
            time.sleep(0.05)
        port = server.servers[0].sockets[0].getsockname()[1]

        with connect(f"ws://127.0.0.1:{port}/ws/voice?token=test-token") as ws:
            queue.put(VoiceEvent(phase="recording", amplitude=0.5))
            ws.recv()
        time.sleep(1.0)  # let the handler notice the disconnect, as it would in life

        queue.put(VoiceEvent(phase="idle", amplitude=0.0))
        time.sleep(1.5)  # > the handler's 0.5s queue poll

        assert queue.qsize() == 1, "the closed socket's handler took the orb's next event"
    finally:
        server.should_exit = True


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


def test_ws_rejects_a_bad_token_without_accepting_it():
    """Accepting first and closing after still fires onopen on the client, which
    reads it as a good connection and resets its reconnect backoff - a stale
    token then reconnects in a tight loop. Reject before the handshake so the
    client sees a failure and backs off. 1008 = policy violation."""
    from fastapi.testclient import TestClient
    from starlette.websockets import WebSocketDisconnect
    import pytest

    from laris.server import build_app

    client = TestClient(build_app(token="good"))

    with pytest.raises(WebSocketDisconnect) as exc:
        with client.websocket_connect("/ws/voice?token=wrong"):
            pass

    assert exc.value.code == 1008
