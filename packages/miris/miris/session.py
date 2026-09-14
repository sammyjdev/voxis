"""Durable session lifecycle and capture metadata."""

import dataclasses
import datetime as dt
import json
import os
import re
import shlex
import struct
import subprocess
import wave
from dataclasses import dataclass, replace
from enum import StrEnum
from pathlib import Path
from collections.abc import Mapping
from typing import Any, Callable, cast


class SessionState(StrEnum):
    STARTING = "starting"
    RECORDING = "recording"
    STOPPING = "stopping"
    CAPTURED = "captured"
    PROCESSING = "processing"
    COMPLETED = "completed"
    INTERRUPTED = "interrupted"
    FAILED = "failed"


@dataclass(frozen=True)
class SessionError:
    code: str
    message: str
    occurred_at: str


@dataclass(frozen=True)
class ChannelInfo:
    path: str
    frames: int
    sample_rate: int
    channels: int


@dataclass(frozen=True)
class SessionManifest:
    schema_version: int
    session_id: str
    state: SessionState
    created_at: str
    updated_at: str
    recorder_pid: int | None
    processing_pid: int | None
    channels: dict[str, ChannelInfo | None]
    error: SessionError | None
    note_path: str | None
    transcript_ready: bool


ALLOWED = {
    SessionState.STARTING: {
        SessionState.RECORDING,
        SessionState.FAILED,
        SessionState.INTERRUPTED,
    },
    SessionState.RECORDING: {SessionState.STOPPING, SessionState.INTERRUPTED},
    SessionState.STOPPING: {SessionState.CAPTURED, SessionState.INTERRUPTED},
    SessionState.CAPTURED: {SessionState.PROCESSING},
    SessionState.PROCESSING: {SessionState.COMPLETED, SessionState.CAPTURED},
    SessionState.INTERRUPTED: {SessionState.CAPTURED},
    SessionState.COMPLETED: set(),
    SessionState.FAILED: set(),
}


class CaptureInvalid(ValueError):
    """Raised when a session does not contain usable capture files."""


def _iso(now: dt.datetime | None = None) -> str:
    return (now or dt.datetime.now().astimezone()).isoformat()


def _to_dict(manifest: SessionManifest) -> dict:
    return dataclasses.asdict(manifest)


def _from_dict(data: dict) -> SessionManifest:
    if not isinstance(data, Mapping):
        raise ValueError("invalid session manifest")
    if data.get("schema_version") != 1:
        raise ValueError("unsupported schema_version")
    channel_data = data.get("channels")
    if not isinstance(channel_data, Mapping):
        raise ValueError("invalid channels")
    channels = {
        name: ChannelInfo(**value) if value is not None else None
        for name, value in channel_data.items()
    }
    error_data = data.get("error")
    error = SessionError(**error_data) if error_data is not None else None
    return SessionManifest(
        schema_version=data["schema_version"],
        session_id=data["session_id"],
        state=SessionState(data["state"]),
        created_at=data["created_at"],
        updated_at=data["updated_at"],
        recorder_pid=data.get("recorder_pid"),
        processing_pid=data.get("processing_pid"),
        channels=channels,
        error=error,
        note_path=data.get("note_path"),
        transcript_ready=data.get("transcript_ready", False),
    )


def create(session_dir: Path, now: dt.datetime | None = None) -> SessionManifest:
    timestamp = _iso(now)
    manifest = SessionManifest(
        schema_version=1,
        session_id=session_dir.name,
        state=SessionState.STARTING,
        created_at=timestamp,
        updated_at=timestamp,
        recorder_pid=None,
        processing_pid=None,
        channels={"mic": None, "system": None},
        error=None,
        note_path=None,
        transcript_ready=False,
    )
    write(session_dir, manifest)
    return manifest


def load(session_dir: Path) -> SessionManifest:
    data = json.loads((session_dir / "session.json").read_text(encoding="utf-8"))
    return _from_dict(data)


def write(session_dir: Path, manifest: SessionManifest) -> None:
    if manifest.schema_version != 1:
        raise ValueError("unsupported schema_version")
    path = session_dir / "session.json"
    tmp = session_dir / f".session.json.{os.getpid()}.tmp"
    tmp.write_text(
        json.dumps(_to_dict(manifest), ensure_ascii=False, indent=2), encoding="utf-8"
    )
    os.replace(tmp, path)


def transition(
    session_dir: Path, state: SessionState, **changes: object
) -> SessionManifest:
    manifest = load(session_dir)
    state = SessionState(state)
    if state not in ALLOWED[manifest.state]:
        raise ValueError(f"invalid transition: {manifest.state.value} -> {state.value}")
    fields = {field.name for field in dataclasses.fields(manifest)}
    unknown = set(changes) - fields
    if unknown:
        raise TypeError(f"unknown manifest fields: {', '.join(sorted(unknown))}")
    changes["state"] = state
    changes["updated_at"] = _iso()
    if state not in {
        SessionState.STARTING,
        SessionState.RECORDING,
        SessionState.STOPPING,
    }:
        changes["recorder_pid"] = None
    if (
        manifest.state is SessionState.PROCESSING
        and state is not SessionState.PROCESSING
    ):
        changes["processing_pid"] = None
    result = replace(manifest, **cast(Any, changes))
    write(session_dir, result)
    return result


def _read_command(pid: int) -> str:
    return subprocess.run(
        ["ps", "-p", str(pid), "-o", "command="],
        capture_output=True,
        text=True,
        timeout=2,
        check=False,
    ).stdout.strip()


def process_matches(
    pid: int,
    marker: str,
    session_dir: Path,
    command_reader: Callable[[int], str] | None = None,
) -> bool:
    if pid <= 0:
        return False
    try:
        command = (command_reader or _read_command)(pid)
    except (OSError, subprocess.SubprocessError, ValueError):
        return False
    if not command:
        return False
    try:
        args = shlex.split(command)
    except ValueError:
        return False
    if marker not in args:
        return False
    expected = str(session_dir)
    return expected in args


_SESSION_NAME = re.compile(r"\d{8}-\d{6}\Z")


def _session_dirs(sessions_dir: Path, timestamp_only: bool = False) -> list[Path]:
    if not sessions_dir.is_dir():
        return []
    paths = [path for path in sessions_dir.iterdir() if path.is_dir()]
    if timestamp_only:
        paths = [path for path in paths if _SESSION_NAME.fullmatch(path.name)]
    return sorted(paths, key=lambda path: path.name, reverse=True)


def _find_state(
    sessions_dir: Path, states: set[SessionState]
) -> tuple[Path, SessionManifest] | None:
    for path in _session_dirs(sessions_dir, timestamp_only=True):
        try:
            manifest = load(path)
        except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError):
            continue
        if manifest.state in states:
            return path, manifest
    return None


def latest_active(sessions_dir: Path) -> tuple[Path, SessionManifest] | None:
    return _find_state(
        sessions_dir,
        {
            SessionState.STARTING,
            SessionState.RECORDING,
            SessionState.STOPPING,
            SessionState.PROCESSING,
        },
    )


def latest_pending(sessions_dir: Path) -> tuple[Path, SessionManifest] | None:
    return _find_state(sessions_dir, {SessionState.CAPTURED, SessionState.INTERRUPTED})


def reconcile_stale(
    sessions_dir: Path, command_reader: Callable[[int], str] | None = None
) -> list[SessionManifest]:
    changed: list[SessionManifest] = []
    for path in _session_dirs(sessions_dir):
        try:
            manifest = load(path)
        except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError):
            continue
        if manifest.state in {
            SessionState.STARTING,
            SessionState.RECORDING,
            SessionState.STOPPING,
        }:
            if manifest.recorder_pid is not None and process_matches(
                manifest.recorder_pid, "miris.recorder", path, command_reader
            ):
                continue
            result = transition(
                path,
                SessionState.INTERRUPTED,
                error=SessionError("recorder_gone", "recorder process is gone", _iso()),
            )
            changed.append(result)
        elif manifest.state is SessionState.PROCESSING:
            if manifest.processing_pid is not None and process_matches(
                manifest.processing_pid, "miris.pipeline", path, command_reader
            ):
                continue
            result = transition(
                path,
                SessionState.CAPTURED,
                error=SessionError(
                    "processor_gone", "processor process is gone", _iso()
                ),
            )
            changed.append(result)
    return changed


def inspect_wav(path: Path) -> ChannelInfo:
    try:
        with wave.open(str(path), "rb") as wav:
            sample_width = wav.getsampwidth()
            sample_rate = wav.getframerate()
            channels = wav.getnchannels()
            frames = wav.getnframes()
            compression = wav.getcomptype()
    except (OSError, wave.Error) as exc:
        raise CaptureInvalid(f"invalid WAV: {path}") from exc
    if compression != "NONE" or sample_width != 2:
        raise CaptureInvalid(f"WAV must be 16-bit PCM: {path}")
    if sample_rate <= 0 or channels not in {1, 2} or frames <= 0:
        raise CaptureInvalid(f"invalid WAV metadata: {path}")
    return ChannelInfo(
        path=path.name, frames=frames, sample_rate=sample_rate, channels=channels
    )


def repair_wav(path: Path) -> ChannelInfo:
    """Deterministic PCM RIFF repair: rebuild container lengths, keep payload.

    Supports only files whose `fmt ` chunk is readable 16-bit PCM and whose
    `data` payload is present on disk - the exact damage a killed writer
    leaves behind (stale RIFF size, zeroed data length). Anything ambiguous
    stays CaptureInvalid rather than being guessed at.
    """
    try:
        raw = path.read_bytes()
    except OSError as exc:
        raise CaptureInvalid(f"unreadable WAV: {path}") from exc
    if len(raw) < 44 or raw[:4] != b"RIFF" or raw[8:12] != b"WAVE":
        raise CaptureInvalid(f"not a RIFF/WAVE file: {path}")
    fmt: bytes | None = None
    data_offset: int | None = None
    off = 12
    while off + 8 <= len(raw):
        chunk_id = raw[off:off + 4]
        chunk_size = int.from_bytes(raw[off + 4:off + 8], "little")
        if chunk_id == b"fmt " and chunk_size >= 16 and fmt is None:
            fmt = raw[off + 8:off + 8 + 16]
        elif chunk_id == b"data":
            data_offset = off + 8
            break
        off += 8 + chunk_size + (chunk_size & 1)
    if fmt is None or data_offset is None:
        raise CaptureInvalid(f"repair needs readable fmt and data chunks: {path}")
    tag, channels, rate, _byterate, block_align, bits = struct.unpack(
        "<HHIIHH", fmt
    )
    if tag != 1 or bits != 16 or channels not in (1, 2):
        raise CaptureInvalid(f"repair supports 16-bit PCM only: {path}")
    if block_align != channels * 2:
        raise CaptureInvalid(f"inconsistent fmt block align: {path}")
    payload = raw[data_offset:]
    if len(payload) % block_align:
        payload = payload[:len(payload) - len(payload) % block_align]
    frames = len(payload) // block_align
    if frames <= 0:
        raise CaptureInvalid(f"repair found no payload frames: {path}")
    data_size = len(payload)
    repaired = (
        b"RIFF" + struct.pack("<I", 4 + (8 + 16) + (8 + data_size))
        + b"WAVE"
        + b"fmt " + struct.pack("<I", 16) + fmt
        + b"data" + struct.pack("<I", data_size) + payload
    )
    tmp = path.with_name(f".{path.name}.repair.{os.getpid()}.tmp")
    tmp.write_bytes(repaired)
    try:
        info = inspect_wav(tmp)
    except CaptureInvalid:
        tmp.unlink(missing_ok=True)
        raise
    os.replace(tmp, path)
    return ChannelInfo(
        path=path.name, frames=info.frames, sample_rate=info.sample_rate,
        channels=info.channels,
    )


def validate_capture(
    session_dir: Path, max_duration_delta_s: float | None
) -> dict[str, ChannelInfo]:
    channels: dict[str, ChannelInfo] = {}
    for name in ("mic", "system"):
        path = session_dir / f"{name}.wav"
        if not path.exists():
            raise CaptureInvalid(f"missing capture: {path.name}")
        channels[name] = inspect_wav(path)
    if max_duration_delta_s is not None:
        mic = channels["mic"]
        system = channels["system"]
        delta = abs(mic.frames / mic.sample_rate - system.frames / system.sample_rate)
        if delta > max_duration_delta_s:
            raise CaptureInvalid(
                f"duration delta {delta:.3f}s exceeds {max_duration_delta_s:.3f}s"
            )
    return channels


def ensure_legacy_manifest(session_dir: Path) -> SessionManifest:
    manifest_path = session_dir / "session.json"
    if manifest_path.exists():
        return load(session_dir)
    created_at = _iso()
    channels: dict[str, ChannelInfo | None] = {"mic": None, "system": None}
    note_path: str | None = None
    note_file = session_dir / "note.path"
    if note_file.exists():
        note_path = note_file.read_text(encoding="utf-8").strip()
    if note_path and Path(note_path).is_file():
        state = SessionState.COMPLETED
        try:
            channels.update(validate_capture(session_dir, None))
        except CaptureInvalid:
            pass
        error = None
    else:
        note_path = None
        try:
            channels.update(validate_capture(session_dir, None))
        except CaptureInvalid as exc:
            files_exist = any(
                (session_dir / name).exists() for name in ("mic.wav", "system.wav")
            )
            state = SessionState.INTERRUPTED if files_exist else SessionState.FAILED
            error = SessionError(
                "capture_invalid" if files_exist else "capture_missing",
                str(exc),
                created_at,
            )
        else:
            state = SessionState.CAPTURED
            error = None
    manifest = SessionManifest(
        schema_version=1,
        session_id=session_dir.name,
        state=state,
        created_at=created_at,
        updated_at=created_at,
        recorder_pid=None,
        processing_pid=None,
        channels=channels,
        error=error,
        note_path=note_path,
        transcript_ready=(session_dir / "transcript.md").exists(),
    )
    write(session_dir, manifest)
    return manifest
