"""Ducking on Linux: while hands-free listens, the default output drops to 20%
through wpctl, so speakers bleeding into the mic do not reach the
transcriber; the same sink gets its volume back when the session ends."""
import subprocess

import pytest


class FakeWpctl:
    """Answers `wpctl inspect` / `get-volume` and records every set-volume."""

    def __init__(self, sink_id="60", volume="0.40"):
        self.sink_id = sink_id
        self.volume = volume
        self.set_calls = []
        self.timeouts = []

    def run(self, argv, **kwargs):
        assert argv[0] == "wpctl"
        self.timeouts.append(kwargs.get("timeout"))
        if argv[1] == "inspect":
            out = f"id {self.sink_id}, type PipeWire:Interface:Node\n    alsa.card = \"1\"\n"
        elif argv[1] == "get-volume":
            out = f"Volume: {self.volume}\n"
        elif argv[1] == "set-volume":
            self.set_calls.append(argv[2:])
            out = ""
        else:
            raise AssertionError(f"unexpected wpctl call {argv}")
        return subprocess.CompletedProcess(argv, 0, stdout=out.encode())


@pytest.fixture
def wpctl(monkeypatch):
    from laris import ducking_linux

    fake = FakeWpctl()
    monkeypatch.setattr(ducking_linux.subprocess, "run", fake.run)
    return fake


def test_duck_lowers_the_default_output_to_20_percent_and_restore_brings_it_back(wpctl):
    from laris.ducking_linux import WpctlDucker

    ducker = WpctlDucker()
    ducker.duck()
    ducker.restore()

    assert wpctl.set_calls == [["60", "0.20"], ["60", "0.40"]]


def test_restore_returns_the_volume_to_the_sink_that_was_ducked(wpctl):
    """A headset plugged in mid-session becomes the new default; the old
    speakers are the ones left at 20%."""
    from laris.ducking_linux import WpctlDucker

    ducker = WpctlDucker()
    ducker.duck()
    wpctl.sink_id = "75"
    ducker.restore()

    assert wpctl.set_calls[-1] == ["60", "0.40"]


def test_an_output_already_at_or_below_20_percent_is_left_alone(wpctl):
    from laris.ducking_linux import WpctlDucker

    wpctl.volume = "0.15"
    ducker = WpctlDucker()
    ducker.duck()
    ducker.restore()

    assert wpctl.set_calls == []


def test_ducking_twice_keeps_the_original_volume_for_the_restore(wpctl):
    from laris.ducking_linux import WpctlDucker

    ducker = WpctlDucker()
    ducker.duck()
    wpctl.volume = "0.20"
    ducker.duck()
    ducker.restore()

    assert wpctl.set_calls == [["60", "0.20"], ["60", "0.40"]]


def test_restore_without_a_duck_changes_nothing(wpctl):
    from laris.ducking_linux import WpctlDucker

    WpctlDucker().restore()

    assert wpctl.set_calls == []


def test_every_wpctl_call_is_bounded_so_a_hung_one_cannot_freeze_dictation(wpctl):
    """The voice loop is single-threaded: a wpctl stuck on a busy PipeWire
    would stall every utterance after it."""
    from laris.ducking_linux import WpctlDucker

    ducker = WpctlDucker()
    ducker.duck()
    ducker.restore()

    assert wpctl.timeouts == [4, 4, 4, 4]


def test_a_missing_or_hung_wpctl_never_breaks_dictation(monkeypatch):
    from laris import ducking_linux
    from laris.ducking_linux import WpctlDucker

    def no_wpctl(argv, **kwargs):
        raise FileNotFoundError("wpctl")

    monkeypatch.setattr(ducking_linux.subprocess, "run", no_wpctl)
    ducker = WpctlDucker()
    ducker.duck()
    ducker.restore()

    def hung_wpctl(argv, **kwargs):
        raise subprocess.TimeoutExpired(argv, 4)

    monkeypatch.setattr(ducking_linux.subprocess, "run", hung_wpctl)
    ducker.duck()
    ducker.restore()
