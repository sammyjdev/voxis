"""`laris toggle`: the Linux trigger. GNOME Wayland gives an app no global
key events, so a GNOME custom shortcut runs `python -m laris toggle`, which
flips the running daemon's hands-free mode over its authenticated HTTP API."""
import itertools
import threading
import time

import pytest
import uvicorn
from fastapi.testclient import TestClient

from laris import peer_discovery
from laris.platform.linux import ToggleTrigger
from laris.server import build_app


class FakeClock:
    def __init__(self):
        self.now = 0.0

    def __call__(self):
        return self.now


@pytest.fixture
def peers(monkeypatch, tmp_path):
    monkeypatch.setattr(peer_discovery, "_peers_path_override", tmp_path / "peers.json")


@pytest.fixture
def live_daemon(peers):
    """A real server on a free port, recorded in peers.json like a booted
    daemon. Its clock moves 5 s per toggle, so two commands are two presses."""
    app = build_app(token="tok")
    seconds = itertools.count(0, 5)
    app.state.toggle_trigger = ToggleTrigger(now=lambda: next(seconds))
    server = uvicorn.Server(uvicorn.Config(app, host="127.0.0.1", port=0, log_level="error"))
    threading.Thread(target=server.run, daemon=True).start()
    while not server.started:
        time.sleep(0.05)
    port = server.servers[0].sockets[0].getsockname()[1]
    peer_discovery.write_peer("laris", f"http://127.0.0.1:{port}", token="tok")
    try:
        yield app
    finally:
        server.should_exit = True


def _run_toggle_command():
    from laris.__main__ import main

    with pytest.raises(SystemExit) as exc:
        main(["toggle"])
    return exc.value.code


def test_each_toggle_command_flips_hands_free_on_the_running_daemon(live_daemon):
    hands_free = live_daemon.state.toggle_trigger.hands_free

    first = _run_toggle_command()
    on_after_first = hands_free.is_set()
    second = _run_toggle_command()

    assert (first, on_after_first) == (0, True)
    assert (second, hands_free.is_set()) == (0, False)


def test_a_toggle_with_a_wrong_token_is_refused_and_changes_nothing():
    app = build_app(token="good")
    app.state.toggle_trigger = ToggleTrigger()

    response = TestClient(app).post("/toggle?token=wrong")

    assert response.status_code == 403
    assert not app.state.toggle_trigger.hands_free.is_set()


def test_a_toggle_without_a_token_is_refused_and_changes_nothing():
    app = build_app(token="good")
    app.state.toggle_trigger = ToggleTrigger()

    response = TestClient(app).post("/toggle")

    assert response.status_code == 403
    assert not app.state.toggle_trigger.hands_free.is_set()


def test_a_daemon_with_no_toggle_trigger_answers_409_not_a_crash():
    """macOS daemons drive hands-free from the key listener and never wire
    the toggle event; the route must say so instead of a 500."""
    response = TestClient(build_app(token="t")).post("/toggle?token=t")

    assert response.status_code == 409


def test_the_toggle_command_fails_with_a_reason_when_no_daemon_answers(peers, capsys):
    peer_discovery.write_peer("laris", "http://127.0.0.1:1", token="tok")  # nothing listens on :1

    code = _run_toggle_command()

    assert code == 1
    assert "did not answer" in capsys.readouterr().err


def test_the_toggle_command_fails_with_a_reason_when_no_daemon_is_recorded(peers, capsys):
    code = _run_toggle_command()

    assert code == 1
    assert "no laris daemon" in capsys.readouterr().err


def _toggle_at(times):
    """POSTs /toggle once per time in `times`; returns the final hands-free state."""
    clock = FakeClock()
    app = build_app(token="t")
    app.state.toggle_trigger = ToggleTrigger(now=clock)
    client = TestClient(app)
    for t in times:
        clock.now = t
        assert client.post("/toggle?token=t").status_code == 200
    return app.state.toggle_trigger.hands_free.is_set()


def test_a_held_shortcut_repeating_every_30_ms_turns_hands_free_on_only_once():
    """GNOME fires a custom shortcut again on key autorepeat: once on press,
    then every 30 ms after a 500 ms delay, each one a `laris toggle` run."""
    held = [0.0] + [0.5 + 0.03 * i for i in range(24)]

    assert _toggle_at(held) is True


def test_a_press_more_than_a_second_after_the_last_repeat_turns_hands_free_off():
    held = [0.0] + [0.5 + 0.03 * i for i in range(24)]

    assert _toggle_at(held + [held[-1] + 1.1]) is False


def test_a_second_tap_more_than_a_second_later_counts_as_a_new_press():
    assert _toggle_at([0.0, 1.1]) is False


def test_a_second_tap_within_a_second_is_taken_for_the_same_held_press():
    """The shortcut gives no key-release signal, so a quick re-tap and a key
    repeat look the same: both are ignored."""
    assert _toggle_at([0.0, 0.2]) is True


def test_concurrent_toggles_from_one_press_never_flip_twice():
    """A burst of `laris toggle` processes lands on the server's threadpool
    at once; only one of them may flip."""
    trigger = ToggleTrigger(now=lambda: 0.0)
    barrier = threading.Barrier(8)

    def _press():
        barrier.wait()
        trigger.toggle()

    threads = [threading.Thread(target=_press) for _ in range(8)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()

    assert trigger.hands_free.is_set()


def test_the_theme_poll_reports_dictating_while_toggled_hands_free_is_on():
    """The tray polls /theme every tick; on Linux, with no orb, that is how
    its icon learns dictation is listening."""
    app = build_app(token="t")
    app.state.toggle_trigger = ToggleTrigger()
    client = TestClient(app)

    before = client.get("/theme?token=t").json()["dictating"]
    client.post("/toggle?token=t")
    after = client.get("/theme?token=t").json()["dictating"]

    assert (before, after) == (False, True)


def test_a_daemon_without_a_toggle_trigger_never_reports_dictating():
    response = TestClient(build_app(token="t")).get("/theme?token=t")

    assert response.json()["dictating"] is False
