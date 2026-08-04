from dataclasses import dataclass
from datetime import datetime
from unittest.mock import MagicMock

from laris.state_machine import (
    PHASE_AWAITING_DICTATION,
    PHASE_IDLE,
    VoiceSession,
)


@dataclass(frozen=True)
class _FakeSession:
    name: str
    project: str
    tmux_pid: int
    cwd: str
    state: str
    git_branch: str | None
    tmux_name: str
    last_active: datetime | None = None


def _stub_agent_phrase_matcher(text: str) -> str | None:
    """Local stand-in for kiris.phrase_matching.match_agent_phrase
    (lives in the kiris repo - see Task 10). Strips a leading 'kiris '
    or 'ola kiris ' from the utterance so route_utterance's KIRIS branch
    can be exercised without importing the kiris package."""
    words = text.lower().split()
    if len(words) >= 2 and words[0] in {"ola", "oi"} and words[1] == "kiris":
        return " ".join(words[2:])
    if words and words[0] == "kiris":
        return " ".join(words[1:])
    return None


def _lina_session():
    return _FakeSession(
        name="lina", project="lina", tmux_pid=1, cwd="/tmp/lina",
        state="working", git_branch=None, tmux_name="lina",
    )


def _make_voice_session():
    send_text = MagicMock()
    end_session = MagicMock()
    vs = VoiceSession(
        get_sessions=lambda: [_lina_session()],
        send_text=send_text,
        send_enter=MagicMock(),
        end_session=end_session,
    )
    return vs, send_text, end_session


def test_plain_text_is_dictation(monkeypatch):
    from laris.daemon import route_utterance

    vs, send_text, _ = _make_voice_session()
    typer = MagicMock()

    phase = route_utterance("hello world", vs, typer)

    assert phase == PHASE_IDLE
    typer.assert_called_once_with("hello world")
    send_text.assert_not_called()


def test_open_verb_enters_command_flow(monkeypatch):
    from laris.daemon import route_utterance

    vs, _, _ = _make_voice_session()
    typer = MagicMock()

    phase = route_utterance("abrir lina", vs, typer)

    assert phase == PHASE_AWAITING_DICTATION
    assert vs.matched_session.name == "lina"
    typer.assert_not_called()


def test_close_verb_kills_session_without_typing(monkeypatch):
    from laris.daemon import route_utterance

    vs, _, end_session = _make_voice_session()
    typer = MagicMock()

    phase = route_utterance("fechar lina", vs, typer)

    assert phase == PHASE_IDLE
    end_session.assert_called_once_with("lina")
    typer.assert_not_called()


def test_mid_flow_utterance_continues_the_flow_not_dictation(monkeypatch):
    from laris.daemon import route_utterance

    vs, send_text, _ = _make_voice_session()
    typer = MagicMock()
    route_utterance("abrir lina", vs, typer)  # now awaiting dictation

    route_utterance("roda os testes", vs, typer)

    send_text.assert_called_once_with("lina", "roda os testes")
    typer.assert_not_called()


def test_live_amplitude_pusher_throttles(monkeypatch):
    import numpy as np

    from laris.daemon import _make_live_amplitude_pusher

    events = []

    class FakeQueue:
        def put(self, event):
            events.append(event)

    clock = {"t": 100.0}
    push = _make_live_amplitude_pusher(FakeQueue(), now=lambda: clock["t"])

    chunk = np.full((4,), 0.5, dtype=np.float32)
    push(chunk)
    push(chunk)  # same instant: throttled
    clock["t"] += 0.2
    push(chunk)

    assert len(events) == 2
    assert all(e.phase == "recording" for e in events)
    assert events[0].amplitude > 0


def test_live_amplitude_pusher_defaults_to_25hz():
    """Mic chunks land at ~31Hz; the orb should see nearly all of them -
    at the old 10Hz the deformation lagged visibly behind speech."""
    import numpy as np

    from laris.daemon import _make_live_amplitude_pusher

    events = []

    class FakeQueue:
        def put(self, event):
            events.append(event)

    clock = {"t": 100.0}
    push = _make_live_amplitude_pusher(FakeQueue(), now=lambda: clock["t"])

    chunk = np.full((4,), 0.5, dtype=np.float32)
    for _ in range(10):  # chunks every 50ms for half a second
        push(chunk)
        clock["t"] += 0.05

    assert len(events) == 10  # none throttled at 20Hz input


class _StubAssistant:
    def __init__(self):
        self.requests = []
        self.followups = []
        self.consume_followup = False

    def handle(self, request):
        self.requests.append(request)

    def handle_followup(self, text):
        self.followups.append(text)
        return self.consume_followup


def test_agent_name_routes_to_assistant_not_dictation():
    from laris.daemon import route_utterance

    vs, send_text, _ = _make_voice_session()
    typer = MagicMock()
    assistant = _StubAssistant()

    phase = route_utterance(
        "kiris abre o axon", vs, typer, assistant=assistant,
        agent_phrase_matcher=_stub_agent_phrase_matcher,
    )

    assert phase == PHASE_IDLE
    assert assistant.requests == ["abre o axon"]
    typer.assert_not_called()
    send_text.assert_not_called()


def test_kiris_phrase_is_plain_dictation_without_assistant():
    """Laris (pure transcriber): the agent's name is just words to type."""
    from laris.daemon import route_utterance

    vs, send_text, _ = _make_voice_session()
    typer = MagicMock()

    phase = route_utterance("kiris abre o axon", vs, typer, assistant=None)

    assert phase == PHASE_IDLE
    typer.assert_called_once_with("kiris abre o axon")
    send_text.assert_not_called()


def test_pending_confirmation_consumes_the_next_utterance():
    from laris.daemon import route_utterance

    vs, _, _ = _make_voice_session()
    typer = MagicMock()
    assistant = _StubAssistant()
    assistant.consume_followup = True

    phase = route_utterance("sim", vs, typer, assistant=assistant)

    assert phase == PHASE_IDLE
    assert assistant.followups == ["sim"]
    typer.assert_not_called()


def test_kiris_dispatch_routes_when_matcher_and_kiris_dispatch_present():
    from laris.daemon import route_utterance

    vs, _, _ = _make_voice_session()
    typer = MagicMock()
    dispatch_calls = []
    def stub_matcher(text): return "abre o axon"
    def stub_dispatch(text): dispatch_calls.append(text); return True

    phase = route_utterance(
        "kiris abre o axon", vs, typer,
        agent_phrase_matcher=stub_matcher,
        kiris_dispatch=stub_dispatch,
    )

    assert phase == PHASE_IDLE
    assert dispatch_calls == ["abre o axon"]
    typer.assert_not_called()


def test_kiris_dispatch_skipped_when_offline_returns_false():
    """When kiris_dispatch returns False (kiris offline/error), route_utterance
    falls through to plain dictation - the utterance is typed into the focused app."""
    from laris.daemon import route_utterance

    vs, _, _ = _make_voice_session()
    typer = MagicMock()
    def stub_matcher(text): return "abre o axon"
    def stub_dispatch_offline(text): return False

    phase = route_utterance(
        "kiris abre o axon", vs, typer,
        agent_phrase_matcher=stub_matcher,
        kiris_dispatch=stub_dispatch_offline,
    )

    assert phase == PHASE_IDLE
    # Utterance is typed as dictation (kiris offline -> fallback)
    typer.assert_called_once_with("kiris abre o axon")


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


class _StopLoop(Exception):
    """Breaks out of run()'s infinite PTT loop once we've seen what we need."""


def _run_with_fakes(monkeypatch, mode):
    """Drive daemon.run() far enough to observe the loop's first iteration."""
    import threading

    import laris.config as config_mod
    import laris.daemon as daemon_mod
    import laris.hotkey as hotkey_mod
    import laris.platform as platform_mod

    class FakeBackend:
        def warm(self):
            pass

        def transcribe(self, audio, sample_rate):
            return ""

        def type_text(self, text):
            pass

        def make_overlay(self, url):
            return MagicMock()

        def make_ducker(self):
            return MagicMock()

    class FakePTT:
        def __init__(self, key):
            self.pressed = threading.Event()
            self.hands_free = threading.Event()

        def start(self):
            pass

    monkeypatch.setattr(platform_mod, "get_backend", lambda: FakeBackend())
    monkeypatch.setattr(hotkey_mod, "PushToTalk", FakePTT)
    monkeypatch.setattr(config_mod, "MODE", mode)

    seen = {}

    def fake_wait(pressed, hands_free=None, on_tick=lambda: None):
        # the flag must already be set the first time the loop asks for input
        seen["armed_at_entry"] = hands_free.is_set()
        raise _StopLoop

    monkeypatch.setattr(daemon_mod, "_wait_for_ptt", fake_wait)

    try:
        daemon_mod.run(make_voice_session=lambda: MagicMock())
    except _StopLoop:
        pass
    return seen


def test_always_on_boots_into_hands_free(monkeypatch):
    """ASSISTANT_MODE=always_on is the tray's "Hands-free" checkbox. It was
    written to the settings file and read into config.MODE, and then nothing
    consumed it - ticking the box did nothing at all."""
    assert _run_with_fakes(monkeypatch, "always_on")["armed_at_entry"] is True


def test_ptt_mode_does_not_arm_hands_free(monkeypatch):
    """The default must still wait for a real key press, not a VAD session."""
    assert _run_with_fakes(monkeypatch, "ptt")["armed_at_entry"] is False


def test_missing_vad_degrades_hands_free_to_ptt(monkeypatch):
    """always_on arms the in-loop flag instead of calling run_always_on()
    precisely so a missing silero-vad degrades to PTT. VadCapture() raises
    SystemExit when the package is absent and nothing caught it, so the whole
    daemon died at the first hands-free session - plain dictation included."""
    import threading

    import laris.audio_capture as audio_mod
    import laris.config as config_mod
    import laris.daemon as daemon_mod
    import laris.hotkey as hotkey_mod
    import laris.platform as platform_mod

    class FakeBackend:
        def warm(self):
            pass

        def transcribe(self, audio, sample_rate):
            return ""

        def type_text(self, text):
            pass

        def make_overlay(self, url):
            return MagicMock()

        def make_ducker(self):
            return MagicMock()

    class FakePTT:
        def __init__(self, key):
            self.pressed = threading.Event()
            self.hands_free = threading.Event()

        def start(self):
            pass

    def no_silero_vad():
        raise SystemExit("[Audio] silero-vad not installed: pip install silero-vad")

    monkeypatch.setattr(platform_mod, "get_backend", lambda: FakeBackend())
    monkeypatch.setattr(hotkey_mod, "PushToTalk", FakePTT)
    monkeypatch.setattr(config_mod, "MODE", "always_on")
    monkeypatch.setattr(audio_mod, "VadCapture", no_silero_vad)

    armed = []

    def fake_wait(pressed, hands_free=None, on_tick=lambda: None):
        armed.append(hands_free.is_set())
        if len(armed) > 1:
            raise _StopLoop
        return "hands_free", hands_free

    monkeypatch.setattr(daemon_mod, "_wait_for_ptt", fake_wait)

    try:
        daemon_mod.run(make_voice_session=lambda: MagicMock())
    except _StopLoop:
        pass

    assert armed == [True, False], "hands-free must disarm and fall back to PTT"


def test_boot_refuses_to_start_beside_a_live_daemon(monkeypatch):
    """A second daemon can't bind :8421, but it still starts its own keyboard
    listener: one key press then records and types the utterance twice."""
    import laris.__main__ as main_mod
    from laris import peer_discovery

    monkeypatch.setattr(peer_discovery, "peer_url", lambda name: "http://127.0.0.1:8421")
    monkeypatch.setattr(peer_discovery, "peer_token", lambda name: "abc")

    class _Ok:
        status = 200

        def __enter__(self):
            return self

        def __exit__(self, *_):
            return False

    assert main_mod._daemon_already_running(http_get=lambda *a, **k: _Ok()) == (
        "http://127.0.0.1:8421"
    )


def test_boot_proceeds_when_the_recorded_daemon_is_gone(monkeypatch):
    """peers.json outlives the daemon that wrote it - a stale entry must not
    block the next boot."""
    import laris.__main__ as main_mod
    from laris import peer_discovery

    monkeypatch.setattr(peer_discovery, "peer_url", lambda name: "http://127.0.0.1:8421")
    monkeypatch.setattr(peer_discovery, "peer_token", lambda name: "abc")

    def refused(*_args, **_kwargs):
        raise OSError("connection refused")

    assert main_mod._daemon_already_running(http_get=refused) is None
