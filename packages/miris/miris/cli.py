"""miris - meeting recorder CLI: start | stop | status | recover | process."""
import argparse
import dataclasses
import datetime as dt
import json
import os
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path
from collections.abc import Callable

from miris import config, session


def _live_session(cfg: config.Config):
    """Newest session with a live recorder pid, or (None, None). Stale pidfiles
    (process gone) are removed on the way."""
    if not cfg.sessions_dir.exists():
        return None, None
    for session_dir in sorted(cfg.sessions_dir.iterdir(), reverse=True):
        pid_file = session_dir / "recorder.pid"
        if not pid_file.exists():
            continue
        try:
            pid = int(pid_file.read_text())
            os.kill(pid, 0)
        except (ValueError, ProcessLookupError, PermissionError):
            pid_file.unlink(missing_ok=True)  # corrupt or stale
            continue
        return session_dir, pid
    return None, None


def wait_for_state(
    session_dir: Path,
    states: set[session.SessionState],
    timeout_s: float,
    sleep: Callable[[float], None] = time.sleep,
) -> session.SessionManifest:
    deadline = time.monotonic() + timeout_s
    while True:
        manifest = session.load(session_dir)
        if manifest.state in states:
            return manifest
        if time.monotonic() >= deadline:
            return manifest
        sleep(min(0.1, max(0.0, deadline - time.monotonic())))


def _error(code: str, message: str) -> session.SessionError:
    return session.SessionError(code, message, dt.datetime.now().astimezone().isoformat())


def _mark_error(session_dir: Path, manifest: session.SessionManifest,
                code: str, message: str) -> session.SessionManifest:
    result = dataclasses.replace(manifest, error=_error(code, message),
                                 updated_at=dt.datetime.now().astimezone().isoformat())
    session.write(session_dir, result)
    return result


def _session_for(cfg: config.Config, session_id: str) -> tuple[Path, session.SessionManifest]:
    if not session_id or Path(session_id).name != session_id:
        sys.exit(f"unknown session: {session_id}")
    session_dir = cfg.sessions_dir / session_id
    if not session_dir.is_dir():
        sys.exit(f"unknown session: {session_id}")
    try:
        return session_dir, session.load(session_dir)
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as exc:
        sys.exit(f"invalid session: {session_id} ({exc})")


def stop_recorder(
    session_dir: Path,
    manifest: session.SessionManifest,
    timeout_s: float = 10.0,
) -> session.SessionManifest:
    if manifest.state is not session.SessionState.RECORDING:
        sys.exit(f"session is not recording: {manifest.state.value}")
    pid = manifest.recorder_pid
    if pid is None or not session.process_matches(pid, "miris.recorder", session_dir):
        sys.exit(f"recorder pid does not belong to {session_dir.name}")

    try:
        os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    current = wait_for_state(
        session_dir,
        {session.SessionState.CAPTURED, session.SessionState.INTERRUPTED},
        timeout_s,
    )
    if current.state in {session.SessionState.CAPTURED, session.SessionState.INTERRUPTED}:
        return current

    if session.process_matches(pid, "miris.recorder", session_dir):
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    return session.transition(
        session_dir,
        session.SessionState.INTERRUPTED,
        error=_error("stop_timeout", "recorder did not stop before deadline"),
    )


def cmd_start(cfg: config.Config) -> None:
    """Spawn the detached recorder for a fresh session."""
    cfg.sessions_dir.mkdir(parents=True, exist_ok=True)
    session.reconcile_stale(cfg.sessions_dir)
    active = session.latest_active(cfg.sessions_dir)
    if active:
        active_dir, active_manifest = active
        sys.exit(f"already recording: {active_dir.name} ({active_manifest.state.value})")
    session_dir = cfg.sessions_dir / dt.datetime.now().strftime("%Y%m%d-%H%M%S")
    session_dir.mkdir(parents=True)
    session.create(session_dir)
    # own stdio for the recorder: inheriting the CLI's pipes would keep them
    # open for the whole recording, deadlocking any caller that reads them
    # (the VOXIS tray collects this CLI's output)
    log_path = session_dir / "recorder.log"
    try:
        log = log_path.open("ab")
        subprocess.Popen([sys.executable, "-m", "miris.recorder", str(session_dir)],
                         start_new_session=True, stdin=subprocess.DEVNULL,
                         stdout=log, stderr=log)
    except OSError as exc:
        session.transition(session_dir, session.SessionState.FAILED,
                           error=_error("startup_failed", str(exc)))
        sys.exit(f"recorder failed to start: {exc}")
    finally:
        if "log" in locals():
            log.close()
    manifest = wait_for_state(
        session_dir,
        {session.SessionState.RECORDING, session.SessionState.FAILED,
         session.SessionState.INTERRUPTED},
        5.0,
    )
    if manifest.state is session.SessionState.RECORDING:
        print(f"recording -> {session_dir}")
        return
    if manifest.state in {session.SessionState.FAILED, session.SessionState.INTERRUPTED}:
        reason = manifest.error.message if manifest.error else manifest.state.value
        sys.exit(f"recorder {manifest.state.value}: {reason} ({log_path})")
    if manifest.recorder_pid is not None and session.process_matches(
        manifest.recorder_pid, "miris.recorder", session_dir
    ):
        try:
            os.kill(manifest.recorder_pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    manifest = session.load(session_dir)
    if manifest.state is session.SessionState.STARTING:
        session.transition(session_dir, session.SessionState.FAILED,
                           error=_error("startup_timeout", "recorder did not become ready"))
    sys.exit(f"recorder did not start (check mic / system-audio permissions): {log_path}")


def _run_pipeline(session_dir: Path) -> None:
    """The pipeline must be a process of its own: reconcile_stale and the tray
    verify a live processor via `ps` looking for the miris.pipeline marker
    next to the session path, which only a dedicated process can show."""
    out = subprocess.run(
        [sys.executable, "-m", "miris.pipeline", str(session_dir)], check=False
    )
    if out.returncode != 0:
        sys.exit(f"processing failed for {session_dir.name}")


def cmd_stop(cfg: config.Config) -> None:
    """Stop the live recorder and run the post-meeting pipeline."""
    cfg.sessions_dir.mkdir(parents=True, exist_ok=True)
    active = session.latest_active(cfg.sessions_dir)
    if not active:
        sys.exit("no active recording")
    session_dir, manifest = active
    if manifest.state is not session.SessionState.RECORDING:
        sys.exit(f"session is not recording: {manifest.state.value}")
    manifest = stop_recorder(session_dir, manifest)
    if manifest.state is not session.SessionState.CAPTURED:
        print(f"recording interrupted: {session_dir}")
        return
    print("recording finished, processing...")
    _run_pipeline(session_dir)


def cmd_status(cfg: config.Config) -> None:
    cfg.sessions_dir.mkdir(parents=True, exist_ok=True)
    session.reconcile_stale(cfg.sessions_dir)
    for session_dir in sorted(cfg.sessions_dir.iterdir(), reverse=True):
        if not session_dir.is_dir():
            continue
        try:
            manifest = session.load(session_dir)
        except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError):
            continue
        suffix = f" [{manifest.error.code}]" if manifest.error else ""
        print(f"{manifest.session_id} {manifest.state.value}{suffix}")
        return
    print("idle")


def cmd_retry(cfg: config.Config, session_id: str) -> None:
    """Re-run the pipeline for an existing session (cached transcript reused)."""
    cmd_process(cfg, session_id)


def cmd_recover(cfg: config.Config, session_id: str) -> None:
    session_dir, manifest = _session_for(cfg, session_id)
    if manifest.state is not session.SessionState.INTERRUPTED:
        sys.exit(f"session is not interrupted: {manifest.state.value}")
    if manifest.recorder_pid is not None and session.process_matches(
        manifest.recorder_pid, "miris.recorder", session_dir
    ):
        sys.exit(f"recorder is still running: {session_id}")
    try:
        channels = session.validate_capture(session_dir, None)
    except session.CaptureInvalid:
        for name in ("mic.wav", "system.wav"):
            path = session_dir / name
            if not path.exists():
                continue
            try:
                session.inspect_wav(path)
            except session.CaptureInvalid:
                try:
                    session.repair_wav(path)
                except session.CaptureInvalid:
                    pass  # ambiguous damage stays interrupted via re-validation
        try:
            channels = session.validate_capture(session_dir, None)
        except session.CaptureInvalid as exc:
            _mark_error(session_dir, manifest, "capture_invalid", str(exc))
            sys.exit(f"recovery failed: {exc}")
    result = session.transition(session_dir, session.SessionState.CAPTURED,
                                channels=channels, error=None)
    print(f"recovered: {result.session_id}")


def cmd_process(cfg: config.Config, session_id: str) -> None:
    session_dir, manifest = _session_for(cfg, session_id)
    if manifest.state is not session.SessionState.CAPTURED:
        sys.exit(f"session is not captured: {manifest.state.value}")
    _run_pipeline(session_dir)


def run_doctor_capture(cfg: config.Config, duration_s: float = 3.0) -> dict:
    """Bounded two-channel capture through the real recorder and helper."""
    import tempfile
    import threading

    from miris import recorder

    with tempfile.TemporaryDirectory() as tmp:
        session_dir = Path(tmp) / "doctor-capture"
        session_dir.mkdir()
        session.create(session_dir)
        stop = threading.Event()
        timer = threading.Timer(duration_s, stop.set)
        timer.start()
        try:
            state = recorder.record(session_dir, stop)
        finally:
            timer.cancel()
        if state is not session.SessionState.CAPTURED:
            raise session.CaptureInvalid(f"doctor capture ended as {state.value}")
        return session.validate_capture(session_dir, max_duration_delta_s=2.0)


def cmd_doctor(cfg: config.Config, exit_on_finish: bool = True) -> bool:
    """End-to-end smoke: binary, vault, llm, bounded two-channel capture."""
    from miris.systemtap import default_tap_factory
    ok = True

    tap_label, tap_where, tap_present = default_tap_factory(Path(os.devnull)).requirement()
    print(f"[{'ok' if tap_present else 'FAIL'}] {tap_label}: {tap_where}")
    ok &= tap_present

    print(f"[{'ok' if cfg.vault_root.is_dir() else 'FAIL'}] vault_root: {cfg.vault_root}")
    ok &= cfg.vault_root.is_dir()

    print(f"[{'ok' if cfg.categories else 'WARN'}] categories configured: "
          f"{list(cfg.categories) or 'none (everything goes to inbox)'}")

    llm_ok = False
    if cfg.llm_command:  # empty would run the prompt itself as the program
        try:
            llm = subprocess.run(cfg.llm_command + ["Responda apenas: OK"],
                                 capture_output=True, text=True, timeout=120)
            llm_ok = llm.returncode == 0 and llm.stdout.strip() != ""
        except (OSError, subprocess.TimeoutExpired):
            pass
    print(f"[{'ok' if llm_ok else 'FAIL'}] llm_command: {' '.join(cfg.llm_command)}")
    ok &= llm_ok
    llm_bin = cfg.llm_command[0] if cfg.llm_command else ""
    if llm_bin and os.sep not in llm_bin:  # resolved through this terminal's PATH only
        found = shutil.which(llm_bin)
        found = os.path.abspath(found) if found else None  # PATH may hold relative dirs
        print(f"[WARN] llm_command: the tray started from the desktop does not have your "
              f"shell PATH - write \"{found or 'the full path'}\" instead of \"{llm_bin}\"")

    if tap_present:
        try:
            channels = run_doctor_capture(cfg)
            for name, label in (("mic", "microphone capture"),
                                ("system", "system-audio capture")):
                info = channels.get(name)
                if info:
                    seconds = info.frames / info.sample_rate
                    print(f"[ok] {label}: {info.frames} frames ({seconds:.1f}s "
                          f"@ {info.sample_rate} Hz)")
                else:
                    print(f"[FAIL] {label}: no frames")
                    ok = False
        except (session.CaptureInvalid, OSError) as exc:
            print(f"[FAIL] two-channel capture (3s): {exc}")
            ok = False

    if exit_on_finish:
        sys.exit(0 if ok else 1)
    return ok


def main() -> None:
    # private meeting audio/transcripts/notes must not be world-readable;
    # inherited by the detached recorder subprocess too
    os.umask(0o077)
    parser = argparse.ArgumentParser(prog="miris")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("start")
    sub.add_parser("stop")
    sub.add_parser("status")
    retry = sub.add_parser("retry")
    retry.add_argument("session_id")
    recover = sub.add_parser("recover")
    recover.add_argument("session_id")
    process = sub.add_parser("process")
    process.add_argument("session_id")
    sub.add_parser("doctor")
    args = parser.parse_args()

    cfg = config.load()
    if args.command == "start":
        cmd_start(cfg)
    elif args.command == "stop":
        cmd_stop(cfg)
    elif args.command == "status":
        cmd_status(cfg)
    elif args.command == "retry":
        cmd_retry(cfg, args.session_id)
    elif args.command == "recover":
        cmd_recover(cfg, args.session_id)
    elif args.command == "process":
        cmd_process(cfg, args.session_id)
    elif args.command == "doctor":
        cmd_doctor(cfg)


if __name__ == "__main__":
    main()
