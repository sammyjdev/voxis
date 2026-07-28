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
    from laris.server import build_app

    app = build_app(token="test-token")
    with TestClient(app).websocket_connect("/ws/voice") as ws:
        data = ws.receive_json()
        assert "error" in data


def test_ws_voice_streams_queued_events():
    from laris.events import VoiceEvent
    from laris.server import build_app, get_voice_event_queue

    app = build_app(token="test-token")
    get_voice_event_queue(app).put(VoiceEvent(phase="recording", amplitude=0.5))

    with TestClient(app).websocket_connect("/ws/voice?token=test-token") as ws:
        data = ws.receive_json()
        assert data == {"phase": "recording", "amplitude": 0.5}


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
