"""LARIS's PTT / hands-free / always-on daemon loop. Panel-agnostic: takes
pre-built VoiceSession + optional assistant as factories, so this module
never imports tmux or session-registry code - see claude-code-ops's
backend/main.py for the composition root that builds those factories."""
from __future__ import annotations

import threading
import time
from collections.abc import Callable

from laris import config
from laris.amplitude import compute_amplitude
from laris.events import VoiceEvent
from laris.protocols import AssistantProtocol
from laris.state_machine import PHASE_IDLE, VoiceSession


def route_utterance(
    text: str,
    voice_session: VoiceSession,
    typer: Callable[[str], None],
    assistant: AssistantProtocol | None = None,
    agent_phrase_matcher: Callable[[str], str | None] | None = None,
    kiris_dispatch: Callable[[str], bool] | None = None,
) -> str:
    """One PTT key, intent comes from the utterance itself: a pending
    assistant confirmation consumes the utterance first, "kiris <pedido>"
    goes to the LLM assistant (in-process AssistantProtocol OR
    kiris_dispatch HTTP callback for the three-process daemon), mid-flow
    speech continues the command flow, "abrir/fechar <projeto>" enters it,
    anything else is dictation into the focused app."""
    from laris.phrase_matching import match_close_phrase, match_open_phrase

    voice_session.expire_if_stale()
    if assistant is not None and assistant.handle_followup(text):
        return PHASE_IDLE
    if voice_session.phase != PHASE_IDLE:
        return voice_session.handle_utterance(text)

    agent_request = agent_phrase_matcher(text) if agent_phrase_matcher else None
    if agent_request is not None:
        if kiris_dispatch is not None:
            # Three-process mode: POST to kiris /ask. On True, utterance consumed.
            # On False (kiris offline/error), FALL THROUGH to dictation so the
            # user's spoken "kiris abre o axon" is typed as plain words instead
            # of being silently dropped.
            if kiris_dispatch(agent_request):
                return PHASE_IDLE
        elif assistant is not None:
            # In-process composition mode (kept for backward compat and dev tests)
            assistant.handle(agent_request)
            return PHASE_IDLE

    open_target = match_open_phrase(text)
    if open_target is not None:
        voice_session.begin_command_flow()
        return voice_session.handle_utterance(open_target)
    if match_close_phrase(text) is not None:
        voice_session.begin_command_flow()
        return voice_session.handle_utterance(text)

    typer(text)
    return PHASE_IDLE


def _make_live_amplitude_pusher(voice_queue, now: Callable[[], float] = time.monotonic, min_interval: float = 0.04) -> Callable:
    """Feeds the orb ~25Hz mic amplitude while capturing - without this the
    orb only moves once per utterance and looks frozen mid-speech."""
    last = {"t": float("-inf")}

    def push(chunk) -> None:
        t = now()
        if t - last["t"] < min_interval:
            return
        last["t"] = t
        voice_queue.put(VoiceEvent(phase="recording", amplitude=compute_amplitude(chunk)))

    return push


def _wait_for_ptt(pressed, hands_free=None, on_tick=lambda: None):
    while True:
        if hands_free is not None and hands_free.is_set():
            return "hands_free", hands_free
        if pressed.is_set():
            return "dictate", pressed
        on_tick()
        time.sleep(0.05)


def _run_hands_free_session(vad, stt, typer, overlay, voice_queue, active, ducker=None):
    overlay.show("dictate")
    voice_queue.put(VoiceEvent(phase="recording", amplitude=0.15))
    if ducker is not None:
        ducker.duck()
    print("[Voice] hands-free ON (tap the dictate key to stop)")
    try:
        for audio, sample_rate in vad.stream_utterances(
            should_stop=lambda: not active.is_set(),
            on_chunk=_make_live_amplitude_pusher(voice_queue),
        ):
            try:
                amplitude = compute_amplitude(audio)
                text = stt.transcribe(audio, sample_rate)
                if text:
                    typer(text)
                    print(f"[Voice] hands-free: {text!r} (amp={amplitude:.4f})")
                voice_queue.put(VoiceEvent(phase="recording", amplitude=amplitude))
            except Exception as exc:  # noqa: BLE001 - one bad utterance must not end the session
                print(f"[Voice] {exc!r} (ignoring this utterance, hands-free keeps running)")
    except Exception as exc:  # noqa: BLE001 - mic/stream failure ends the session, not the loop
        print(f"[Voice] {exc!r} (hands-free session aborted)")
        active.clear()
    finally:
        if ducker is not None:
            ducker.restore()
        overlay.hide()
        voice_queue.put(VoiceEvent(phase=PHASE_IDLE, amplitude=0.0))
        print("[Voice] hands-free OFF")


def run(
    make_voice_session: Callable[[], VoiceSession],
    make_assistant: Callable[[], AssistantProtocol | None] = lambda: None,
    agent_phrase_matcher: Callable[[str], str | None] | None = None,
    kiris_dispatch: Callable[[str], bool] | None = None,
    voice_queue=None,
    overlay_url: str = "",
    on_ready: Callable[[], None] | None = None,
) -> None:
    """Runs the PTT voice loop on the calling thread - callers run this on a
    background daemon thread (see claude-code-ops's backend/main.py)."""
    from laris.hotkey import PushToTalk
    from laris.platform import get_backend

    if voice_queue is None:
        import queue
        voice_queue = queue.Queue()

    backend = get_backend()
    backend.warm()
    stt = backend
    type_text = backend.type_text
    voice_session = make_voice_session()
    assistant = make_assistant()
    overlay = backend.make_overlay(overlay_url)
    ducker = backend.make_ducker()
    ptt = PushToTalk(config.DICTATE_KEY)
    ptt.start()
    # only now is dictation real: the model is loaded and the key listener is
    # attached. The tray polls this to stop claiming "Ditado" during the ~3.6s
    # it takes to get here.
    if on_ready is not None:
        on_ready()
    vad = None
    kiris = "com Kiris (assistente)" if assistant else "pura (VOICE_ASSISTANT=0, muda)"
    print(f"[Voice] Laris (transcritora) pronta, {kiris} - PTT: {config.DICTATE_KEY}")

    def _reset_orb_if_stale():
        if voice_session.expire_if_stale():
            voice_queue.put(VoiceEvent(phase=PHASE_IDLE, amplitude=0.0))

    while True:
        kind, pressed = _wait_for_ptt(ptt.pressed, hands_free=ptt.hands_free, on_tick=_reset_orb_if_stale)
        if kind == "hands_free":
            if vad is None:
                from laris.audio_capture import VadCapture
                vad = VadCapture()
            _run_hands_free_session(vad, stt, type_text, overlay, voice_queue, pressed, ducker=ducker)
            continue

        from laris.audio_capture import record_while

        phase, amplitude = voice_session.phase, 0.0
        overlay.show(kind)
        voice_queue.put(VoiceEvent(phase="recording", amplitude=0.15))
        ducker.duck()
        try:
            audio = record_while(pressed, on_chunk=_make_live_amplitude_pusher(voice_queue))
            overlay.show("transcribing")
            duration = len(audio) / config.SAMPLE_RATE
            if len(audio) >= config.SAMPLE_RATE // 2:
                amplitude = compute_amplitude(audio)
                text = stt.transcribe(audio, config.SAMPLE_RATE)
                if text:
                    phase = route_utterance(
                        text, voice_session, type_text,
                        assistant=assistant,
                        agent_phrase_matcher=agent_phrase_matcher,
                        kiris_dispatch=kiris_dispatch,
                    )
                    print(f"[Voice] {kind}: {text!r} ({duration:.1f}s, amp={amplitude:.4f}) -> phase={phase}")
                else:
                    print(f"[Voice] {kind}: empty transcription ({duration:.1f}s, amp={amplitude:.4f})")
            else:
                print(f"[Voice] {kind}: audio too short ({duration:.2f}s), ignored")
        except Exception as exc:  # noqa: BLE001 - one bad utterance must not kill the listener
            print(f"[Voice] {exc!r} (ignoring this utterance, voice loop keeps running)")
        finally:
            ducker.restore()
            overlay.hide()

        voice_queue.put(VoiceEvent(phase=phase, amplitude=amplitude))


def run_always_on(make_voice_session, voice_queue=None) -> None:
    """Runs on a background daemon thread: VadCapture blocks synchronously
    on the mic stream, so it can't share the asyncio event loop uvicorn
    uses. Heavy ML imports are deferred here so importing laris.daemon
    doesn't require torch/mlx_whisper to be installed."""
    from laris.audio_capture import VadCapture
    from laris.platform import get_backend

    if voice_queue is None:
        import queue
        voice_queue = queue.Queue()

    backend = get_backend()
    backend.warm()
    stt = backend
    voice_session = make_voice_session()

    for audio, sample_rate in VadCapture().stream_utterances():
        try:
            text = stt.transcribe(audio, sample_rate)
            if not text:
                continue
            phase = voice_session.handle_utterance(text)
            amplitude = compute_amplitude(audio)
            print(f"[Voice] heard: {text!r} -> phase={phase}")
        except Exception as exc:  # noqa: BLE001 - one bad utterance must not kill the listener
            print(f"[Voice] {exc!r} (ignoring this utterance, voice loop keeps running)")
            continue

        voice_queue.put(VoiceEvent(phase=phase, amplitude=amplitude))
