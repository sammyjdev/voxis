//! MIRIS integration. Session state is read straight from the session
//! manifests (running the Python CLI costs seconds of imports - too heavy for
//! a 2s poll); actions still go through the CLI binary. A live PID alone is
//! never proof of recording: process identity is verified against `ps`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Starting,
    Recording,
    Stopping,
    Captured,
    Processing,
    Completed,
    Interrupted,
    Failed,
}

impl SessionState {
    /// Nonterminal capture/processing states: quit and new starts are refused.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            SessionState::Starting
                | SessionState::Recording
                | SessionState::Stopping
                | SessionState::Processing
        )
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SessionError {
    #[allow(dead_code)]
    pub code: String,
    #[allow(dead_code)]
    pub message: String,
    #[allow(dead_code)]
    pub occurred_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ChannelInfo {
    #[allow(dead_code)]
    pub path: String,
    #[allow(dead_code)]
    pub frames: u64,
    #[allow(dead_code)]
    pub sample_rate: u32,
    #[allow(dead_code)]
    pub channels: u32,
}

/// Mirror of `session.json` schema version 1. The tray is a read-only
/// consumer: Python owns every write, including stale-state corrections.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SessionManifest {
    #[allow(dead_code)]
    pub schema_version: u32,
    pub session_id: String,
    pub state: SessionState,
    #[allow(dead_code)]
    pub created_at: String,
    #[allow(dead_code)]
    pub updated_at: String,
    pub recorder_pid: Option<i32>,
    pub processing_pid: Option<i32>,
    #[allow(dead_code)]
    pub channels: HashMap<String, Option<ChannelInfo>>,
    #[allow(dead_code)]
    pub error: Option<SessionError>,
    #[allow(dead_code)]
    pub note_path: Option<String>,
    #[allow(dead_code)]
    pub transcript_ready: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionView {
    pub path: PathBuf,
    pub manifest: SessionManifest,
    /// What the tray displays: Python's state, or the stale-state correction
    /// (dead recorder -> interrupted, dead processor -> captured).
    pub derived_state: SessionState,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MirisSnapshot {
    /// Newest session whose recorded process is verifiably alive.
    pub active: Option<SessionView>,
    /// Newest session waiting on the user: captured or (derived-)interrupted.
    /// An interrupted session must not block a new recording.
    pub pending: Option<SessionView>,
    pub last_note: Option<PathBuf>,
}

fn session_dirs_newest_first(sessions_dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(sessions_dir) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort(); // names are %Y%m%d-%H%M%S, lexicographic == chronological
    dirs.reverse();
    dirs
}

/// Unknown schema versions and unreadable manifests are skipped with a log
/// line, never deleted - a partial write must not cost the whole session.
fn parse_manifest(dir: &Path) -> Option<SessionManifest> {
    let raw = std::fs::read_to_string(dir.join("session.json")).ok()?;
    match serde_json::from_str::<SessionManifest>(&raw) {
        Ok(m) if m.schema_version == 1 => Some(m),
        Ok(m) => {
            eprintln!(
                "voxis: skipping {} (unsupported manifest schema {})",
                dir.display(),
                m.schema_version
            );
            None
        }
        Err(e) => {
            eprintln!("voxis: skipping {} (unreadable manifest: {e})", dir.display());
            None
        }
    }
}

fn ps_command(pid: i32) -> String {
    Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

/// The process must name both the marker module and this exact session dir,
/// mirroring the Python identity check: a recycled PID never looks alive.
fn command_matches(command: &str, marker: &str, dir: &Path) -> bool {
    let dir = dir.to_string_lossy().into_owned();
    command.split_whitespace().any(|t| t == marker)
        && command.split_whitespace().any(|t| t == dir)
}

fn derive_state(
    manifest: &SessionManifest,
    dir: &Path,
    command_reader: &impl Fn(i32, &Path) -> String,
) -> SessionState {
    let live = |pid: Option<i32>, marker: &str| {
        pid.filter(|pid| *pid > 0)
            .map(|pid| command_matches(&command_reader(pid, dir), marker, dir))
            .unwrap_or(false)
    };
    match manifest.state {
        SessionState::Starting | SessionState::Recording | SessionState::Stopping => {
            if live(manifest.recorder_pid, "miris.recorder") {
                manifest.state
            } else {
                SessionState::Interrupted
            }
        }
        SessionState::Processing => {
            if live(manifest.processing_pid, "miris.pipeline") {
                SessionState::Processing
            } else {
                SessionState::Captured
            }
        }
        other => other,
    }
}

/// Active and pending are selected independently so a pending recovery never
/// blocks an urgent new recording. Read-only: Python persists any stale-state
/// correction on its next command.
pub fn snapshot_with_command_reader(
    sessions_dir: &Path,
    command_reader: impl Fn(i32, &Path) -> String,
) -> MirisSnapshot {
    let mut active = None;
    let mut pending = None;
    for dir in session_dirs_newest_first(sessions_dir) {
        let Some(manifest) = parse_manifest(&dir) else { continue };
        let derived = derive_state(&manifest, &dir, &command_reader);
        let is_live_active = manifest.state.is_active() && derived == manifest.state;
        let is_pending = derived == SessionState::Captured
            || derived == SessionState::Interrupted;
        if is_live_active && active.is_none() {
            active = Some(SessionView { path: dir, manifest, derived_state: derived });
        } else if is_pending && pending.is_none() {
            pending = Some(SessionView { path: dir, manifest, derived_state: derived });
        }
        if active.is_some() && pending.is_some() {
            break;
        }
    }
    MirisSnapshot { active, pending, last_note: last_note(sessions_dir) }
}

pub fn snapshot(sessions_dir: &Path) -> MirisSnapshot {
    snapshot_with_command_reader(sessions_dir, |pid, _| ps_command(pid))
}

/// Minutes elapsed since the session started (its dir name is the timestamp).
pub fn recording_minutes(session: &str, now: chrono::DateTime<chrono::Local>) -> Option<i64> {
    let naive = chrono::NaiveDateTime::parse_from_str(session, "%Y%m%d-%H%M%S").ok()?;
    let started = naive.and_local_timezone(chrono::Local).single()?;
    Some((now - started).num_minutes().max(0))
}

/// Newest session with a note.path record whose target still exists.
pub fn last_note(sessions_dir: &Path) -> Option<PathBuf> {
    for dir in session_dirs_newest_first(sessions_dir) {
        let Ok(raw) = std::fs::read_to_string(dir.join("note.path")) else { continue };
        let path = PathBuf::from(raw.trim());
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// `miris start` - fast (the recorder detaches itself).
pub fn start(miris_bin: &str) -> Result<(), String> {
    run_cli(miris_bin, &["start"]).map(|_| ())
}

/// `miris stop` - BLOCKING: runs transcription + LLM, minutes on long
/// meetings. Callers must run it off the main thread.
pub fn stop(miris_bin: &str) -> Result<StopOutcome, String> {
    stop_outcome(&run_cli(miris_bin, &["stop"])?)
}

#[derive(Debug, Clone, PartialEq)]
pub enum StopOutcome {
    Note(PathBuf),
    Interrupted,
}

pub fn stop_outcome(stdout: &str) -> Result<StopOutcome, String> {
    for line in stdout.lines().rev() {
        if let Some(p) = line.strip_prefix("note: ") {
            return Ok(StopOutcome::Note(PathBuf::from(p.trim())));
        }
        if line.starts_with("recording interrupted:") {
            return Ok(StopOutcome::Interrupted);
        }
    }
    Err(format!("no note path in miris stop output:\n{stdout}"))
}

/// `miris recover <id>` - validates/repairs partial audio, stops at captured.
pub fn recover(miris_bin: &str, session_id: &str) -> Result<(), String> {
    run_cli(miris_bin, &["recover", session_id]).map(|_| ())
}

/// `miris process <id>` - pipeline retry for a captured session.
pub fn process(miris_bin: &str, session_id: &str) -> Result<PathBuf, String> {
    let stdout = run_cli(miris_bin, &["process", session_id])?;
    parse_note_path(&stdout)
        .ok_or_else(|| format!("no note path in miris process output:\n{stdout}"))
}

pub fn parse_note_path(stdout: &str) -> Option<PathBuf> {
    stdout
        .lines()
        .rev()
        .find_map(|l| l.strip_prefix("note: "))
        .map(|p| PathBuf::from(p.trim()))
}

fn run_cli(miris_bin: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(miris_bin)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run {miris_bin}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(3).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        // exit status prefix so an empty stderr still yields a usable error
        Err(format!(
            "miris {} exited {}: {}",
            args.join(" "),
            out.status,
            tail.join("\n")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::fs;

    fn session(dir: &std::path::Path, name: &str, pid: Option<i32>) -> std::path::PathBuf {
        let s = dir.join(name);
        fs::create_dir_all(&s).unwrap();
        if let Some(p) = pid {
            fs::write(s.join("recorder.pid"), p.to_string()).unwrap();
        }
        s
    }

    #[test]
    fn recording_minutes_from_session_name() {
        let now = chrono::Local.with_ymd_and_hms(2026, 7, 16, 10, 12, 0).unwrap();
        assert_eq!(recording_minutes("20260716-100000", now), Some(12));
        assert_eq!(recording_minutes("garbage", now), None);
    }

    #[test]
    fn last_note_newest_existing_target() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("note.md");
        fs::write(&target, "x").unwrap();
        let old = session(tmp.path(), "20260716-100000", None);
        fs::write(old.join("note.path"), target.to_string_lossy().as_bytes()).unwrap();
        let newer_without = session(tmp.path(), "20260716-200000", None);
        let _ = newer_without; // newest session has no note.path -> falls back to older
        assert_eq!(last_note(tmp.path()), Some(target));
    }

    #[test]
    fn last_note_none_when_target_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let s = session(tmp.path(), "20260716-100000", None);
        fs::write(s.join("note.path"), "/nonexistent/note.md").unwrap();
        assert_eq!(last_note(tmp.path()), None);
    }

    #[test]
    fn parse_note_path_last_note_line() {
        let out = "recording finished, processing...\nnote: /vault/x.md\n";
        assert_eq!(parse_note_path(out), Some(std::path::PathBuf::from("/vault/x.md")));
        assert_eq!(parse_note_path("idle\n"), None);
    }

    #[test]
    fn run_cli_captures_stdout_and_stderr() {
        assert_eq!(run_cli("echo", &["start"]).unwrap().trim(), "start");
        assert!(run_cli("/nonexistent-bin-xyz", &["start"]).is_err());
    }

    fn write_manifest(root: &Path, id: &str, state: &str, pid: Option<i32>) {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        let body = serde_json::json!({
            "schema_version": 1,
            "session_id": id,
            "state": state,
            "created_at": "2026-09-14T14:30:00-03:00",
            "updated_at": "2026-09-14T14:30:01-03:00",
            "recorder_pid": pid,
            "processing_pid": serde_json::Value::Null,
            "channels": {"mic": null, "system": null},
            "error": serde_json::Value::Null,
            "note_path": serde_json::Value::Null,
            "transcript_ready": false
        });
        fs::write(dir.join("session.json"), body.to_string()).unwrap();
    }

    #[test]
    fn snapshot_selects_active_and_newest_pending_independently() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(tmp.path(), "20260914-100000", "interrupted", None);
        write_manifest(tmp.path(), "20260914-110000", "captured", None);
        write_manifest(
            tmp.path(),
            "20260914-120000",
            "recording",
            Some(std::process::id() as i32),
        );
        let snap = snapshot_with_command_reader(tmp.path(), |_, dir| {
            format!("python -m miris.recorder {}", dir.display())
        });
        assert_eq!(
            snap.active.unwrap().manifest.state,
            SessionState::Recording
        );
        assert_eq!(
            snap.pending.as_ref().unwrap().manifest.session_id,
            "20260914-110000"
        );
    }

    #[test]
    fn stale_recording_is_derived_as_interrupted() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(tmp.path(), "20260914-120000", "recording", Some(99_999_999));
        let snap = snapshot_with_command_reader(tmp.path(), |_, _| String::new());
        assert!(snap.active.is_none());
        let pending = snap.pending.unwrap();
        assert_eq!(pending.derived_state, SessionState::Interrupted);
        assert_eq!(pending.manifest.state, SessionState::Recording);
    }

    #[test]
    fn stale_processing_is_derived_as_captured_for_display() {
        let tmp = tempfile::tempdir().unwrap();
        let body = serde_json::json!({
            "schema_version": 1,
            "session_id": "20260914-130000",
            "state": "processing",
            "created_at": "2026-09-14T14:30:00-03:00",
            "updated_at": "2026-09-14T14:30:01-03:00",
            "recorder_pid": serde_json::Value::Null,
            "processing_pid": 99_999_999,
            "channels": {"mic": null, "system": null},
            "error": serde_json::Value::Null,
            "note_path": serde_json::Value::Null,
            "transcript_ready": false
        });
        let dir = tmp.path().join("20260914-130000");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("session.json"), body.to_string()).unwrap();
        let snap = snapshot_with_command_reader(tmp.path(), |_, _| String::new());
        assert!(snap.active.is_none());
        assert_eq!(snap.pending.unwrap().derived_state, SessionState::Captured);
    }

    #[test]
    fn snapshot_skips_malformed_and_unknown_schema_manifests() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(tmp.path(), "20260914-100000", "captured", None);
        let bad = tmp.path().join("20260914-090000");
        fs::create_dir_all(&bad).unwrap();
        fs::write(bad.join("session.json"), "{not json").unwrap();
        let future = tmp.path().join("20260914-080000");
        fs::create_dir_all(&future).unwrap();
        let mut body = serde_json::json!({
            "schema_version": 1, "session_id": "20260914-080000",
            "state": "captured", "created_at": "x", "updated_at": "x",
            "recorder_pid": null, "processing_pid": null,
            "channels": {"mic": null, "system": null}, "error": null,
            "note_path": null, "transcript_ready": false
        });
        body["schema_version"] = serde_json::json!(2);
        fs::write(future.join("session.json"), body.to_string()).unwrap();
        let snap = snapshot_with_command_reader(tmp.path(), |_, _| String::new());
        assert_eq!(
            snap.pending.unwrap().manifest.session_id,
            "20260914-100000"
        );
    }

    #[test]
    fn completed_and_failed_are_neither_active_nor_pending() {
        let tmp = tempfile::tempdir().unwrap();
        write_manifest(tmp.path(), "20260914-100000", "completed", None);
        write_manifest(tmp.path(), "20260914-090000", "failed", None);
        let snap = snapshot_with_command_reader(tmp.path(), |_, _| String::new());
        assert!(snap.active.is_none());
        assert!(snap.pending.is_none());
    }

    #[test]
    fn stop_outcome_distinguishes_interrupted_from_note() {
        assert_eq!(
            stop_outcome("recording finished, processing...\nnote: /vault/x.md\n"),
            Ok(StopOutcome::Note(std::path::PathBuf::from("/vault/x.md")))
        );
        assert_eq!(
            stop_outcome("recording interrupted: /sessions/20260914-143000\n"),
            Ok(StopOutcome::Interrupted)
        );
        assert!(stop_outcome("idle\n").is_err());
    }
}
