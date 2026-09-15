#!/usr/bin/env python3
"""Interactive harness for the VOXIS v1.0.0 owner-Mac failure matrix.

Drive each case from the built VOXIS tray as the validation ledger requires;
this script injects the scriptable failures, measures detection time from the
manifest, and prints paste-ready rows for
docs/releases/2026-09-14-v1.0.0-validation.md.

Run it with the project interpreter (stdlib only, but keep it consistent):

    cd packages/miris && .venv/bin/python ../../scripts/v1_matrix.py [case ...]

Case numbers (default: all, in ledger order):
  1 microphone denied            5 microphone disconnected
  2 system-audio denied          6 LLM failure
  3 helper terminated            7 quit during active work
  4 recorder terminated

Session directories are never deleted - the ledger asks for preserved
evidence. Notifications are raised by the tray, not this script: paste the
exact text you saw into the Observed column next to the code and elapsed
time printed here.
"""
import os
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "packages" / "miris"))

from miris import config, session  # noqa: E402

POLL_S = 0.25
DETECT_TARGET_S = 4.0  # tray notification budget: recorder 2s stall + 2s poll


def load_cfg() -> config.Config:
    return config.load()


def newest_dir(cfg: config.Config, before: set[Path] | None = None) -> Path | None:
    dirs = [p for p in cfg.sessions_dir.glob("*") if p.is_dir()]
    if before is not None:
        dirs = [p for p in dirs if p not in before]
    return max(dirs, key=lambda p: p.name, default=None)


def wait_for_new_session(cfg: config.Config, timeout_s: float = 30.0) -> Path:
    before = {p for p in cfg.sessions_dir.glob("*") if p.is_dir()}
    print(f"  waiting for a new session dir (max {timeout_s:.0f}s)...")
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        found = newest_dir(cfg, before)
        if found is not None:
            print(f"  session: {found.name}")
            return found
        time.sleep(POLL_S)
    raise TimeoutError("no new session directory appeared")


def wait_state(
    session_dir: Path, states: set[session.SessionState], timeout_s: float
) -> tuple[session.SessionManifest, float]:
    """Poll until the manifest reaches one of `states`; return it + elapsed."""
    start = time.monotonic()
    last = None
    while time.monotonic() - start < timeout_s:
        try:
            last = session.load(session_dir)
        except Exception:
            time.sleep(POLL_S)
            continue
        if last.state in states:
            return last, time.monotonic() - start
        time.sleep(POLL_S)
    return last, time.monotonic() - start


def wav_size(session_dir: Path, name: str) -> int:
    path = session_dir / name
    return path.stat().st_size if path.exists() else -1


def pid_of_pattern(pattern: str) -> int | None:
    out = subprocess.run(
        ["pgrep", "-f", pattern], capture_output=True, text=True
    ).stdout.split()
    return int(out[0]) if out else None


def find_tray() -> tuple[int, str] | None:
    """Return (pid, binary path) of the running VOXIS tray, or None."""
    out = subprocess.run(
        ["pgrep", "-fl", "VOXIS.app/Contents/MacOS"], capture_output=True, text=True
    ).stdout.strip().splitlines()
    if not out:
        return None
    pid, _, path = out[0].partition(" ")
    return int(pid), path


def tray_bundle(tray_path: str) -> str:
    """The .app directory a running tray binary belongs to."""
    return tray_path.split("/Contents/MacOS")[0]


def relaunch_tray(bundle: str) -> None:
    """Revoking a TCC permission kills the running app; bring it back."""
    subprocess.run(["open", bundle], check=False)
    for _ in range(40):
        if find_tray() is not None:
            time.sleep(1.0)  # let setup settle (daemon spawn, menu build)
            return
        time.sleep(0.5)
    raise RuntimeError(f"tray did not relaunch from {bundle}")


def confirm(prompt: str) -> bool:
    return ask(f"  {prompt} [y/N] ").strip().lower() == "y"


def ask(prompt: str) -> str:
    try:
        return input(prompt)
    except EOFError:
        print("\nno interactive terminal - aborting")
        raise SystemExit(2)


def record_line(msg: str) -> None:
    print(f"  >> {msg}")


def begin(case_no: int, title: str, expected: str) -> None:
    print(f"\n=== case {case_no}: {title}")
    print(f"  expected: {expected}")


class Result:
    def __init__(self, case_no: int, title: str):
        self.case_no = case_no
        self.title = title
        self.verdict = "FAIL"
        self.session = "-"
        self.code = "-"
        self.elapsed = "-"
        self.notes = ""

    def row(self) -> str:
        return (
            f"| {self.title} | {self.notes} "
            f"(code={self.code}, detect={self.elapsed}, session={self.session}) "
            f"| {self.verdict} |"
        )


def _permission_denied_case(
    cfg: config.Config, case_no: int, title: str, revoke_hint: str
) -> Result:
    r = Result(case_no, title)
    tray = find_tray()
    if tray is None:
        r.notes = "no running tray at case start"
        return r
    bundle = tray_bundle(tray[1])
    print(f"""  steps:
  1. {revoke_hint}
     NOTE: macOS kills VOXIS the moment you flip the toggle - that is normal.
  2. come back here and press Enter: the script relaunches VOXIS
  3. when it is back, click "Gravar reunião" in the tray""")
    ask("  press Enter after revoking the permission: ")
    relaunch_tray(bundle)
    record_line(f"tray relaunched from {bundle}")
    session_dir = wait_for_new_session(cfg)
    manifest, elapsed = wait_state(
        session_dir,
        {session.SessionState.FAILED, session.SessionState.RECORDING},
        30.0,
    )
    r.session = session_dir.name
    r.code = manifest.error.code if manifest and manifest.error else "-"
    r.notes = f"final state {manifest.state.value if manifest else 'none'}"
    if manifest is not None and manifest.state is session.SessionState.FAILED:
        r.verdict = "PASS"
        record_line('tray should have shown "Falha ao iniciar gravação: ..." - paste its exact text in the ledger')
    else:
        r.notes += " - FALSE recording state, do not edit the expected result"
    r.elapsed = f"{elapsed:.1f}s"
    return r


def case_1_mic_denied(cfg: config.Config) -> Result:
    return _permission_denied_case(
        cfg,
        1,
        "Microphone denied",
        "System Settings > Privacy & Security > Microphone > toggle VOXIS OFF",
    )


def case_2_system_audio_denied(cfg: config.Config) -> Result:
    return _permission_denied_case(
        cfg,
        2,
        "System audio denied",
        "System Settings > Privacy & Security: revoke VOXIS's screen/system-audio recording grant",
    )


def start_and_wait_recording(cfg: config.Config) -> Path:
    print("""  steps:
  1. click "Gravar reunião" in the tray
  2. wait until the menu shows "Parar gravação (...)" """)
    session_dir = wait_for_new_session(cfg)
    manifest, _ = wait_state(session_dir, {session.SessionState.RECORDING}, 60.0)
    if manifest.state is not session.SessionState.RECORDING:
        raise RuntimeError(
            f"session {session_dir.name} never reached recording "
            f"(state={manifest.state.value}, code={manifest.error.code if manifest.error else '-'})"
        )
    return session_dir


def channels_stopped(session_dir: Path) -> bool:
    sizes = [(wav_size(session_dir, n)) for n in ("mic.wav", "system.wav")]
    time.sleep(1.5)
    sizes_after = [(wav_size(session_dir, n)) for n in ("mic.wav", "system.wav")]
    return sizes == sizes_after and all(s > 0 for s in sizes_after)


def case_3_helper_terminated(cfg: config.Config) -> Result:
    r = Result(3, "Helper terminated")
    session_dir = start_and_wait_recording(cfg)
    helper = pid_of_pattern(f"audiotap {session_dir}/system.wav")
    if helper is None:
        helper = pid_of_pattern("audiotap.*system.wav")
    if helper is None:
        r.session = session_dir.name
        r.notes = "no audiotap helper found - could not inject"
        return r
    ask("  press Enter to SIGKILL the system-audio helper NOW: ")
    t0 = time.monotonic()
    os.kill(helper, signal.SIGKILL)
    manifest, elapsed = wait_state(
        session_dir, {session.SessionState.INTERRUPTED}, 15.0
    )
    r.session = session_dir.name
    r.code = manifest.error.code if manifest.error else "-"
    r.elapsed = f"{elapsed:.1f}s"
    r.notes = f"final state {manifest.state.value}"
    if (
        manifest.state is session.SessionState.INTERRUPTED
        and elapsed <= DETECT_TARGET_S
        and channels_stopped(session_dir)
    ):
        r.verdict = "PASS"
        record_line('tray should have shown "Gravação interrompida: ..." within 4s - paste its exact text')
    else:
        if elapsed > DETECT_TARGET_S:
            r.notes += f" - detection {elapsed:.1f}s exceeds {DETECT_TARGET_S:.0f}s"
        if not channels_stopped(session_dir):
            r.notes += " - a channel kept growing after interruption"
    return r


def case_4_recorder_terminated(cfg: config.Config) -> Result:
    r = Result(4, "Recorder terminated")
    session_dir = start_and_wait_recording(cfg)
    manifest = session.load(session_dir)
    pid = manifest.recorder_pid
    if not pid or pid_of_pattern(f"miris.recorder {session_dir}") != pid:
        r.session = session_dir.name
        r.notes = "recorder pid missing or identity mismatch - could not inject"
        return r
    ask("  press Enter to SIGKILL the recorder NOW: ")
    os.kill(pid, signal.SIGKILL)
    time.sleep(1.0)
    stale = session.load(session_dir)
    # the CLI persists the correction; the tray derives the same thing read-only
    out = subprocess.run(
        [sys.executable, "-m", "miris.cli", "status"],
        capture_output=True, text=True,
        cwd=REPO / "packages" / "miris",
    ).stdout.strip()
    r.session = session_dir.name
    r.code = session.load(session_dir).error.code if session.load(session_dir).error else "-"
    r.notes = f"tray row must now offer 'Recuperar gravação' (status: {out or 'empty'})"
    if (
        stale.state is session.SessionState.RECORDING
        and session.load(session_dir).state is session.SessionState.INTERRUPTED
        and r.code == "recorder_gone"
    ):
        record_line("manifest was left stale (recording) and the CLI reconciled it to interrupted [recorder_gone]")
        record_line("close and REOPEN the tray menu first - an already-open menu does not refresh")
        if confirm("does the tray menu offer 'Recuperar gravação' (and 'Gravar reunião' is available again)?"):
            r.verdict = "PASS"
            record_line('tray should have shown "Gravação interrompida: ..." - paste its exact text')
        else:
            r.notes += " - tray did not surface recovery"
    else:
        r.notes += " - unexpected manifest path"
    return r


def case_5_mic_disconnected(cfg: config.Config) -> Result:
    r = Result(5, "Microphone disconnected")
    if not confirm("is an unpluggable microphone selected as the VOXIS input?"):
        r.verdict = "N/A"
        r.notes = "no unpluggable input available - keep the ledger row BLOCKED with this reason"
        return r
    session_dir = start_and_wait_recording(cfg)
    ask("  press Enter, then UNPLUG the microphone NOW: ")
    t0 = time.monotonic()
    manifest, elapsed = wait_state(
        session_dir, {session.SessionState.INTERRUPTED}, 20.0
    )
    r.session = session_dir.name
    r.code = manifest.error.code if manifest.error else "-"
    r.elapsed = f"{elapsed:.1f}s"
    r.notes = f"final state {manifest.state.value}"
    if (
        manifest.state is session.SessionState.INTERRUPTED
        and elapsed <= DETECT_TARGET_S
        and channels_stopped(session_dir)
    ):
        r.verdict = "PASS"
        record_line('tray should have shown "Gravação interrompida: ..." within 4s - paste its exact text')
    else:
        if elapsed > DETECT_TARGET_S:
            r.notes += f" - detection {elapsed:.1f}s exceeds {DETECT_TARGET_S:.0f}s"
    return r


def case_6_llm_failure(cfg: config.Config) -> Result:
    r = Result(6, "LLM failure")
    config_path = config.miris_home() / "config.toml"
    original = config_path.read_bytes()
    swapped = False
    try:
        text = original.decode("utf-8")
        import re

        new_text, n = re.subn(
            r"(?m)^llm_command\s*=.*$", 'llm_command = ["false"]', text
        )
        if n != 1:
            r.notes = "could not rewrite llm_command in config.toml"
            return r
        config_path.write_text(new_text, encoding="utf-8")
        swapped = True
        record_line("llm_command temporarily set to [\"false\"] (restored automatically at the end)")
        session_dir = start_and_wait_recording(cfg)
        ask("  record ~30s of audio, then click 'Parar gravação' in the tray; press Enter after the click: ")
        manifest, elapsed = wait_state(
            session_dir,
            {
                session.SessionState.CAPTURED,
                session.SessionState.COMPLETED,
                session.SessionState.INTERRUPTED,
            },
            300.0,
        )
        r.session = session_dir.name
        r.code = manifest.error.code if manifest.error else "-"
        r.elapsed = f"{elapsed:.1f}s"
        mic_ok = wav_size(session_dir, "mic.wav") > 44
        r.notes = f"final state {manifest.state.value}, mic.wav {'preserved' if mic_ok else 'MISSING'}"
        if (
            manifest.state is session.SessionState.CAPTURED
            and r.code == "processing_failed"
            and mic_ok
        ):
            r.verdict = "PASS"
            record_line('tray should have shown "Falha ao processar reunião: ..." and now offers "Processar gravação"')
        elif manifest.state is session.SessionState.COMPLETED:
            r.notes += " - LLM did not fail; is llm_command actually used?"
    finally:
        if swapped:
            config_path.write_bytes(original)
            record_line("original llm_command restored")
    return r


def case_7_quit_active(cfg: config.Config) -> Result:
    r = Result(7, "Quit during active work")
    tray = find_tray()
    if tray is None:
        r.notes = "no running tray at case start - open the release candidate first"
        return r
    session_dir = start_and_wait_recording(cfg)
    print("""  steps:
  1. open the tray menu: "Sair" must appear greyed out
  2. try clicking it anyway""")
    ask("  press Enter after you tried: ")
    alive = find_tray() is not None
    still = session.load(session_dir).state is session.SessionState.RECORDING
    r.session = session_dir.name
    r.notes = f"tray process {'alive' if alive else 'DIED'}, recording {'ongoing' if still else 'STOPPED'}"
    if alive and still and confirm('was "Sair" visually disabled?'):
        r.verdict = "PASS"
        record_line("paste nothing here - the evidence is the greyed-out item and the live recording")
    print("  you can stop the recording normally via the tray now.")
    return r


CASES = {
    1: ("Microphone denied", case_1_mic_denied),
    2: ("System audio denied", case_2_system_audio_denied),
    3: ("Helper terminated", case_3_helper_terminated),
    4: ("Recorder terminated", case_4_recorder_terminated),
    5: ("Microphone disconnected", case_5_mic_disconnected),
    6: ("LLM failure", case_6_llm_failure),
    7: ("Quit during active work", case_7_quit_active),
}


def main() -> None:
    args = sys.argv[1:]
    if any(not a.isdigit() for a in args):
        print(__doc__)
        raise SystemExit(2)
    unknown = [a for a in args if int(a) not in CASES]
    if unknown:
        print(f"unknown case number(s): {', '.join(unknown)}", file=sys.stderr)
        raise SystemExit(2)
    chosen = {int(n): CASES[int(n)] for n in sorted(set(args), key=int)} if args else CASES
    cfg = load_cfg()
    cfg.sessions_dir.mkdir(parents=True, exist_ok=True)
    print(f"VOXIS failure matrix harness - sessions dir: {cfg.sessions_dir}")
    tray = find_tray()
    if tray is None:
        rc = REPO / "apps/tray/src-tauri/target/release/bundle/macos/VOXIS.app"
        print("no running VOXIS tray found. Open the release candidate first:")
        print(f"  open '{rc}'")
        raise SystemExit(1)
    tray_pid, tray_path = tray
    print(f"tray under test: pid {tray_pid}: {tray_path}")
    if "target/release/bundle" not in tray_path:
        print("WARNING: this is not the release-candidate bundle - results are")
        print("not valid ledger evidence unless you copied that bundle here.")
        if not confirm("continue anyway?"):
            raise SystemExit(1)
    results: list[Result] = []
    for no, (title, fn) in chosen.items():
        try:
            results.append(fn(cfg))
        except (TimeoutError, RuntimeError) as exc:
            r = Result(no, title)
            r.notes = f"injection failed: {exc}"
            results.append(r)
        except KeyboardInterrupt:
            print("\naborted - already-run cases are summarized below")
            break
    print("\n=== paste into docs/releases/2026-09-14-v1.0.0-validation.md ===")
    for r in results:
        print(r.row())


if __name__ == "__main__":
    main()
