from fastapi.testclient import TestClient

from laris.server import build_app


def _client(tmp_path, monkeypatch):
    monkeypatch.setenv("ORB_THEME_PATH", str(tmp_path / "orb-theme.json"))
    # config reads env at import time for this constant - reload pattern:
    import importlib

    from laris import config as laris_config
    importlib.reload(laris_config)
    app = build_app(token="tok")
    return TestClient(app)


def test_get_theme_returns_defaults_and_palettes(tmp_path, monkeypatch):
    client = _client(tmp_path, monkeypatch)
    resp = client.get("/theme", params={"token": "tok"})
    assert resp.status_code == 200
    body = resp.json()
    assert body["theme"]["archetype"] == "waveform"
    assert body["palettes"]["blade"]["accent"] == "#3fb6c4"


def test_get_theme_defaults_ptt_trusted_true_when_unset(tmp_path, monkeypatch):
    """Old/unknown daemons that never set the flag must not trigger a nag."""
    client = _client(tmp_path, monkeypatch)
    assert client.get("/theme", params={"token": "tok"}).json()["ptt_trusted"] is True


def test_get_theme_reports_untrusted_when_state_false(tmp_path, monkeypatch):
    monkeypatch.setenv("ORB_THEME_PATH", str(tmp_path / "orb-theme.json"))
    import importlib

    from laris import config as laris_config
    importlib.reload(laris_config)
    app = build_app(token="tok")
    app.state.ptt_trusted = False
    assert TestClient(app).get("/theme", params={"token": "tok"}).json()["ptt_trusted"] is False


def test_get_theme_rejects_bad_token(tmp_path, monkeypatch):
    client = _client(tmp_path, monkeypatch)
    assert client.get("/theme", params={"token": "nope"}).status_code == 403


def test_both_routes_reject_missing_token(tmp_path, monkeypatch):
    client = _client(tmp_path, monkeypatch)
    assert client.get("/theme").status_code == 403
    assert client.post("/theme", json={"palette": "acid"}).status_code == 403


def test_post_theme_rejects_bad_token(tmp_path, monkeypatch):
    client = _client(tmp_path, monkeypatch)
    resp = client.post("/theme", params={"token": "nope"}, json={"palette": "acid"})
    assert resp.status_code == 403


def test_post_theme_persists_and_returns(tmp_path, monkeypatch):
    client = _client(tmp_path, monkeypatch)
    resp = client.post("/theme", params={"token": "tok"}, json={"palette": "acid"})
    assert resp.status_code == 200
    assert resp.json()["theme"]["palette"] == "acid"
    assert client.get("/theme", params={"token": "tok"}).json()["theme"]["palette"] == "acid"


def test_post_theme_invalid_value_is_422_with_options(tmp_path, monkeypatch):
    client = _client(tmp_path, monkeypatch)
    resp = client.post("/theme", params={"token": "tok"}, json={"archetype": "cube"})
    assert resp.status_code == 422
    assert "waveform" in resp.json()["detail"]
