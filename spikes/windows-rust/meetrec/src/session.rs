//! The MIRIS CLI contract the VOXIS tray drives (apps/tray/src-tauri/src/miris.rs):
//!
//!   start            create <home>/sessions/<%Y%m%d-%H%M%S>, spawn the recorder
//!                    detached, return once session.json says "recording"
//!   stop             ask the live recorder to stop (stop file), wait for it,
//!                    then run the pipeline; last stdout line "note: <path>"
//!   process <id>     rerun the pipeline on a captured session
//!   recover <id>     repair WAV headers of a killed recorder, mark captured
//!
//! The tray proves a pid alive by its command line naming the marker
//! ("miris.recorder" / "miris.pipeline") and the session dir, so both children
//! are spawned as `meetrec <marker> <dir>`. session.json is schema 1.
//! <home> is MIRIS_HOME or %USERPROFILE%\.miris, the tray's default miris_home.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

fn miris_home() -> PathBuf {
    std::env::var_os("MIRIS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| super::home().join(".miris"))
}

fn sessions_dir() -> PathBuf {
    miris_home().join("sessions")
}

fn now() -> String {
    chrono::Local::now().to_rfc3339()
}

fn read(dir: &Path) -> Result<Value, String> {
    let raw = std::fs::read_to_string(dir.join("session.json")).map_err(|e| format!("{}: {e}", dir.display()))?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

/// tmp + rename: the tray polls this file every 2 s
fn write(dir: &Path, m: &Value) -> Result<(), String> {
    let tmp = dir.join(".session.json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(m).unwrap()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join("session.json")).map_err(|e| e.to_string())
}

fn update(dir: &Path, f: impl FnOnce(&mut Value)) -> Result<(), String> {
    let mut m = read(dir)?;
    f(&mut m);
    m["updated_at"] = json!(now());
    write(dir, &m)
}

fn fail(dir: &Path, state: &str, code: &str, message: &str) {
    let _ = update(dir, |m| {
        m["state"] = json!(state);
        m["error"] = json!({"code": code, "message": message, "occurred_at": now()});
    });
}

/// The recording device chosen in the tray (config.toml `mic`).
pub fn configured_mic() -> Option<String> {
    super::config_value("mic")
}

/// CreateProcess hands a child every inheritable handle, including the stdout
/// pipe our caller (the tray, a shell) is reading to EOF. A detached recorder
/// holding it would block that caller until the meeting ends, so our own std
/// handles stop being inheritable; explicit Stdio handles are duplicated anyway.
#[cfg(windows)]
fn stop_inheriting_std_handles() {
    use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT};
    use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
    for h in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        unsafe {
            SetHandleInformation(GetStdHandle(h), HANDLE_FLAG_INHERIT, 0);
        }
    }
}

fn spawn_child(marker: &str, dir: &Path, detached: bool) -> Result<std::process::Child, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.arg(marker).arg(dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(if detached {
            DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
        } else {
            CREATE_NO_WINDOW
        });
    }
    if detached {
        #[cfg(windows)]
        stop_inheriting_std_handles();
        // the tray reads start's stdout to completion; a detached child must
        // not hold that pipe open, so its output goes to a log file
        let log = std::fs::File::create(dir.join("recorder.log")).map_err(|e| e.to_string())?;
        cmd.stdout(log.try_clone().map_err(|e| e.to_string())?).stderr(log).stdin(std::process::Stdio::null());
    }
    cmd.spawn().map_err(|e| format!("spawn {marker}: {e}"))
}

/// Whether the process a session names is alive as its owner, and ending one.
pub trait Owners {
    fn live(&self, pid: Option<u64>, marker: &str, dir: &Path) -> bool;
    /// Ends the process; true when the end was requested.
    fn end(&self, pid: u64) -> bool;
}

/// A process as `sysinfo` reads it: its image and its argv elements.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcInfo {
    pub exe: Option<PathBuf>,
    pub args: Vec<String>,
}

pub fn process_identity(pid: u32) -> Option<ProcInfo> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let pid = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always).with_exe(UpdateKind::Always),
    );
    let p = sys.process(pid)?;
    Some(ProcInfo {
        exe: p.exe().map(Path::to_path_buf),
        args: p.cmd().iter().map(|a| a.to_string_lossy().into_owned()).collect(),
    })
}

/// The owner of a session: this `meetrec` image, run with exactly the marker
/// and the session folder as its arguments, compared element by element (a
/// profile path with spaces stays one element), the folder as a path.
fn is_owner(info: &ProcInfo, me: &Path, marker: &str, dir: &Path) -> bool {
    let same_image = info.exe.as_deref().is_some_and(|exe| {
        exe == me || matches!((std::fs::canonicalize(exe), std::fs::canonicalize(me)), (Ok(a), Ok(b)) if a == b)
    });
    same_image && info.args.len() == 3 && info.args[1] == marker && Path::new(&info.args[2]) == dir
}

/// The real processes of this machine.
struct SysOwners;

impl Owners for SysOwners {
    fn live(&self, pid: Option<u64>, marker: &str, dir: &Path) -> bool {
        let Ok(me) = std::env::current_exe() else { return false };
        pid.and_then(|p| u32::try_from(p).ok())
            .and_then(process_identity)
            .is_some_and(|info| is_owner(&info, &me, marker, dir))
    }

    fn end(&self, pid: u64) -> bool {
        use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
        let Ok(pid) = u32::try_from(pid).map(Pid::from_u32) else { return false };
        let mut sys = System::new();
        sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing());
        let ended = sys.process(pid).is_some_and(|p| p.kill());
        // TerminateProcess returns before the process is gone
        for _ in 0..30 {
            sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing());
            if sys.process(pid).is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        ended
    }
}

/// The pid and marker of the process that owns a session in `state`.
fn owner_of(m: &Value) -> Option<(Option<u64>, &'static str)> {
    match m["state"].as_str()? {
        "starting" | "recording" | "stopping" => Some((m["recorder_pid"].as_u64(), "miris.recorder")),
        "processing" => Some((m["processing_pid"].as_u64(), "miris.pipeline")),
        _ => None,
    }
}

fn session_dirs_newest_first(sessions: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(sessions)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_dir()).collect())
        .unwrap_or_default();
    dirs.sort();
    dirs.reverse();
    dirs
}

/// Whether a new session may start: refused while an active session has a
/// live owner; an active session whose owner died is settled first, keeping
/// every file (recorder -> interrupted, pipeline -> captured, `owner_died`).
fn admit(sessions: &Path, owners: &dyn Owners) -> Result<(), String> {
    for dir in session_dirs_newest_first(sessions) {
        let Ok(m) = read(&dir) else { continue };
        let Some((pid, marker)) = owner_of(&m) else { continue };
        if owners.live(pid, marker, &dir) {
            return Err(format!("a session is already active: {}", dir.display()));
        }
        if marker == "miris.recorder" {
            fail(&dir, "interrupted", "owner_died", "the recorder ended without finishing the recording");
        } else {
            fail(&dir, "captured", "owner_died", "the pipeline ended before the note; process retries it");
        }
    }
    Ok(())
}

/// Waits for the recorder to report `recording`. A recorder that exits
/// before that fails the session with `recorder_error`, so no session is
/// left in `starting`.
fn wait_until_recording(dir: &Path, mut exited: impl FnMut() -> bool, tries: u32, every: std::time::Duration) -> Result<(), String> {
    let tail = || {
        let log = std::fs::read_to_string(dir.join("recorder.log")).unwrap_or_default();
        log.lines().filter(|l| !l.trim().is_empty()).last().unwrap_or("no output").to_owned()
    };
    for _ in 0..tries {
        std::thread::sleep(every);
        match read(dir)?["state"].as_str() {
            Some("recording") => {
                println!("recording: {}", dir.display());
                return Ok(());
            }
            Some("failed") | Some("interrupted") => return Err(format!("recorder did not start: {}", tail())),
            _ => {}
        }
        if exited() {
            let e = format!("recorder exited before recording: {}", tail());
            fail(dir, "failed", "recorder_error", &e);
            return Err(e);
        }
    }
    Err(format!("recorder did not start: {}", tail()))
}

/// `process` and `recover` change nothing while the session's owner lives.
fn guard_inactive(dir: &Path, owners: &dyn Owners) -> Result<(), String> {
    let m = read(dir)?;
    match owner_of(&m) {
        Some((pid, marker)) if owners.live(pid, marker, dir) => Err(format!("session is active: {}", dir.display())),
        _ => Ok(()),
    }
}

/// `transcript.md` stands for this session's audio when the manifest says
/// it is ready, it is not empty, and it is newer than both recordings.
fn transcript_fresh(dir: &Path, transcript_ready: bool) -> bool {
    let modified = |f: &str| std::fs::metadata(dir.join(f)).and_then(|m| m.modified()).ok();
    let Some(t) = modified("transcript.md") else { return false };
    let not_empty = std::fs::metadata(dir.join("transcript.md")).is_ok_and(|m| m.len() > 0);
    transcript_ready && not_empty && ["mic.wav", "system.wav"].iter().all(|w| modified(w).is_none_or(|m| t > m))
}

/// Milliseconds since the epoch, for the meeting latency marks.
fn unix_ms() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

/// The transcript is on disk: the manifest says so, and the meeting latency
/// mark `transcript_ready <unix_ms>` goes to the output.
fn transcribed(dir: &Path, out: &mut dyn std::io::Write) -> Result<(), String> {
    update(dir, |m| m["transcript_ready"] = json!(true))?;
    let _ = writeln!(out, "transcript_ready {}", unix_ms());
    Ok(())
}

fn start() -> Result<(), String> {
    admit(&sessions_dir(), &SysOwners)?;
    let id = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let dir = create_session_dir(&sessions_dir(), &id)?;
    let t = now();
    write(&dir, &json!({
        "schema_version": 1, "session_id": id, "state": "starting", "created_at": t, "updated_at": t,
        "recorder_pid": null, "processing_pid": null, "channels": {"mic": null, "system": null},
        "error": null, "note_path": null, "transcript_ready": false,
    }))?;
    let mut child = spawn_child("miris.recorder", &dir, true)?;
    update(&dir, |m| m["recorder_pid"] = json!(child.id()))?;
    // opening WASAPI takes about a second
    wait_until_recording(&dir, || matches!(child.try_wait(), Ok(Some(_))), 100, std::time::Duration::from_millis(100))
}

/// A pipeline that failed: back to `captured` so the tray offers "Processar
/// gravação"; every file stays.
fn pipeline_failed(dir: &Path, e: &str) {
    let code = if e.contains(super::minutes::DEADLINE_MARK) { "provider_timeout" } else { "pipeline_error" };
    fail(dir, "captured", code, e);
}

/// A recorder that failed: `failed`, with `disk_write_error` when a track
/// could not be written (both WAV headers are already finalized), else
/// `recorder_error`. Every file stays.
fn recorder_failed(dir: &Path, e: &str) {
    let code = if e.starts_with("disk_write_error") { "disk_write_error" } else { "recorder_error" };
    fail(dir, "failed", code, e);
}

/// `sessions/<id>` for a new session, created with a call that fails when
/// the folder exists: a session folder is never reused.
fn create_session_dir(sessions: &Path, id: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(sessions).map_err(|e| format!("{}: {e}", sessions.display()))?;
    let dir = sessions.join(id);
    std::fs::create_dir(&dir).map_err(|e| format!("session folder {}: {e}", dir.display()))?;
    Ok(dir)
}

fn recorder(dir: &Path) -> Result<(), String> {
    let stop_file = dir.join("stop");
    let mic = configured_mic();
    let started = || {
        let _ = update(dir, |m| m["state"] = json!("recording"));
    };
    let (mic_t, sys_t) = super::record(dir, mic.as_deref(), super::Stop::File(stop_file.clone()), started)?;
    let _ = std::fs::remove_file(&stop_file);
    let channel = |name: &str, t: &super::Track| {
        json!({"path": dir.join(name).to_string_lossy(), "frames": t.frames, "sample_rate": t.rate, "channels": 1})
    };
    update(dir, |m| {
        m["state"] = json!("captured");
        m["channels"] = json!({"mic": channel("mic.wav", &mic_t), "system": channel("system.wav", &sys_t)});
    })
}

fn pipeline(dir: &Path) -> Result<PathBuf, String> {
    let ready = read(dir)?["transcript_ready"].as_bool().unwrap_or(false);
    update(dir, |m| {
        m["state"] = json!("processing");
        m["processing_pid"] = json!(std::process::id());
        m["error"] = Value::Null;
    })?;
    // a retry keeps a transcript that already covers both recordings
    if !transcript_fresh(dir, ready) {
        let (model, vad) = super::models()?;
        super::transcribe(dir, &model.to_string_lossy(), &vad.to_string_lossy(), super::LANGUAGE);
    }
    transcribed(dir, &mut std::io::stdout())?;
    let ata = super::minutes::run(dir)?;
    finish(dir, ata)
}

/// Completes the session and returns its note: the Doc's .url, or ata.md
/// when Google is not set up, or the transcript when no minutes were written.
/// "Última nota" opens it with the default handler.
fn finish(dir: &Path, ata: super::minutes::Ata) -> Result<PathBuf, String> {
    use super::minutes::Ata;
    let note = match ata {
        Ata::Doc(title, link) => {
            let safe: String = title.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) { '-' } else { c }).collect();
            let url = dir.join(format!("{safe}.url"));
            std::fs::write(&url, format!("[InternetShortcut]\r\nURL={link}\r\n")).map_err(|e| e.to_string())?;
            url
        }
        Ata::Local(path) => path,
        Ata::NotWritten => dir.join("transcript.md"),
    };
    std::fs::write(dir.join("note.path"), note.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    update(dir, |m| {
        m["state"] = json!("completed");
        m["note_path"] = json!(note.to_string_lossy());
    })?;
    Ok(note)
}

/// Runs the pipeline as a `miris.pipeline <dir>` child so the tray can see it
/// alive; its stdout (ending in "note: ...") goes straight to our caller.
fn run_pipeline(dir: &Path) -> Result<(), String> {
    let status = spawn_child("miris.pipeline", dir, false)?.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("pipeline failed ({status}); see {}", dir.join("session.json").display()))
    }
}

fn newest_with_state(states: &[&str]) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(sessions_dir()).ok()?.filter_map(|e| e.ok().map(|e| e.path())).collect();
    dirs.sort();
    dirs.into_iter()
        .rev()
        .find(|d| read(d).ok().and_then(|m| m["state"].as_str().map(|s| states.contains(&s))).unwrap_or(false))
}

fn stop() -> Result<(), String> {
    let dir = newest_with_state(&["recording", "starting"]).ok_or("no recording session")?;
    stop_with(&dir, &SysOwners, 600, std::time::Duration::from_millis(100), &mut std::io::stdout(), run_pipeline)
}

/// `stop` on `dir`: the stop file only for a verified recorder; `tries`
/// polls `every` apart for `captured`, then `stopped <unix_ms>` and the
/// pipeline. A recorder that never finishes is ended (identity checked
/// again) and the session marked `interrupted`; one that cannot be ended
/// keeps `stopping`, so `start` keeps refusing.
fn stop_with(
    dir: &Path,
    owners: &dyn Owners,
    tries: u32,
    every: std::time::Duration,
    out: &mut dyn std::io::Write,
    pipeline: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let pid = read(dir)?["recorder_pid"].as_u64();
    if !owners.live(pid, "miris.recorder", dir) {
        fail(dir, "interrupted", "owner_died", "the recorder ended without finishing the recording");
        let _ = writeln!(out, "recording interrupted: {}", dir.display());
        return Ok(());
    }
    update(dir, |m| m["state"] = json!("stopping"))?;
    std::fs::write(dir.join("stop"), b"").map_err(|e| e.to_string())?;
    // the recorder flips the manifest to captured once both WAVs are final
    for _ in 0..tries {
        std::thread::sleep(every);
        let state = read(dir)?["state"].clone();
        if state == "captured" {
            let _ = writeln!(out, "stopped {}", unix_ms());
            let _ = out.flush();
            return pipeline(dir);
        }
        // the recorder ended on its own with a failure (disk_write_error, recorder_error): its
        // manifest already carries the cause and both files stay; waiting out the 60 s would
        // replace that cause with recorder_timeout
        if state == "failed" || state == "interrupted" {
            let _ = writeln!(out, "recording interrupted: {}", dir.display());
            return Ok(());
        }
    }
    if let Some(p) = pid.filter(|_| owners.live(pid, "miris.recorder", dir)) {
        owners.end(p);
    }
    if owners.live(pid, "miris.recorder", dir) {
        return Err(format!("recorder did not stop; owner still live: {}", dir.display()));
    }
    fail(dir, "interrupted", "recorder_timeout", "recorder did not finish within 60 s; stop ended it");
    let _ = writeln!(out, "recording interrupted: {}", dir.display());
    Ok(())
}

/// The header sizes of a WAV whose writer could not finalize it (a failed
/// flush on a full disk): they follow from the bytes that did reach the
/// file, cut to whole f32 samples. Only existing bytes are overwritten, so
/// it works on a disk with no free space left.
pub(crate) fn repair_wav_in<F: std::io::Read + std::io::Write + std::io::Seek>(f: &mut F) -> Result<(), String> {
    use std::io::{Read, SeekFrom};
    let len = f.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    if len < 44 {
        return Err(format!("only {len} bytes reached the file, too short to repair"));
    }
    if len - 8 > u64::from(u32::MAX) {
        return Err(format!("{len} bytes: larger than a WAV header can describe"));
    }
    f.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut head = Vec::new();
    f.take(4096).read_to_end(&mut head).map_err(|e| e.to_string())?;
    let data_offset = head.windows(4).position(|w| w == b"data").ok_or("no data chunk")? as u64 + 8;
    if len < data_offset {
        return Err(format!("only {len} bytes reached the file, the data chunk was never started"));
    }
    let data_len = (len - data_offset) & !3;
    f.seek(SeekFrom::Start(4)).map_err(|e| e.to_string())?;
    f.write_all(&((len - 8) as u32).to_le_bytes()).map_err(|e| e.to_string())?;
    f.seek(SeekFrom::Start(data_offset - 4)).map_err(|e| e.to_string())?;
    f.write_all(&(data_len as u32).to_le_bytes()).map_err(|e| e.to_string())?;
    f.flush().map_err(|e| e.to_string())
}

/// The bytes every track this recorder writes holds at the start (hound 3.5.1, 32-bit float
/// mono, extensible header: 68 bytes); sizes, rate, byte rate and channel mask vary.
const PRODUCT_HEADER_FIXED: [(usize, &[u8]); 12] = [
    (0, b"RIFF"),
    (8, b"WAVE"),
    (12, b"fmt "),
    (16, &[40, 0, 0, 0]),
    (20, &[0xFE, 0xFF]),
    (22, &[1, 0]),
    (32, &[4, 0]),
    (34, &[32, 0]),
    (36, &[22, 0]),
    (38, &[32, 0]),
    (44, &[0x03, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71]),
    (60, b"data"),
];

/// True when every fixed header byte that `head` covers (at most its first 68) holds.
fn product_header_holds(head: &[u8]) -> bool {
    PRODUCT_HEADER_FIXED
        .iter()
        .all(|(at, want)| want.iter().enumerate().all(|(i, w)| head.get(at + i).map_or(true, |b| b == w)))
}

/// Writes the RIFF size at bytes 4..8 and the data size at bytes 64..68 of a track, and nothing else.
fn write_sizes(name: &str, path: &Path, riff: u64, data: u64) -> Result<(), String> {
    use std::io::{Seek, SeekFrom, Write};
    let too_big = || format!("{name}: unreadable (larger than a WAV header can describe)");
    let (riff, data) = (u32::try_from(riff).map_err(|_| too_big())?, u32::try_from(data).map_err(|_| too_big())?);
    let mut f = std::fs::OpenOptions::new().write(true).open(path).map_err(|e| format!("{name}: {e}"))?;
    f.seek(SeekFrom::Start(4)).and_then(|_| f.write_all(&riff.to_le_bytes())).map_err(|e| format!("{name}: {e}"))?;
    f.seek(SeekFrom::Start(64)).and_then(|_| f.write_all(&data.to_le_bytes())).map_err(|e| format!("{name}: {e}"))?;
    f.flush().map_err(|e| format!("{name}: {e}"))
}

/// At most the first 68 bytes of a track.
fn read_head(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut head = Vec::new();
    std::fs::File::open(path).and_then(|f| f.take(68).read_to_end(&mut head)).map_err(|e| e.to_string())?;
    Ok(head)
}

/// A killed recorder leaves WAV headers with zero sizes; hound writes the
/// 44-byte canonical header for f32 mono, so sizes follow from the file length.
fn repair_wav(path: &Path) -> Result<(), String> {
    use std::io::{Seek, SeekFrom, Write};
    let len = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if len < 44 {
        return Err(format!("{} is too short to repair", path.display()));
    }
    if hound::WavReader::open(path).map(|r| r.len() > 0).unwrap_or(false) {
        return Ok(());
    }
    let mut f = std::fs::OpenOptions::new().write(true).open(path).map_err(|e| e.to_string())?;
    let data_offset = {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        bytes.windows(4).position(|w| w == b"data").ok_or("no data chunk")? as u64 + 8
    };
    f.seek(SeekFrom::Start(4)).map_err(|e| e.to_string())?;
    f.write_all(&((len - 8) as u32).to_le_bytes()).map_err(|e| e.to_string())?;
    f.seek(SeekFrom::Start(data_offset - 4)).map_err(|e| e.to_string())?;
    f.write_all(&((len - data_offset) as u32).to_le_bytes()).map_err(|e| e.to_string())
}

fn session_dir(id: &str) -> Result<PathBuf, String> {
    let ok = id.len() == 15 && id.chars().enumerate().all(|(i, c)| if i == 8 { c == '-' } else { c.is_ascii_digit() });
    if !ok {
        return Err(format!("bad session id {id}"));
    }
    Ok(sessions_dir().join(id))
}

pub fn cli(cmd: &str, args: &[String]) -> i32 {
    cli_with(&SysOwners, cmd, args)
}

/// `cli` with the owner check passed in, so a test can reach `process` and
/// `recover` through their real routing.
fn cli_with(owners: &dyn Owners, cmd: &str, args: &[String]) -> i32 {
    cli_to(owners, cmd, args, &mut std::io::stdout(), &mut std::io::stderr())
}

/// `cli_with` with its printed lines going to `out` and its error line to `err`, so a test can
/// read what the person at the console sees.
fn cli_to(owners: &dyn Owners, cmd: &str, args: &[String], out: &mut dyn std::io::Write, err: &mut dyn std::io::Write) -> i32 {
    let arg = |i: usize| args.get(i).map(String::as_str).ok_or_else(|| format!("{cmd}: missing argument"));
    let result = match cmd {
        "start" => start(),
        "stop" => stop(),
        "process" => arg(0).and_then(session_dir).and_then(|d| {
            guard_inactive(&d, owners)?;
            run_pipeline(&d)
        }),
        "recover" => arg(0).and_then(session_dir).and_then(|d| {
            guard_inactive(&d, owners)?;
            // every track is looked at before any is written, so a refusal changes nothing
            let mut tracks = Vec::new();
            for f in ["mic.wav", "system.wav"] {
                let p = d.join(f);
                if !p.exists() {
                    continue;
                }
                let len = std::fs::metadata(&p).map_err(|e| e.to_string())?.len();
                let head = read_head(&p)?;
                let holds = product_header_holds(&head);
                if len >= 1 && !holds {
                    return Err(format!("{f}: unreadable (not a product WAV header)"));
                }
                tracks.push((f, p, len, holds, head));
            }
            let mut audio = false;
            for (f, p, len, holds, head) in tracks {
                // W: bytes of whole samples present; D: the declared data size
                let w = len.saturating_sub(68) / 4 * 4;
                let declared = head.get(64..68).map(|b| u64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])));
                audio |= holds && w >= 4;
                let line = match len {
                    0 => format!("{f}: empty (0 bytes), no audio captured, skipped"),
                    1..=67 => format!("{f}: partial header ({len} bytes), no audio captured, skipped"),
                    68..=71 if holds => format!("{f}: no samples, skipped"),
                    _ if holds && w >= 4 && declared == Some(w) => continue,
                    _ if holds && w >= 4 && declared.is_some_and(|d| d == 0 || d > w) => {
                        write_sizes(f, &p, 60 + w, w)?;
                        let old_riff = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
                        format!("{f}: riff_size {old_riff} -> {}, data_size {} -> {w}", 60 + w, declared.unwrap_or(0))
                    }
                    _ => {
                        repair_wav(&p)?;
                        continue;
                    }
                };
                writeln!(out, "{line}").unwrap_or_else(|e| panic!("failed printing to stdout: {e}"));
            }
            update(&d, |m| {
                if audio {
                    m["state"] = json!("captured");
                    m["error"] = Value::Null;
                } else {
                    m["state"] = json!("failed");
                    m["error"] = json!({"code": "no_audio", "message": "no track holds audio; the files were kept", "occurred_at": now()});
                }
            })
        }),
        "miris.recorder" => arg(0).map(PathBuf::from).and_then(|d| recorder(&d).inspect_err(|e| recorder_failed(&d, e))),
        "miris.pipeline" => arg(0).map(PathBuf::from).and_then(|d| match pipeline(&d) {
            Ok(note) => {
                writeln!(out, "note: {}", note.display()).unwrap_or_else(|e| panic!("failed printing to stdout: {e}"));
                Ok(())
            }
            Err(e) => {
                pipeline_failed(&d, &e);
                Err(e)
            }
        }),
        _ => Err(format!("unknown command {cmd}")),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            writeln!(err, "{e}").unwrap_or_else(|e| panic!("failed printing to stderr: {e}"));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minutes::{run_with, Credentials};

    /// A session folder the pipeline left after transcribing.
    fn transcribed_session() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("session.json"), r#"{"schema": 1, "state": "processing"}"#).unwrap();
        std::fs::write(dir.path().join("transcript.md"), "# Transcrição\n\n**[00:01] Você:** vamos fechar sexta\n").unwrap();
        dir
    }

    #[test]
    fn without_google_credentials_the_minutes_stay_in_ata_md() {
        let session = transcribed_session();
        let creds = Credentials { anthropic_key: Some("key".into()), google: false };
        let minutes = |_: &str| -> Result<Value, String> {
            Ok(json!({"titulo": "Alinhamento", "resumo": "Fechar sexta.", "decisoes": [], "action_items": []}))
        };

        let ata = run_with(session.path(), &creds, &minutes).unwrap();
        let note = finish(session.path(), ata).unwrap();

        assert_eq!(note, session.path().join("ata.md"));
        assert!(std::fs::read_to_string(&note).unwrap().contains("Fechar sexta."));
        assert_eq!(read(session.path()).unwrap()["state"], "completed");
    }

    #[test]
    fn without_an_anthropic_key_the_session_completes_with_the_transcript() {
        let session = transcribed_session();
        let creds = Credentials { anthropic_key: None, google: true };
        let minutes = |_: &str| -> Result<Value, String> { panic!("no key: the minutes must not be asked for") };

        let ata = run_with(session.path(), &creds, &minutes).unwrap();
        let note = finish(session.path(), ata).unwrap();

        assert_eq!(note, session.path().join("transcript.md"));
        assert!(!session.path().join("ata.md").exists());
        assert_eq!(read(session.path()).unwrap()["state"], "completed");
    }

    // ------------------------------------------------------------ S5 provider failures and notes

    /// A session the pipeline left with a transcript and minutes on disk.
    fn processed_session() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &json!({"schema_version": 1, "state": "processing", "transcript_ready": true})).unwrap();
        std::fs::write(dir.path().join("transcript.md"), "# Transcrição\n\nfechar sexta\n").unwrap();
        std::fs::write(dir.path().join("ata.md"), "# Alinhamento\n").unwrap();
        dir
    }

    #[test]
    fn a_provider_timeout_returns_the_session_to_captured_and_keeps_the_files() {
        let s = processed_session();

        pipeline_failed(s.path(), "anthropic: no answer within 300 s (deadline)");

        let m = read(s.path()).unwrap();
        assert_eq!(m["state"], "captured");
        assert_eq!(m["error"]["code"], "provider_timeout");
        assert!(s.path().join("transcript.md").is_file() && s.path().join("ata.md").is_file());
    }

    #[test]
    fn a_malformed_provider_answer_returns_to_captured_with_the_files() {
        let s = processed_session();

        pipeline_failed(s.path(), "anthropic: minutes json: expected value at line 1 column 1: <html>");

        let m = read(s.path()).unwrap();
        assert_eq!(m["state"], "captured");
        assert_eq!(m["error"]["code"], "pipeline_error");
        assert!(m["error"]["message"].as_str().unwrap().starts_with("anthropic: minutes json"));
        assert!(s.path().join("transcript.md").is_file() && s.path().join("ata.md").is_file());
    }

    #[test]
    fn a_drive_upload_makes_the_note_a_url_shortcut_to_the_doc() {
        let session = transcribed_session();
        let creds = Credentials { anthropic_key: Some("key".into()), google: true };
        let minutes = |_: &str| -> Result<Value, String> {
            Ok(json!({"titulo": "Alinhamento", "resumo": "Fechar sexta.", "decisoes": [], "action_items": []}))
        };
        let drive = |_: &str, _: &str| -> Result<Value, String> { Ok(json!({"webViewLink": "https://docs.google.com/document/d/D"})) };

        let ata = crate::minutes::run_with_drive(session.path(), &creds, &minutes, &drive).unwrap();
        let note = finish(session.path(), ata).unwrap();

        assert_eq!(note.extension().unwrap(), "url");
        assert_eq!(note.parent().unwrap(), session.path());
        assert!(std::fs::read_to_string(&note).unwrap().contains("URL=https://docs.google.com/document/d/D"));
        assert_eq!(read(session.path()).unwrap()["note_path"], note.to_string_lossy().as_ref());
    }

    // ------------------------------------------------------------ S2 session folders and recover

    #[test]
    fn start_refuses_a_session_folder_that_already_exists() {
        let sessions = tempfile::tempdir().unwrap();
        let taken = sessions.path().join("20261107-143200");
        std::fs::create_dir_all(&taken).unwrap();
        std::fs::write(taken.join("mic.wav"), b"earlier").unwrap();

        let refused = create_session_dir(sessions.path(), "20261107-143200");
        let fresh = create_session_dir(sessions.path(), "20261107-143201");

        assert!(refused.is_err());
        assert_eq!(std::fs::read(taken.join("mic.wav")).unwrap(), b"earlier");
        assert_eq!(fresh, Ok(sessions.path().join("20261107-143201")));
    }

    /// A WAV as the recorder writes it, `n` samples.
    fn wav(path: &Path, n: usize) {
        let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..n {
            w.write_sample((i as f32 / n as f32) - 0.5).unwrap();
        }
        w.finalize().unwrap();
    }

    fn data_offset(bytes: &[u8]) -> usize {
        bytes.windows(4).position(|w| w == b"data").unwrap() + 8
    }

    #[test]
    fn recover_fixes_only_the_header_sizes_and_keeps_every_sample_byte() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mic.wav");
        wav(&path, 1000);
        let mut killed = std::fs::read(&path).unwrap();
        let off = data_offset(&killed);
        // a killed recorder never rewrote the sizes: both still say zero
        killed[4..8].copy_from_slice(&0u32.to_le_bytes());
        killed[off - 4..off].copy_from_slice(&0u32.to_le_bytes());
        std::fs::write(&path, &killed).unwrap();

        repair_wav(&path).unwrap();

        let fixed = std::fs::read(&path).unwrap();
        assert_eq!(fixed.len(), killed.len());
        assert_eq!(fixed[off..], killed[off..], "sample bytes unchanged");
        assert_eq!(fixed[8..off - 4], killed[8..off - 4], "only the size fields change");
        assert_eq!(hound::WavReader::open(&path).unwrap().len(), 1000);
    }

    #[test]
    fn a_disk_write_error_fails_the_session_and_keeps_both_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("session.json"), r#"{"schema_version": 1, "state": "recording"}"#).unwrap();
        wav(&dir.path().join("mic.wav"), 100);
        wav(&dir.path().join("system.wav"), 100);
        let (mic, sys) = (std::fs::read(dir.path().join("mic.wav")).unwrap(), std::fs::read(dir.path().join("system.wav")).unwrap());

        recorder_failed(dir.path(), "disk_write_error: disk full");

        let m = read(dir.path()).unwrap();
        assert_eq!(m["state"], "failed");
        assert_eq!(m["error"]["code"], "disk_write_error");
        assert_eq!(std::fs::read(dir.path().join("mic.wav")).unwrap(), mic);
        assert_eq!(std::fs::read(dir.path().join("system.wav")).unwrap(), sys);
    }

    #[test]
    fn recover_leaves_a_valid_wav_byte_identical() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("system.wav");
        wav(&path, 500);
        let before = std::fs::read(&path).unwrap();

        repair_wav(&path).unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    // ------------------------------------------------------------ S3 owners (F19, F23, AC 12-17, 85, 86)

    /// Owners whose liveness the test decides; `end` kills only when allowed.
    struct Fake {
        live: std::cell::Cell<bool>,
        can_end: bool,
    }

    impl Owners for Fake {
        fn live(&self, _: Option<u64>, _: &str, _: &Path) -> bool {
            self.live.get()
        }
        fn end(&self, _: u64) -> bool {
            if self.can_end {
                self.live.set(false);
            }
            self.can_end
        }
    }

    fn alive() -> Fake {
        Fake { live: std::cell::Cell::new(true), can_end: true }
    }

    fn dead() -> Fake {
        Fake { live: std::cell::Cell::new(false), can_end: true }
    }

    /// A schema 1 manifest in `sessions/<id>` with a recording beside it.
    fn session_in(sessions: &Path, id: &str, state: &str, recorder: Option<u64>, pipeline: Option<u64>) -> PathBuf {
        let dir = sessions.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        write(&dir, &json!({
            "schema_version": 1, "session_id": id, "state": state, "created_at": "x", "updated_at": "x",
            "recorder_pid": recorder, "processing_pid": pipeline, "channels": {"mic": null, "system": null},
            "error": null, "note_path": null, "transcript_ready": false,
        }))
        .unwrap();
        std::fs::write(dir.join("mic.wav"), b"audio").unwrap();
        dir
    }

    #[test]
    fn start_refuses_while_the_newest_active_session_has_a_live_owner() {
        let sessions = tempfile::tempdir().unwrap();
        let dir = session_in(sessions.path(), "20261107-143200", "recording", Some(4242), None);

        let e = admit(sessions.path(), &alive()).unwrap_err();

        assert_eq!(e, format!("a session is already active: {}", dir.display()));
        assert_eq!(read(&dir).unwrap()["state"], "recording");
        assert_eq!(std::fs::read_dir(sessions.path()).unwrap().count(), 1, "nothing created");
    }

    #[test]
    fn a_dead_recorder_is_marked_interrupted_with_owner_died_and_a_new_session_starts() {
        let sessions = tempfile::tempdir().unwrap();
        let dirs: Vec<PathBuf> = ["starting", "recording", "stopping"]
            .iter()
            .enumerate()
            .map(|(i, st)| session_in(sessions.path(), &format!("20261107-14320{i}"), st, Some(4242), None))
            .collect();

        admit(sessions.path(), &dead()).unwrap();
        let new = create_session_dir(sessions.path(), "20261107-143300");

        for d in &dirs {
            let m = read(d).unwrap();
            assert_eq!(m["state"], "interrupted", "{}", d.display());
            assert_eq!(m["error"]["code"], "owner_died");
            assert_eq!(std::fs::read(d.join("mic.wav")).unwrap(), b"audio");
        }
        assert!(new.is_ok());
    }

    #[test]
    fn a_pid_reused_by_another_program_is_not_a_live_owner() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("20261107-143200");
        let me = std::env::current_exe().unwrap();
        let notepad = ProcInfo {
            exe: Some(PathBuf::from(r"C:\Windows\System32\notepad.exe")),
            args: vec!["notepad.exe".into(), "miris.recorder".into(), dir.to_string_lossy().into_owned()],
        };
        let _one_at_a_time = crate::test_lock();
        let mut child = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "Start-Sleep 20;#", "miris.recorder"])
            .arg(&dir)
            .spawn()
            .unwrap();

        let live = SysOwners.live(Some(child.id() as u64), "miris.recorder", &dir);

        let _ = child.kill();
        let _ = child.wait();
        assert!(!is_owner(&notepad, &me, "miris.recorder", &dir));
        assert!(!live, "a process that is not this meetrec never owns a session");
    }

    #[test]
    fn a_dead_pipeline_puts_its_session_back_to_captured() {
        let sessions = tempfile::tempdir().unwrap();
        let dir = session_in(sessions.path(), "20261107-143200", "processing", None, Some(4242));

        admit(sessions.path(), &dead()).unwrap();

        let m = read(&dir).unwrap();
        assert_eq!(m["state"], "captured");
        assert_eq!(std::fs::read(dir.join("mic.wav")).unwrap(), b"audio");
    }

    #[test]
    fn a_recorder_that_exits_before_recording_fails_the_session_with_recorder_error() {
        let sessions = tempfile::tempdir().unwrap();
        let dir = session_in(sessions.path(), "20261107-143200", "starting", Some(4242), None);
        std::fs::write(dir.join("recorder.log"), "nenhum microfone disponível\n").unwrap();

        let e = wait_until_recording(&dir, || true, 50, std::time::Duration::from_millis(1)).unwrap_err();

        let m = read(&dir).unwrap();
        assert_eq!(m["state"], "failed");
        assert_eq!(m["error"]["code"], "recorder_error");
        assert!(e.contains("nenhum microfone disponível"), "{e}");
    }

    #[test]
    fn stop_ends_a_recorder_that_never_finishes_and_marks_it_interrupted() {
        let sessions = tempfile::tempdir().unwrap();
        let dir = session_in(sessions.path(), "20261107-143200", "recording", Some(4242), None);
        let owners = alive();
        let mut out = Vec::new();

        let r = stop_with(&dir, &owners, 3, std::time::Duration::from_millis(1), &mut out, |_| panic!("no capture, no pipeline"));

        assert_eq!(r, Ok(()));
        let m = read(&dir).unwrap();
        assert_eq!(m["state"], "interrupted");
        assert_eq!(m["error"]["code"], "recorder_timeout");
        assert!(!owners.live.get(), "stop ended the recorder");
        assert_eq!(std::fs::read(dir.join("mic.wav")).unwrap(), b"audio");
        assert!(String::from_utf8_lossy(&out).contains("recording interrupted:"));
    }

    #[test]
    fn stop_keeps_the_terminal_failure_the_recorder_wrote_after_the_stop_request() {
        let sessions = tempfile::tempdir().unwrap();
        let dir = session_in(sessions.path(), "20261107-143400", "recording", Some(4242), None);
        let flip = dir.clone();
        let recorder = std::thread::spawn(move || {
            while !flip.join("stop").exists() {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            recorder_failed(&flip, "disk_write_error: mic.wav: no space left on device");
        });
        let owners = alive();
        let mut out = Vec::new();
        let started = std::time::Instant::now();

        let r = stop_with(&dir, &owners, 300, std::time::Duration::from_millis(10), &mut out, |_| panic!("no capture, no pipeline"));

        recorder.join().unwrap();
        assert_eq!(r, Ok(()));
        assert!(started.elapsed() < std::time::Duration::from_millis(1500), "stop waited out its timeout");
        let m = read(&dir).unwrap();
        assert_eq!(m["state"], "failed");
        assert_eq!(m["error"]["code"], "disk_write_error");
        assert!(owners.live.get(), "a recorder that already reported its failure is not killed as a timeout");
        assert!(String::from_utf8_lossy(&out).contains("recording interrupted:"));
    }

    /// The pid of a process that has exited: a real child of this test, waited for.
    fn exited_pid() -> u64 {
        let mut child = Command::new("cmd").args(["/C", "exit", "0"]).spawn().unwrap();
        let pid = child.id() as u64;
        child.wait().unwrap();
        pid
    }

    #[test]
    fn a_session_whose_recorder_exited_does_not_block_a_new_start_under_the_system_owner_check() {
        let sessions = tempfile::tempdir().unwrap();
        let dir = session_in(sessions.path(), "20261107-100000", "recording", Some(exited_pid()), None);

        let admitted = admit(sessions.path(), &SysOwners);

        assert_eq!(admitted, Ok(()));
        let m = read(&dir).unwrap();
        assert_eq!(m["state"], "interrupted");
        assert_eq!(m["error"]["code"], "owner_died");
    }

    #[test]
    fn stop_settles_a_session_whose_recorder_exited_at_once_under_the_system_owner_check() {
        let sessions = tempfile::tempdir().unwrap();
        let dir = session_in(sessions.path(), "20261107-100001", "recording", Some(exited_pid()), None);
        let started = std::time::Instant::now();

        let r = stop_with(&dir, &SysOwners, 600, std::time::Duration::from_millis(100), &mut Vec::new(), |_| panic!("no capture, no pipeline"));

        assert_eq!(r, Ok(()));
        assert!(started.elapsed() < std::time::Duration::from_secs(5), "stop waited for a recorder that is gone");
        let m = read(&dir).unwrap();
        assert_eq!((m["state"].as_str(), m["error"]["code"].as_str()), (Some("interrupted"), Some("owner_died")));
    }

    /// A session folder under a temporary MIRIS_HOME whose mic.wav has the zero-size header a killed
    /// recorder leaves: (home, session dir, mic.wav bytes, session.json bytes).
    fn live_owner_fixture(state: &str) -> (tempfile::TempDir, PathBuf, Vec<u8>, Vec<u8>) {
        let home = tempfile::tempdir().unwrap();
        let sessions = home.path().join("sessions");
        let dir = session_in(&sessions, "20261107-110000", state, Some(4242), None);
        let mut wav = std::io::Cursor::new(Vec::new());
        {
            let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
            let mut w = hound::WavWriter::new(&mut wav, spec).unwrap();
            for _ in 0..50 {
                w.write_sample(0.25f32).unwrap();
            }
            w.finalize().unwrap();
        }
        let mut bytes = wav.into_inner();
        let data_at = bytes.windows(4).position(|w| w == b"data").unwrap() + 4;
        bytes[4..8].copy_from_slice(&0u32.to_le_bytes());
        bytes[data_at..data_at + 4].copy_from_slice(&0u32.to_le_bytes());
        std::fs::write(dir.join("mic.wav"), &bytes).unwrap();
        let manifest = std::fs::read(dir.join("session.json")).unwrap();
        (home, dir, bytes, manifest)
    }

    #[test]
    fn recover_and_process_through_the_cli_refuse_a_live_owner_and_leave_every_byte() {
        let _one_at_a_time = crate::test_lock();
        let (home, dir, wav, manifest) = live_owner_fixture("recording");
        std::env::set_var("MIRIS_HOME", home.path());
        let id = ["20261107-110000".to_owned()];

        let recovered = cli_with(&alive(), "recover", &id);
        let processed = cli_with(&alive(), "process", &id);

        assert_eq!((recovered, processed), (1, 1), "both exit 1 with session is active");
        assert_eq!(std::fs::read(dir.join("mic.wav")).unwrap(), wav, "the recording is untouched");
        assert_eq!(std::fs::read(dir.join("session.json")).unwrap(), manifest, "the manifest is untouched");
    }

    #[test]
    fn recover_through_the_cli_repairs_a_session_whose_owner_is_gone() {
        let _one_at_a_time = crate::test_lock();
        let (home, dir, wav, _) = live_owner_fixture("recording");
        std::env::set_var("MIRIS_HOME", home.path());

        let recovered = cli_with(&dead(), "recover", &["20261107-110000".to_owned()]);

        assert_eq!(recovered, 0);
        assert_ne!(std::fs::read(dir.join("mic.wav")).unwrap(), wav, "the header is repaired");
        assert_eq!(hound::WavReader::open(dir.join("mic.wav")).unwrap().len(), 50);
        assert_eq!(read(&dir).unwrap()["state"], "captured");
    }

    /// Every file under `dir` with its bytes, keyed by its path below `dir`.
    fn tree_bytes(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
        fn walk(root: &Path, at: &Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
            for e in std::fs::read_dir(at).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(root, &p, out);
                } else {
                    out.insert(p.strip_prefix(root).unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap());
                }
            }
        }
        let mut out = std::collections::BTreeMap::new();
        walk(dir, dir, &mut out);
        out
    }

    /// Session folders for the current second and the next two, each holding a finished manifest,
    /// a recording and a sentinel: whichever second `start` picks, its folder already exists.
    fn folders_at_the_generated_second(sessions: &Path) -> Vec<PathBuf> {
        (0..3)
            .map(|i| {
                let id = (chrono::Local::now() + chrono::Duration::seconds(i)).format("%Y%m%d-%H%M%S").to_string();
                let dir = session_in(sessions, &id, "completed", None, None);
                std::fs::write(dir.join("sentinel.txt"), format!("must survive {id}")).unwrap();
                dir
            })
            .collect()
    }

    #[test]
    fn start_through_the_real_entry_refuses_a_session_folder_at_the_generated_second_and_keeps_every_byte() {
        let _one_at_a_time = crate::test_lock();
        let home = tempfile::tempdir().unwrap();
        let sessions = home.path().join("sessions");
        let taken = folders_at_the_generated_second(&sessions);
        std::env::set_var("MIRIS_HOME", home.path());
        let before = tree_bytes(&sessions);

        let started = start();

        let e = started.unwrap_err();
        assert!(e.starts_with("session folder "), "start did not stop at the existing folder: {e}");
        assert_eq!(tree_bytes(&sessions), before, "no byte of an existing session was reused or changed");
        assert_eq!(std::fs::read_dir(&sessions).unwrap().count(), taken.len(), "no folder was added: a free second would be a false green");
    }

    /// A loopback stand-in for Anthropic, the Google token endpoint and Drive: every call answers at
    /// once except the one named in `stalls`, which is held for 3 s and dropped.
    fn provider_fake(stalls: &'static str) -> String {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                std::thread::spawn(move || {
                    use std::io::{BufRead, Read, Write};
                    let mut s = s;
                    let mut r = std::io::BufReader::new(s.try_clone().unwrap());
                    let mut first = String::new();
                    let _ = r.read_line(&mut first);
                    let mut length = 0usize;
                    loop {
                        let mut line = String::new();
                        if r.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                            break;
                        }
                        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                            length = v.trim().parse().unwrap_or(0);
                        }
                    }
                    let mut body = vec![0u8; length];
                    let _ = r.read_exact(&mut body);
                    let mut parts = first.split_whitespace();
                    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
                    let (call, reply) = match (method, target) {
                        ("POST", t) if t.starts_with("/v1/messages") => ("anthropic", r#"{"content": []}"#),
                        ("POST", t) if t.starts_with("/token") => ("token", r#"{"access_token": "a"}"#),
                        ("GET", t) if t.contains("appProperties") => ("lookup", r#"{"files": []}"#),
                        ("GET", t) if t.starts_with("/drive/v3/files") => {
                            ("list", if stalls == "mkdir" { r#"{"files": []}"# } else { r#"{"files": [{"id": "F"}]}"# })
                        }
                        ("POST", t) if t.starts_with("/drive/v3/files") => ("mkdir", r#"{"id": "F"}"#),
                        ("POST", t) if t.starts_with("/upload/") => ("upload", r#"{"id": "D", "webViewLink": "x"}"#),
                        _ => ("other", "{}"),
                    };
                    if call == stalls {
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        return;
                    }
                    let _ = write!(s, "HTTP/1.1 200 X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len());
                });
            }
        });
        base
    }

    #[test]
    fn a_real_transport_stall_at_each_provider_ends_at_its_deadline_and_the_pipeline_keeps_every_byte() {
        use crate::minutes::{run_with_drive, summarize, upload_doc, Credentials, Ctx, Deadlines};
        let _one_at_a_time = crate::test_lock();
        let short = std::time::Duration::from_millis(300);
        // (the call that stalls, the name its error carries, whether the minutes were already on disk)
        let branches = [
            ("anthropic", "anthropic", false),
            ("token", "google token", true),
            ("list", "drive list", true),
            ("mkdir", "drive mkdir", true),
            ("upload", "drive upload", true),
        ];
        for (stalls, label, ata_on_disk) in branches {
            let base = provider_fake(stalls);
            let ctx = Ctx {
                anthropic: format!("{base}/v1/messages"),
                oauth_default: format!("{base}/token"),
                drive_files: format!("{base}/drive/v3/files"),
                drive_upload: format!("{base}/upload/drive/v3/files"),
                deadlines: Deadlines { connect: short, anthropic: short, oauth: short, drive_list: short, drive_upload: short },
            };
            let cfg = tempfile::tempdir().unwrap();
            std::fs::write(cfg.path().join("google-oauth-client.json"), json!({"installed": {"client_id": "c", "client_secret": "s", "token_uri": format!("{base}/token")}}).to_string()).unwrap();
            std::fs::write(cfg.path().join("google-token.json"), r#"{"refresh_token": "r"}"#).unwrap();
            let tmp = tempfile::tempdir().unwrap();
            let dir = session_in(tmp.path(), "20261107-143200", "processing", None, Some(4242));
            std::fs::write(dir.join("system.wav"), [9u8, 8, 7, 6, 5]).unwrap();
            std::fs::write(dir.join("transcript.md"), "# Transcricao\n\n**[00:01] Voce:** fechar sexta\n").unwrap();
            let kept = ["mic.wav", "system.wav", "transcript.md"].map(|f| (f, std::fs::read(dir.join(f)).unwrap()));
            let minutes = |_: &str| Ok(json!({"titulo": "Alinhamento", "resumo": "Fechar sexta.", "decisoes": [], "action_items": []}));
            let raw = dir.join("minutes.json");
            let creds = Credentials { anthropic_key: Some("k".into()), google: true };
            let started = std::time::Instant::now();

            let e = if stalls == "anthropic" {
                let ask = |t: &str| summarize(&ctx, "k", "m", t, &raw).map(|(m, _)| m);
                run_with_drive(&dir, &creds, &ask, &|t, md| upload_doc(&ctx, cfg.path(), t, md, "20261107-143200")).unwrap_err()
            } else {
                run_with_drive(&dir, &creds, &minutes, &|t, md| upload_doc(&ctx, cfg.path(), t, md, "20261107-143200")).unwrap_err()
            };

            assert!(started.elapsed() < std::time::Duration::from_millis(2500), "{stalls}: took {:?}", started.elapsed());
            assert!(e.starts_with(label) && e.contains(crate::minutes::DEADLINE_MARK), "{stalls}: {e}");
            let ata_after_minutes = std::fs::read(dir.join("ata.md")).ok();
            assert_eq!(ata_after_minutes.is_some(), ata_on_disk, "{stalls}: ata.md is written before the first Drive call and not before the summary");
            pipeline_failed(&dir, &e);

            let m = read(&dir).unwrap();
            assert_eq!((m["state"].as_str(), m["error"]["code"].as_str()), (Some("captured"), Some("provider_timeout")), "{stalls}");
            assert_eq!(m["error"]["message"].as_str(), Some(e.as_str()), "{stalls}: the real transport error is the one recorded");
            for (f, bytes) in &kept {
                assert_eq!(&std::fs::read(dir.join(f)).unwrap(), bytes, "{stalls}: {f} changed");
            }
            assert_eq!(std::fs::read(dir.join("ata.md")).ok(), ata_after_minutes, "{stalls}: ata.md changed");
            if let Some(ata) = ata_after_minutes {
                assert!(String::from_utf8(ata).unwrap().contains("Fechar sexta."), "{stalls}");
            }
        }
    }

    #[test]
    fn stop_keeps_stopping_when_the_recorder_cannot_be_ended() {
        let sessions = tempfile::tempdir().unwrap();
        let dir = session_in(sessions.path(), "20261107-143200", "recording", Some(4242), None);
        let owners = Fake { live: std::cell::Cell::new(true), can_end: false };

        let e = stop_with(&dir, &owners, 3, std::time::Duration::from_millis(1), &mut Vec::new(), |_| Ok(())).unwrap_err();

        assert!(e.contains("recorder did not stop; owner still live"), "{e}");
        assert_eq!(read(&dir).unwrap()["state"], "stopping");
        assert!(admit(sessions.path(), &owners).is_err(), "start refuses while it lives");
    }

    #[test]
    fn an_owner_is_live_only_with_this_image_and_exactly_the_marker_and_folder() {
        let me = PathBuf::from(r"C:\Users\Sam Doe\AppData\Local\VOXIS\meetrec.exe");
        let dir = PathBuf::from(r"C:\Users\Sam Doe\.miris\sessions\20261107-143200");
        let info = |exe: &str, args: &[&str]| ProcInfo {
            exe: Some(PathBuf::from(exe)),
            args: args.iter().map(|a| a.to_string()).collect(),
        };
        let exe = me.to_str().unwrap();
        let d = dir.to_str().unwrap();

        assert!(is_owner(&info(exe, &[exe, "miris.recorder", d]), &me, "miris.recorder", &dir));
        assert!(is_owner(&info(exe, &[exe, "miris.recorder", r"C:\Users\Sam Doe/.miris/sessions/20261107-143200"]), &me, "miris.recorder", &dir));
        assert!(!is_owner(&info(r"C:\Windows\notepad.exe", &[exe, "miris.recorder", d]), &me, "miris.recorder", &dir));
        assert!(!is_owner(&info(exe, &[exe, "miris.pipeline", d]), &me, "miris.recorder", &dir));
        assert!(!is_owner(&info(exe, &[exe, "miris.recorder", d, "extra"]), &me, "miris.recorder", &dir));
        assert!(!is_owner(&info(exe, &[exe, "miris.recorder", r"C:\Users\Sam Doe\.miris\sessions\20261107-143201"]), &me, "miris.recorder", &dir));
        assert!(!is_owner(&info(exe, &[exe, "miris.recorder C:\\Users\\Sam", "Doe\\.miris"]), &me, "miris.recorder", &dir));
        assert!(!is_owner(&ProcInfo { exe: None, args: vec![exe.into(), "miris.recorder".into(), d.into()] }, &me, "miris.recorder", &dir));
    }

    #[test]
    fn sysinfo_reads_the_argv_elements_of_a_spawned_process() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Sam Doe").join("20261107-143200");
        let _one_at_a_time = crate::test_lock();
        let mut child = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "Start-Sleep 20;#", "miris.recorder"])
            .arg(&dir)
            .spawn()
            .unwrap();

        let info = process_identity(child.id());

        let _ = child.kill();
        let _ = child.wait();
        let info = info.expect("a live process is readable");
        assert!(info.exe.unwrap().to_string_lossy().to_lowercase().ends_with("powershell.exe"));
        assert_eq!(info.args[info.args.len() - 2], "miris.recorder");
        assert_eq!(info.args.last().unwrap(), &dir.to_string_lossy().into_owned(), "a spaced folder is one element");
    }

    #[test]
    fn process_and_recover_refuse_a_session_whose_owner_is_live() {
        let sessions = tempfile::tempdir().unwrap();
        let recording = session_in(sessions.path(), "20261107-143200", "recording", Some(4242), None);
        let processing = session_in(sessions.path(), "20261107-143300", "processing", None, Some(4343));
        let before = std::fs::read(recording.join("session.json")).unwrap();

        let r1 = guard_inactive(&recording, &alive());
        let r2 = guard_inactive(&processing, &alive());

        assert_eq!(r1, Err(format!("session is active: {}", recording.display())));
        assert_eq!(r2, Err(format!("session is active: {}", processing.display())));
        assert_eq!(std::fs::read(recording.join("session.json")).unwrap(), before);
        assert_eq!(guard_inactive(&recording, &dead()), Ok(()));
    }

    #[test]
    fn stop_writes_the_stop_file_only_after_checking_the_recorder() {
        let sessions = tempfile::tempdir().unwrap();
        let gone = session_in(sessions.path(), "20261107-143200", "recording", Some(4242), None);
        let live = session_in(sessions.path(), "20261107-143300", "recording", Some(4343), None);
        let mut out = Vec::new();

        stop_with(&gone, &dead(), 3, std::time::Duration::from_millis(1), &mut out, |_| Ok(())).unwrap();
        stop_with(&live, &alive(), 1, std::time::Duration::from_millis(1), &mut Vec::new(), |_| Ok(())).unwrap();

        assert!(!gone.join("stop").exists(), "no stop file for a recorder that is not ours");
        assert_eq!(read(&gone).unwrap()["error"]["code"], "owner_died");
        assert!(String::from_utf8_lossy(&out).contains(&format!("recording interrupted: {}", gone.display())));
        assert!(live.join("stop").exists());
    }

    // ------------------------------------------------------------ retry reuse and latency marks (AC 81, 88)

    fn set_age(path: &Path, secs_ago: u64) {
        let f = std::fs::File::options().write(true).open(path).unwrap();
        f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(secs_ago)).unwrap();
    }

    #[test]
    fn a_retry_reuses_a_fresh_transcript_and_runs_only_the_minutes() {
        let dir = tempfile::tempdir().unwrap();
        for f in ["mic.wav", "system.wav"] {
            std::fs::write(dir.path().join(f), b"audio").unwrap();
            set_age(&dir.path().join(f), 600);
        }
        std::fs::write(dir.path().join("transcript.md"), "# Transcrição\n\nfechar sexta\n").unwrap();

        assert!(transcript_fresh(dir.path(), true));
        assert!(!transcript_fresh(dir.path(), false), "only when the manifest says it is ready");
    }

    #[test]
    fn a_wav_newer_than_the_transcript_runs_the_full_pipeline() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("transcript.md"), "# Transcrição\n\nfechar sexta\n").unwrap();
        set_age(&dir.path().join("transcript.md"), 600);
        std::fs::write(dir.path().join("mic.wav"), b"audio").unwrap();
        std::fs::write(dir.path().join("system.wav"), b"audio").unwrap();
        set_age(&dir.path().join("system.wav"), 900);
        let empty = tempfile::tempdir().unwrap();
        std::fs::write(empty.path().join("transcript.md"), "").unwrap();

        assert!(!transcript_fresh(dir.path(), true));
        assert!(!transcript_fresh(empty.path(), true), "an empty transcript is not reused");
    }

    #[test]
    fn the_meeting_latency_marks_name_the_stop_and_the_ready_transcript() {
        let sessions = tempfile::tempdir().unwrap();
        let dir = session_in(sessions.path(), "20261107-143200", "recording", Some(4242), None);
        let flip = dir.clone();
        // the recorder side: captured once the stop file shows up
        let recorder = std::thread::spawn(move || {
            while !flip.join("stop").exists() {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            update(&flip, |m| m["state"] = json!("captured")).unwrap();
        });
        let mut out = Vec::new();
        let mut piped = false;

        stop_with(&dir, &alive(), 2000, std::time::Duration::from_millis(1), &mut out, |_| {
            piped = true;
            Ok(())
        })
        .unwrap();
        recorder.join().unwrap();
        let mut marks = Vec::new();
        transcribed(&dir, &mut marks).unwrap();

        let out = String::from_utf8(out).unwrap();
        let marks = String::from_utf8(marks).unwrap();
        assert!(piped);
        assert!(out.lines().any(|l| l.strip_prefix("stopped ").is_some_and(|t| t.parse::<u128>().is_ok())), "{out}");
        assert!(marks.lines().any(|l| l.strip_prefix("transcript_ready ").is_some_and(|t| t.parse::<u128>().is_ok())), "{marks}");
        assert_eq!(read(&dir).unwrap()["transcript_ready"], true);
    }
}

// M7 test-only module (checks C8 and C12 of .specs/features/m7-silent-track-recovery/checks.md).
// Appended to session.rs; it adds tests only, no production line. Fixtures are synthetic bytes,
// created in a new folder under the run's TEMP and never removed by the test.
#[cfg(test)]
mod m7_tests {
    use super::*;
    use sha2::{Digest, Sha256};

    /// The expected class of a fixture, written from the plan's truth table; never computed by product code.
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Oracle {
        Empty,
        Repairable,
    }

    const IEEE_FLOAT_GUID: [u8; 16] = [0x03, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71];
    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn le32(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    fn le16(b: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([b[at], b[at + 1]])
    }

    fn sha256_hex(b: &[u8]) -> String {
        Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
    }

    /// The fixed fields of the product header (plan, "Track classes and outcomes").
    fn assert_product_header(b: &[u8], what: &str) {
        assert!(b.len() >= 68, "fixture: {what} is {} bytes, below the 68-byte product header", b.len());
        assert_eq!(&b[0..4], b"RIFF", "fixture: {what} bytes 0..4");
        assert_eq!(&b[8..12], b"WAVE", "fixture: {what} bytes 8..12");
        assert_eq!(&b[12..16], b"fmt ", "fixture: {what} bytes 12..16");
        assert_eq!(le32(b, 16), 40, "fixture: {what} bytes 16..20");
        assert_eq!(le16(b, 20), 0xFFFE, "fixture: {what} bytes 20..22");
        assert_eq!(le16(b, 22), 1, "fixture: {what} bytes 22..24");
        assert_eq!(le16(b, 32), 4, "fixture: {what} bytes 32..34");
        assert_eq!(le16(b, 34), 32, "fixture: {what} bytes 34..36");
        assert_eq!(le16(b, 36), 22, "fixture: {what} bytes 36..38");
        assert_eq!(le16(b, 38), 32, "fixture: {what} bytes 38..40");
        assert_eq!(&b[44..60], &IEEE_FLOAT_GUID, "fixture: {what} bytes 44..60");
        assert_eq!(&b[60..64], b"data", "fixture: {what} bytes 60..64");
    }

    /// K(n): n samples as the recorder writes them (hound 3.5.1, 32-bit float mono), then R = 0 and D = 0.
    fn k(n: usize) -> (Vec<u8>, Oracle) {
        let spec = hound::WavSpec { channels: 1, sample_rate: 48_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut c = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut c, spec).unwrap();
            for i in 0..n {
                w.write_sample(((i % 97) as f32 / 97.0) - 0.5).unwrap();
            }
            w.finalize().unwrap();
        }
        let mut b = c.into_inner();
        assert_product_header(&b, "V(n) before the kill");
        assert_eq!(le32(&b, 64) as usize, 4 * n, "fixture: V({n}) D before the kill");
        assert_eq!(le32(&b, 4) as usize, 60 + 4 * n, "fixture: V({n}) R before the kill");
        b[4..8].copy_from_slice(&0u32.to_le_bytes());
        b[64..68].copy_from_slice(&0u32.to_le_bytes());
        (b, Oracle::Repairable)
    }

    /// Fixture assertion for K(n): L, D, R, W and the oracle class of the truth table.
    fn assert_k(b: &[u8], n: usize, class: Oracle) {
        assert_product_header(b, "K(n)");
        let l = b.len();
        let w = (l - 68) - (l - 68) % 4;
        assert_eq!(l, 68 + 4 * n, "fixture: K({n}) length L");
        assert_eq!(le32(b, 64), 0, "fixture: K({n}) D (bytes 64..68)");
        assert_eq!(le32(b, 4), 0, "fixture: K({n}) R (bytes 4..8)");
        assert_eq!(w, 4 * n, "fixture: K({n}) W");
        // truth table: product header, L >= 72, W >= 4, D = 0 -> REPAIRABLE
        assert!(l >= 72 && w >= 4, "fixture: K({n}) is too short for REPAIRABLE");
        assert_eq!(class, Oracle::Repairable, "fixture: K({n}) oracle class");
    }

    /// E: the empty file.
    fn e() -> (Vec<u8>, Oracle) {
        (Vec::new(), Oracle::Empty)
    }

    /// A new folder under the run's TEMP; created new, never removed by the test.
    fn run_folder(tag: &str) -> PathBuf {
        let root = std::env::temp_dir();
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = root.join(format!("{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        assert!(dir.starts_with(&root), "fixture: {} is outside {}", dir.display(), root.display());
        dir
    }

    /// A schema 1 manifest in `<home>/sessions/<id>`, left `interrupted` by a dead recorder.
    fn session_m7(home: &Path, id: &str, state: &str) -> PathBuf {
        let dir = home.join("sessions").join(id);
        std::fs::create_dir_all(&dir).unwrap();
        write(&dir, &json!({
            "schema_version": 1, "session_id": id, "state": state, "created_at": "x", "updated_at": "x",
            "recorder_pid": 4242, "processing_pid": null, "channels": {"mic": null, "system": null},
            "error": {"code": "owner_died", "message": "x"}, "note_path": null, "transcript_ready": false,
        }))
        .unwrap();
        dir
    }

    /// No owner of any session is alive.
    struct Gone;

    impl Owners for Gone {
        fn live(&self, _: Option<u64>, _: &str, _: &Path) -> bool {
            false
        }
        fn end(&self, _: u64) -> bool {
            false
        }
    }

    fn changed_offsets(before: &[u8], after: &[u8]) -> Vec<usize> {
        (0..before.len().min(after.len())).filter(|&i| before[i] != after[i]).collect()
    }

    /// C8 (AC 1): mic K(1000) and system E on an interrupted session with a dead owner.
    #[test]
    fn m7_1_recover_skips_an_empty_system_track_and_repairs_mic() {
        let _one_at_a_time = crate::test_lock();
        let home = run_folder("m7_1");
        let id = "20261109-120001";
        let dir = session_m7(&home, id, "interrupted");
        let (mic, mic_class) = k(1000);
        assert_k(&mic, 1000, mic_class);
        let (sys, sys_class) = e();
        assert_eq!((sys.len(), sys_class), (0, Oracle::Empty), "fixture: E length and oracle class");
        std::fs::write(dir.join("mic.wav"), &mic).unwrap();
        std::fs::write(dir.join("system.wav"), &sys).unwrap();
        std::env::set_var("MIRIS_HOME", &home);

        let exit = cli_with(&Gone, "recover", &[id.to_owned()]);

        let m = read(&dir).unwrap();
        let mic_after = std::fs::read(dir.join("mic.wav")).unwrap();
        let sys_after = std::fs::read(dir.join("system.wav")).unwrap();
        let changed = changed_offsets(&mic, &mic_after);
        let samples = hound::WavReader::open(dir.join("mic.wav")).map(|r| r.len());
        eprintln!(
            "m7_1 observed: exit={exit} state={} error={} mic_len={}->{} mic_changed_offsets={changed:?} mic_R={} mic_D={} mic_samples={samples:?} system_len={} system_sha256={}",
            m["state"],
            m["error"],
            mic.len(),
            mic_after.len(),
            le32(&mic_after, 4),
            le32(&mic_after, 64),
            sys_after.len(),
            sha256_hex(&sys_after)
        );

        assert_eq!(exit, 0, "behavior: recover of mic REPAIRABLE + system EMPTY must exit 0");
        assert_eq!(m["state"], "captured", "behavior: state after recover");
        assert!(m["error"].is_null(), "behavior: error must be null, got {}", m["error"]);
        assert_eq!(mic_after.len(), mic.len(), "behavior: mic.wav length unchanged");
        assert!(changed.iter().all(|&i| (4..8).contains(&i) || (64..68).contains(&i)), "behavior: only bytes 4..8 and 64..68 of mic.wav may change: {changed:?}");
        assert_eq!(&mic_after[68..], &mic[68..], "behavior: mic.wav sample bytes identical");
        assert_eq!(samples.as_ref().ok(), Some(&1000), "behavior: mic.wav reads 1000 samples");
        assert_eq!(sys_after.len(), 0, "behavior: system.wav stays 0 bytes");
        assert_eq!(sha256_hex(&sys_after), EMPTY_SHA256, "behavior: system.wav SHA-256");
    }

    /// C12 (AC 4), the control of C8: mic K(1000) and system K(500), both repaired.
    #[test]
    fn m7_4_two_repairable_tracks_are_both_repaired() {
        let _one_at_a_time = crate::test_lock();
        let home = run_folder("m7_4");
        let id = "20261109-120004";
        let dir = session_m7(&home, id, "interrupted");
        let (mic, mic_class) = k(1000);
        assert_k(&mic, 1000, mic_class);
        let (sys, sys_class) = k(500);
        assert_k(&sys, 500, sys_class);
        std::fs::write(dir.join("mic.wav"), &mic).unwrap();
        std::fs::write(dir.join("system.wav"), &sys).unwrap();
        std::env::set_var("MIRIS_HOME", &home);

        let exit = cli_with(&Gone, "recover", &[id.to_owned()]);

        let m = read(&dir).unwrap();
        eprintln!("m7_4 observed: exit={exit} state={} error={}", m["state"], m["error"]);
        assert_eq!(exit, 0, "behavior: recover of two REPAIRABLE tracks exits 0");
        assert_eq!(m["state"], "captured", "behavior: state after recover");
        for (name, before, n) in [("mic.wav", &mic, 1000u32), ("system.wav", &sys, 500u32)] {
            let after = std::fs::read(dir.join(name)).unwrap();
            let changed = changed_offsets(before, &after);
            assert_eq!(after.len(), before.len(), "behavior: {name} length unchanged");
            assert!(changed.iter().all(|&i| (4..8).contains(&i) || (64..68).contains(&i)), "behavior: {name} changed outside the size fields: {changed:?}");
            assert_eq!(&after[68..], &before[68..], "behavior: {name} sample bytes identical");
            assert_eq!(le32(&after, 64), 4 * n, "behavior: {name} D' = W");
            assert_eq!(le32(&after, 4), 60 + 4 * n, "behavior: {name} R' = 60 + W");
            assert_eq!(hound::WavReader::open(dir.join(name)).unwrap().len(), n, "behavior: {name} sample count");
        }
    }

    // ---------------------------------------------------------------- CP3 S1 slices (C8 onward)

    /// An interrupted session under a new MIRIS_HOME holding the given tracks; the caller holds `test_lock`.
    fn interrupted_with(tag: &str, id: &str, tracks: &[(&str, &[u8])]) -> PathBuf {
        let home = run_folder(tag);
        let dir = session_m7(&home, id, "interrupted");
        for (name, bytes) in tracks {
            std::fs::write(dir.join(name), bytes).unwrap();
        }
        std::env::set_var("MIRIS_HOME", &home);
        dir
    }

    /// `recover <id>` through `cli_to`: (exit, stdout, stderr).
    fn recover_to(id: &str) -> (i32, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let exit = cli_to(&Gone, "recover", &[id.to_owned()], &mut out, &mut err);
        (exit, String::from_utf8_lossy(&out).into_owned(), String::from_utf8_lossy(&err).into_owned())
    }

    /// C8 (AC 1), printed half: the skip line for the empty system track.
    #[test]
    fn m7_1_recover_prints_that_the_empty_system_track_is_skipped() {
        let _one_at_a_time = crate::test_lock();
        let (mic, mic_class) = k(1000);
        assert_k(&mic, 1000, mic_class);
        let (sys, sys_class) = e();
        assert_eq!((sys.len(), sys_class), (0, Oracle::Empty), "fixture: E length and oracle class");
        let _dir = interrupted_with("m7_1p", "20261109-120011", &[("mic.wav", &mic), ("system.wav", &sys)]);

        let (exit, out, err) = recover_to("20261109-120011");

        eprintln!("m7_1p observed: exit={exit} stdout={out:?} stderr={err:?}");
        assert!(out.lines().any(|l| l == "system.wav: empty (0 bytes), no audio captured, skipped"), "behavior: stdout must hold the empty-track skip line, got {out:?}");
        assert_eq!(exit, 0, "behavior: exit");
    }

    /// Hk: the 68-byte product header a killed writer leaves (R = 0, D = 0); checked against the fixed fields.
    fn hk() -> Vec<u8> {
        let (mut b, _) = k(1);
        b.truncate(68);
        assert_product_header(&b, "Hk");
        assert_eq!((b.len(), le32(&b, 4), le32(&b, 64)), (68, 0, 0), "fixture: Hk L, R, D");
        b
    }

    /// F(n): the first n bytes (1 to 67) of a product header; oracle FRAGMENT.
    fn f(n: usize) -> Vec<u8> {
        let h = hk();
        let b = h[..n].to_vec();
        assert_eq!((b.len(), &b[..], "FRAGMENT"), (n, &h[..n], "FRAGMENT"), "fixture: F({n}) length and covered prefix");
        b
    }

    /// Z(n): n bytes of ASCII `x`; byte 0 disagrees with `R` of `RIFF`, oracle UNREADABLE.
    fn z(n: usize) -> Vec<u8> {
        let b = vec![b'x'; n];
        assert!(b.len() == n && b[0] != b'R', "fixture: Z({n}) is {n} bytes that disagree with the fixed fields");
        b
    }

    /// C9 (AC 2): a partial product header is skipped, kept and named.
    #[test]
    fn m7_2_a_partial_header_is_skipped_and_kept() {
        let _one_at_a_time = crate::test_lock();
        let (mic, mic_class) = k(1000);
        assert_k(&mic, 1000, mic_class);
        for (i, n) in [1usize, 43, 44, 63, 67].into_iter().enumerate() {
            let sys = f(n);
            let id = format!("20261109-1202{i:02}");
            let dir = interrupted_with("m7_2f", &id, &[("mic.wav", &mic), ("system.wav", &sys)]);

            let (exit, out, err) = recover_to(&id);

            eprintln!("m7_2f F({n}) observed: exit={exit} stdout={out:?} stderr={err:?}");
            let line = format!("system.wav: partial header ({n} bytes), no audio captured, skipped");
            assert!(out.lines().any(|l| l == line), "behavior: F({n}) stdout must hold {line:?}, got {out:?}");
            assert_eq!(exit, 0, "behavior: F({n}) exit");
            assert_eq!(std::fs::read(dir.join("system.wav")).unwrap(), sys, "behavior: F({n}) system.wav byte-identical");
        }
    }

    /// C10 (AC 2, AC 9): a short file that disagrees with the fixed fields is UNREADABLE and refuses the recovery.
    #[test]
    fn m7_2_a_short_non_header_file_is_unreadable() {
        let _one_at_a_time = crate::test_lock();
        let (mic, mic_class) = k(1000);
        assert_k(&mic, 1000, mic_class);
        for (i, n) in [1usize, 30, 67].into_iter().enumerate() {
            let sys = z(n);
            let id = format!("20261109-1203{i:02}");
            let dir = interrupted_with("m7_2z", &id, &[("mic.wav", &mic), ("system.wav", &sys)]);
            let manifest = std::fs::read(dir.join("session.json")).unwrap();

            let (exit, out, err) = recover_to(&id);

            eprintln!("m7_2z Z({n}) observed: exit={exit} stdout={out:?} stderr={err:?}");
            assert!(err.lines().any(|l| l.starts_with("system.wav: unreadable (") && l.ends_with(')')), "behavior: Z({n}) stderr must name system.wav as unreadable (<reason>), got {err:?}");
            assert_eq!(exit, 1, "behavior: Z({n}) exit");
            assert_eq!(std::fs::read(dir.join("system.wav")).unwrap(), sys, "behavior: Z({n}) system.wav byte-identical");
            assert_eq!(std::fs::read(dir.join("mic.wav")).unwrap(), mic, "behavior: Z({n}) mic.wav byte-identical");
            assert_eq!(std::fs::read(dir.join("session.json")).unwrap(), manifest, "behavior: Z({n}) session.json byte-identical");
            let records: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("recover.journal")).collect();
            assert!(records.is_empty(), "behavior: Z({n}) no recover.journal* file");
        }
    }

    /// C11 (AC 3): a header-only track (68 to 71 bytes) is skipped and kept.
    #[test]
    fn m7_3_a_header_only_track_is_skipped_and_kept() {
        let _one_at_a_time = crate::test_lock();
        let (mic, mic_class) = k(1000);
        assert_k(&mic, 1000, mic_class);
        let mut hf = hk();
        hf[4..8].copy_from_slice(&60u32.to_le_bytes());
        let mut hd = hk();
        hd[64..68].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut h70 = hk();
        h70.extend_from_slice(&[0, 0]);
        let cases = [("Hk", hk()), ("Hf", hf), ("Hd(4294967295)", hd), ("70 bytes", h70)];
        for (i, (name, sys)) in cases.into_iter().enumerate() {
            assert_product_header(&sys, name);
            assert!((68..=71).contains(&sys.len()), "fixture: {name} is HEADER-ONLY by length");
            let id = format!("20261109-1204{i:02}");
            let dir = interrupted_with("m7_3", &id, &[("mic.wav", &mic), ("system.wav", &sys)]);

            let (exit, out, err) = recover_to(&id);

            eprintln!("m7_3 {name} observed: exit={exit} stdout={out:?} stderr={err:?}");
            assert!(out.lines().any(|l| l == "system.wav: no samples, skipped"), "behavior: {name} stdout must hold the no-samples line, got {out:?}");
            assert_eq!(exit, 0, "behavior: {name} exit");
            assert_eq!(std::fs::read(dir.join("system.wav")).unwrap(), sys, "behavior: {name} system.wav byte-identical");
        }
    }

    /// A finalized product WAV of n samples from `sample(i)`; checked against the fixed fields, D = 4n, R = 60 + 4n.
    fn v_with(n: usize, sample: impl Fn(usize) -> f32) -> Vec<u8> {
        let spec = hound::WavSpec { channels: 1, sample_rate: 48_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut c = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut c, spec).unwrap();
            for i in 0..n {
                w.write_sample(sample(i)).unwrap();
            }
            w.finalize().unwrap();
        }
        let b = c.into_inner();
        assert_product_header(&b, "V(n)");
        assert_eq!((b.len(), le32(&b, 64) as usize, le32(&b, 4) as usize), (68 + 4 * n, 4 * n, 60 + 4 * n), "fixture: V({n}) L, D, R");
        b
    }

    fn v(n: usize) -> Vec<u8> {
        v_with(n, |i| ((i % 89) as f32 / 89.0) - 0.5)
    }

    /// The sample count hound reads, reading every sample.
    fn samples_of(path: &Path) -> Result<u32, String> {
        hound::WavReader::open(path).map_err(|e| e.to_string()).and_then(|r| {
            let n = r.len();
            r.into_samples::<f32>().collect::<Result<Vec<_>, _>>().map(|_| n).map_err(|e| e.to_string())
        })
    }

    /// C13 (AC 5): a VALID track (all-zero, audible, or with a stale RIFF size) is kept and not called empty.
    #[test]
    fn m7_5_a_valid_track_silent_audible_or_stale_riff_is_kept() {
        let _one_at_a_time = crate::test_lock();
        let (mic, mic_class) = k(1000);
        assert_k(&mic, 1000, mic_class);
        let mut vr = v(400);
        vr[4..8].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!((le32(&vr, 4), le32(&vr, 64), vr.len()), (0, 1600, 1668), "fixture: Vr(400) R, D, L (VALID: D = W)");
        let cases = [("V0(48000)", v_with(48_000, |_| 0.0)), ("V(1000)", v(1000)), ("Vr(400)", vr)];
        for (i, (name, sys)) in cases.into_iter().enumerate() {
            let id = format!("20261109-1205{i:02}");
            let dir = interrupted_with("m7_5", &id, &[("mic.wav", &mic), ("system.wav", &sys)]);

            let (exit, out, err) = recover_to(&id);

            eprintln!("m7_5 {name} observed: exit={exit} stdout={out:?} stderr={err:?}");
            assert_eq!(exit, 0, "behavior: {name} exit");
            assert_eq!(std::fs::read(dir.join("system.wav")).unwrap(), sys, "behavior: {name} system.wav byte-identical");
            assert!(!out.contains("system.wav: empty"), "behavior: {name} is not reported empty: {out:?}");
            assert_eq!(read(&dir).unwrap()["state"], "captured", "behavior: {name} state");
        }
    }

    /// C14 (AC 6): a track declaring more samples than present gets D = W and R = 60 + W, samples untouched.
    #[test]
    fn m7_6_a_truncated_track_declares_exactly_the_samples_present() {
        let _one_at_a_time = crate::test_lock();
        let (mic, mic_class) = k(1000);
        assert_k(&mic, 1000, mic_class);
        let mut t = v(400);
        t[64..68].copy_from_slice(&4000u32.to_le_bytes());
        t[4..8].copy_from_slice(&4060u32.to_le_bytes());
        assert_eq!((le32(&t, 64), le32(&t, 4), t.len(), t.len() - 68), (4000, 4060, 1668, 1600), "fixture: T(1000, 400) D, R, L, W (REPAIRABLE: D > W)");
        for (i, (name, sys)) in [("T(1000, 400)", t), ("T(400, 400)", v(400))].into_iter().enumerate() {
            let id = format!("20261109-1206{i:02}");
            let dir = interrupted_with("m7_6", &id, &[("mic.wav", &mic), ("system.wav", &sys)]);

            let (exit, out, err) = recover_to(&id);

            let after = std::fs::read(dir.join("system.wav")).unwrap();
            let n = samples_of(&dir.join("system.wav"));
            eprintln!("m7_6 {name} observed: exit={exit} D={} R={} samples={n:?} stdout={out:?} stderr={err:?}", le32(&after, 64), le32(&after, 4));
            assert_eq!(exit, 0, "behavior: {name} exit");
            assert_eq!((le32(&after, 64), le32(&after, 4)), (1600, 1660), "behavior: {name} D and R after recover");
            assert_eq!(&after[68..], &sys[68..], "behavior: {name} sample bytes identical");
            assert_eq!(n, Ok(400), "behavior: {name} reads 400 samples");
        }
    }

    /// C15 (AC 6): a header declaring 1000 samples with none present is HEADER-ONLY and kept.
    #[test]
    fn m7_6_a_declaration_with_no_samples_is_header_only() {
        let _one_at_a_time = crate::test_lock();
        let (mic, mic_class) = k(1000);
        assert_k(&mic, 1000, mic_class);
        let mut hd = hk();
        hd[64..68].copy_from_slice(&4000u32.to_le_bytes());
        assert_eq!((hd.len(), le32(&hd, 64)), (68, 4000), "fixture: Hd(4000) L, D (HEADER-ONLY)");
        let dir = interrupted_with("m7_6h", "20261109-120610", &[("mic.wav", &mic), ("system.wav", &hd)]);

        let (exit, out, err) = recover_to("20261109-120610");

        eprintln!("m7_6h observed: exit={exit} stdout={out:?} stderr={err:?}");
        assert_eq!(std::fs::read(dir.join("system.wav")).unwrap(), hd, "behavior: Hd(4000) byte-identical");
        assert!(out.lines().any(|l| l == "system.wav: no samples, skipped"), "behavior: Hd(4000) no-samples line, got {out:?}");
        assert_eq!(exit, 0, "behavior: exit");
    }

    /// C16 (AC 7): trailing bytes after the last whole sample stay; a second recover writes nothing.
    #[test]
    fn m7_7_trailing_bytes_stay_and_a_second_recover_writes_nothing() {
        let _one_at_a_time = crate::test_lock();
        let (mic, mic_class) = k(1000);
        assert_k(&mic, 1000, mic_class);
        let (mut kt, kt_class) = k(250);
        assert_k(&kt, 250, kt_class);
        kt.extend_from_slice(&[0x11, 0x22]);
        assert_eq!((kt.len(), le32(&kt, 64)), (1070, 0), "fixture: Kt(250, 2) L, D (REPAIRABLE)");
        let dir = interrupted_with("m7_7", "20261109-120700", &[("mic.wav", &mic), ("system.wav", &kt)]);

        let (exit, out, err) = recover_to("20261109-120700");

        let after = std::fs::read(dir.join("system.wav")).unwrap();
        eprintln!("m7_7 Kt observed: exit={exit} len={} D={} R={} stdout={out:?} stderr={err:?}", after.len(), le32(&after, 64), le32(&after, 4));
        assert_eq!(exit, 0, "behavior: Kt(250, 2) exit");
        assert_eq!((after.len(), le32(&after, 64), le32(&after, 4)), (1070, 1000, 1060), "behavior: Kt(250, 2) L, D, R after recover");
        assert_eq!(&after[68..], &kt[68..], "behavior: Kt(250, 2) samples and trailing bytes unchanged");
        assert_eq!(samples_of(&dir.join("system.wav")), Ok(250), "behavior: Kt(250, 2) reads 250 samples");
        let once = std::fs::read(dir.join("system.wav")).unwrap();
        let (again, _, _) = recover_to("20261109-120700");
        assert_eq!((again, std::fs::read(dir.join("system.wav")).unwrap()), (0, once), "behavior: a second recover writes nothing");

        let mut vt = v(250);
        vt.extend_from_slice(&[0x33, 0x44]);
        let dir = interrupted_with("m7_7v", "20261109-120701", &[("mic.wav", &mic), ("system.wav", &vt)]);
        assert_eq!(recover_to("20261109-120701").0, 0, "control: Vt(250, 2) exit");
        assert_eq!(std::fs::read(dir.join("system.wav")).unwrap(), vt, "control: Vt(250, 2) byte-identical");
    }

    /// X: 100 bytes, `x` x 40, `data`, `x` x 56; not a product header, oracle UNREADABLE.
    fn x100() -> Vec<u8> {
        let mut b = vec![b'x'; 40];
        b.extend_from_slice(b"data");
        b.extend_from_slice(&[b'x'; 56]);
        assert!(b.len() == 100 && &b[40..44] == b"data" && b[0] != b'R', "fixture: X is 100 bytes with data at 40 and no RIFF");
        b
    }

    /// Y: 100 bytes of `x` with no `data`; oracle UNREADABLE.
    fn y100() -> Vec<u8> {
        let b = vec![b'x'; 100];
        assert!(b.len() == 100 && !b.windows(4).any(|w| w == b"data"), "fixture: Y is 100 bytes with no data tag");
        b
    }

    fn journal_files(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("recover.journal")).collect()
    }

    /// C17 (AC 8): a non-header file with the bytes `data` in it is never written.
    #[test]
    fn m7_8_a_non_header_file_with_data_bytes_is_never_written() {
        let _one_at_a_time = crate::test_lock();
        let (mic, mic_class) = k(1000);
        assert_k(&mic, 1000, mic_class);
        let x = x100();
        let dir = interrupted_with("m7_8x", "20261109-120800", &[("mic.wav", &mic), ("system.wav", &x)]);

        let (exit, out, err) = recover_to("20261109-120800");

        eprintln!("m7_8x observed: exit={exit} stdout={out:?} stderr={err:?}");
        assert_eq!(std::fs::read(dir.join("system.wav")).unwrap(), x, "behavior: X byte-identical");
        assert_eq!(std::fs::read(dir.join("mic.wav")).unwrap(), mic, "behavior: mic.wav byte-identical when the recovery is refused");
        assert!(err.lines().any(|l| l.starts_with("system.wav: unreadable (") && l.ends_with(')')), "behavior: stderr names system.wav as unreadable, got {err:?}");
        assert_eq!(exit, 1, "behavior: exit");
    }

    /// C18 (AC 8): only a REPAIRABLE track is written, and only its bytes 4..8 and 64..68.
    #[test]
    fn m7_8_only_a_repairable_track_is_written_and_only_eight_bytes() {
        let _one_at_a_time = crate::test_lock();
        let (mic, mic_class) = k(1000);
        assert_k(&mic, 1000, mic_class);
        let mut hf = hk();
        hf[4..8].copy_from_slice(&60u32.to_le_bytes());
        let cases = [("E", Vec::new()), ("F(1)", f(1)), ("F(64)", f(64)), ("Hf", hf), ("Hk", hk()), ("V0(1000)", v_with(1000, |_| 0.0))];
        for (i, (name, sys)) in cases.into_iter().enumerate() {
            let id = format!("20261109-1208{:02}", 10 + i);
            let dir = interrupted_with("m7_8", &id, &[("mic.wav", &mic), ("system.wav", &sys)]);

            let (exit, out, err) = recover_to(&id);

            let mic_after = std::fs::read(dir.join("mic.wav")).unwrap();
            let changed = changed_offsets(&mic, &mic_after);
            eprintln!("m7_8 {name} observed: exit={exit} mic_changed={changed:?} stdout={out:?} stderr={err:?}");
            assert_eq!(std::fs::read(dir.join("system.wav")).unwrap(), sys, "behavior: {name} system.wav byte-identical");
            assert!(!changed.is_empty() && changed.iter().all(|&o| (4..8).contains(&o) || (64..68).contains(&o)), "behavior: REPAIRABLE mic.wav changes only bytes 4..8 and 64..68: {changed:?}");
            assert_eq!(exit, 0, "behavior: {name} exit");
        }
    }

    /// C19 (AC 9): an UNREADABLE track refuses the whole recovery before any byte is written.
    #[test]
    fn m7_9_an_unreadable_track_refuses_the_whole_recovery() {
        let _one_at_a_time = crate::test_lock();
        let (mic, mic_class) = k(1000);
        assert_k(&mic, 1000, mic_class);
        for (i, (name, sys)) in [("Y", y100()), ("Z(30)", z(30))].into_iter().enumerate() {
            let id = format!("20261109-1209{i:02}");
            let dir = interrupted_with("m7_9", &id, &[("mic.wav", &mic), ("system.wav", &sys)]);
            let manifest = std::fs::read(dir.join("session.json")).unwrap();

            let (exit, out, err) = recover_to(&id);

            eprintln!("m7_9 {name} observed: exit={exit} stdout={out:?} stderr={err:?}");
            assert_eq!(std::fs::read(dir.join("mic.wav")).unwrap(), mic, "behavior: {name} mic.wav byte-identical");
            assert_eq!(std::fs::read(dir.join("system.wav")).unwrap(), sys, "behavior: {name} system.wav byte-identical");
            assert_eq!(std::fs::read(dir.join("session.json")).unwrap(), manifest, "behavior: {name} session.json byte-identical");
            assert!(journal_files(&dir).is_empty(), "behavior: {name} no recover.journal* file");
            assert!(err.lines().any(|l| l.starts_with("system.wav: unreadable (") && l.ends_with(')')), "behavior: {name} stderr names system.wav as unreadable, got {err:?}");
            assert_eq!(exit, 1, "behavior: {name} exit");
        }
        let v0 = v_with(1000, |_| 0.0);
        let y = y100();
        let dir = interrupted_with("m7_9c", "20261109-120910", &[("mic.wav", &v0), ("system.wav", &y)]);
        let _ = recover_to("20261109-120910");
        assert_eq!((std::fs::read(dir.join("mic.wav")).unwrap(), std::fs::read(dir.join("system.wav")).unwrap()), (v0, y), "control: V0(1000) + Y bytes identical");
    }

    /// A virtual track of `len` bytes: `head` then zeros; every write is recorded, none is applied.
    struct Virtual {
        len: u64,
        head: Vec<u8>,
        pos: u64,
        writes: Vec<(u64, Vec<u8>)>,
    }

    impl std::io::Read for Virtual {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let left = self.len.saturating_sub(self.pos);
            let n = (buf.len() as u64).min(left) as usize;
            for (i, b) in buf[..n].iter_mut().enumerate() {
                let at = self.pos as usize + i;
                *b = if (self.pos + i as u64) < self.head.len() as u64 { self.head[at] } else { 0 };
            }
            self.pos += n as u64;
            Ok(n)
        }
    }

    impl std::io::Write for Virtual {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.writes.push((self.pos, buf.to_vec()));
            self.pos += buf.len() as u64;
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl std::io::Seek for Virtual {
        fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
            self.pos = match to {
                std::io::SeekFrom::Start(p) => p,
                std::io::SeekFrom::End(d) => (self.len as i64 + d) as u64,
                std::io::SeekFrom::Current(d) => (self.pos as i64 + d) as u64,
            };
            Ok(self.pos)
        }
    }

    /// C21 (AC 10): `repair_wav_in` refuses a length whose RIFF size does not fit a u32 and writes nothing.
    #[test]
    fn m7_10_repair_refuses_a_length_above_u32_and_writes_nothing() {
        let head = hk();
        let mut over = Virtual { len: 4_294_967_304, head: head.clone(), pos: 0, writes: Vec::new() };
        assert_eq!((over.len - 8, u64::from(u32::MAX)), (4_294_967_296, 4_294_967_295), "fixture: L - 8 is one above the largest u32");

        let r = repair_wav_in(&mut over);

        eprintln!("m7_10 L=4294967304 observed: result={r:?} writes={:?}", over.writes);
        assert!(over.writes.is_empty(), "behavior: no write is issued for L = 4294967304, got {:?}", over.writes);
        assert!(r.as_ref().is_err_and(|e| e.contains("larger than a WAV header can describe")), "behavior: the error names the limit, got {r:?}");

        let mut fits = Virtual { len: 4_294_967_303, head, pos: 0, writes: Vec::new() };
        let r = repair_wav_in(&mut fits);
        eprintln!("m7_10 L=4294967303 observed: result={r:?} writes={:?}", fits.writes);
        assert_eq!(r, Ok(()), "control: L = 4294967303 follows the ordinary rules");
        assert!(fits.writes.contains(&(4, u32::MAX.to_le_bytes().to_vec())), "control: R = L - 8 = 4294967295 written at 4");
    }

    /// C24 (AC 12): a repair prints the old and new size fields in decimal.
    #[test]
    fn m7_12_recover_prints_the_old_and_new_size_fields() {
        let _one_at_a_time = crate::test_lock();
        let (mic, mic_class) = k(1000);
        assert_k(&mic, 1000, mic_class);
        let _dir = interrupted_with("m7_12", "20261109-121200", &[("mic.wav", &mic)]);

        let (exit, out, err) = recover_to("20261109-121200");

        eprintln!("m7_12 observed: exit={exit} stdout={out:?} stderr={err:?}");
        assert!(out.lines().any(|l| l == "mic.wav: riff_size 0 -> 4060, data_size 0 -> 4000"), "behavior: stdout must hold the size line, got {out:?}");
        assert_eq!(exit, 0, "behavior: exit");
    }

    /// C25 (AC 13): with no track holding audio, recover ends `failed` with `no_audio` and keeps every file.
    #[test]
    fn m7_13_no_track_with_audio_ends_failed_no_audio() {
        let _one_at_a_time = crate::test_lock();
        let cases: [(&str, Vec<u8>, Vec<u8>, &str); 3] = [
            ("E + E", Vec::new(), Vec::new(), "failed"),
            ("Hk + E", hk(), Vec::new(), "failed"),
            ("K(1) + E", k(1).0, Vec::new(), "captured"),
        ];
        for (i, (name, mic, sys, want)) in cases.into_iter().enumerate() {
            let id = format!("20261109-1213{i:02}");
            let dir = interrupted_with("m7_13", &id, &[("mic.wav", &mic), ("system.wav", &sys)]);

            let (exit, out, err) = recover_to(&id);

            let m = read(&dir).unwrap();
            eprintln!("m7_13 {name} observed: exit={exit} state={} error={} stdout={out:?} stderr={err:?}", m["state"], m["error"]);
            assert_eq!(exit, 0, "behavior: {name} exit");
            assert_eq!(m["state"], want, "behavior: {name} state");
            if want == "failed" {
                assert_eq!(m["error"]["code"], "no_audio", "behavior: {name} error.code");
                assert_eq!(m["error"]["message"], "no track holds audio; the files were kept", "behavior: {name} error.message");
                assert_eq!(std::fs::read(dir.join("mic.wav")).unwrap(), mic, "behavior: {name} mic.wav byte-identical");
                assert_eq!(std::fs::read(dir.join("system.wav")).unwrap(), sys, "behavior: {name} system.wav byte-identical");
            }
        }
    }
}

// M7 CP4 test-only module for session.rs (check C38 and the byte half of C41 of .specs/features/m7-silent-track-recovery/checks.md).
// Self-contained so it compiles unchanged at c2d7b6c and at HEAD; tests only, no production line. Fixtures are
// synthetic bytes in a new folder under the run's TEMP, never removed by the test.
#[cfg(test)]
mod m7_cp4 {
    use super::*;

    fn le32(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    /// V(n) finalized, from hound 3.5.1 32-bit float mono (68-byte header).
    fn v(n: usize) -> Vec<u8> {
        let spec = hound::WavSpec { channels: 1, sample_rate: 48_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut c = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut c, spec).unwrap();
            for i in 0..n {
                w.write_sample(((i % 97) as f32 / 97.0) - 0.5).unwrap();
            }
            w.finalize().unwrap();
        }
        let b = c.into_inner();
        assert_eq!((b.len(), &b[60..64], le32(&b, 64) as usize, le32(&b, 4) as usize), (68 + 4 * n, &b"data"[..], 4 * n, 60 + 4 * n), "fixture: V({n}) L, data tag, D, R");
        b
    }

    /// K(n): V(n) with R = 0 and D = 0 (killed writer); oracle REPAIRABLE.
    fn k(n: usize) -> Vec<u8> {
        let mut b = v(n);
        b[4..8].copy_from_slice(&0u32.to_le_bytes());
        b[64..68].copy_from_slice(&0u32.to_le_bytes());
        assert!(b.len() >= 72 && le32(&b, 64) == 0, "fixture: K({n}) is REPAIRABLE");
        b
    }

    /// Hk: the 68-byte header of a killed writer; oracle HEADER-ONLY.
    fn hk() -> Vec<u8> {
        let mut b = k(1);
        b.truncate(68);
        assert_eq!((b.len(), le32(&b, 4), le32(&b, 64)), (68, 0, 0), "fixture: Hk L, R, D");
        b
    }

    fn run_folder(tag: &str) -> PathBuf {
        let root = std::env::temp_dir();
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = root.join(format!("{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        assert!(dir.starts_with(&root), "fixture: {} is outside {}", dir.display(), root.display());
        dir
    }

    struct Gone;

    impl Owners for Gone {
        fn live(&self, _: Option<u64>, _: &str, _: &Path) -> bool {
            false
        }
        fn end(&self, _: u64) -> bool {
            false
        }
    }

    /// An interrupted session under a new MIRIS_HOME with the given tracks; the caller holds `test_lock`.
    fn interrupted(tag: &str, id: &str, tracks: &[(&str, &[u8])]) -> PathBuf {
        let home = run_folder(tag);
        let dir = home.join("sessions").join(id);
        std::fs::create_dir_all(&dir).unwrap();
        write(&dir, &json!({
            "schema_version": 1, "session_id": id, "state": "interrupted", "created_at": "x", "updated_at": "x",
            "recorder_pid": 4242, "processing_pid": null, "channels": {"mic": null, "system": null},
            "error": {"code": "owner_died", "message": "x"}, "note_path": null, "transcript_ready": false,
        }))
        .unwrap();
        for (name, bytes) in tracks {
            std::fs::write(dir.join(name), bytes).unwrap();
        }
        std::env::set_var("MIRIS_HOME", &home);
        dir
    }

    /// Every file directly in `dir` with its bytes.
    fn folder_bytes(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| e.path().is_file())
            .map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read(e.path()).unwrap()))
            .collect()
    }

    const RECOVER_NAMES: [&str; 7] = ["session.json", ".session.json.tmp", "recover.lock", "recover.journal.tmp", "recover.journal", "recover.done.json", "recover.journal.rejected"];

    /// The changes outside the allowlist: a name outside RECOVER_NAMES that appeared, vanished or changed,
    /// except bytes 4..8 and 64..68 of a track the oracle calls REPAIRABLE when the recovery succeeded.
    fn outside_allowlist(before: &std::collections::BTreeMap<String, Vec<u8>>, after: &std::collections::BTreeMap<String, Vec<u8>>, repairable: &[&str], ok: bool) -> Vec<String> {
        let mut bad = Vec::new();
        let names: std::collections::BTreeSet<&String> = before.keys().chain(after.keys()).collect();
        for n in names {
            if RECOVER_NAMES.contains(&n.as_str()) {
                continue;
            }
            match (before.get(n), after.get(n)) {
                (Some(b), Some(a)) if b == a => {}
                (Some(b), Some(a)) if ok && repairable.contains(&n.as_str()) && b.len() == a.len() => {
                    let off: Vec<usize> = (0..b.len()).filter(|&i| b[i] != a[i] && !(4..8).contains(&i) && !(64..68).contains(&i)).collect();
                    if !off.is_empty() {
                        bad.push(format!("{n}: bytes outside the size fields changed at {:?}", &off[..off.len().min(8)]));
                    }
                }
                (b, a) => bad.push(format!("{n}: {} -> {}", b.map_or("absent".into(), |b| format!("{} bytes", b.len())), a.map_or("absent".into(), |a| format!("{} bytes", a.len())))),
            }
        }
        bad
    }

    /// C38 (AC 21): `recover` changes only its allowlisted names and the two size fields of a REPAIRABLE track,
    /// and a refused recovery changes no track byte.
    #[test]
    fn m7_21_recover_changes_only_its_allowlisted_names_and_eight_bytes() {
        let _one_at_a_time = crate::test_lock();
        let mic = k(1000);
        let y = vec![b'x'; 100];
        let cases: [(&str, Vec<u8>, &[&str]); 5] = [
            ("K(1000) + E", Vec::new(), &["mic.wav"]),
            ("K(1000) + Hk", hk(), &["mic.wav"]),
            ("K(1000) + F(10)", hk()[..10].to_vec(), &["mic.wav"]),
            ("K(1000) + V(400)", v(400), &["mic.wav"]),
            ("K(1000) + Y (refused)", y, &["mic.wav"]),
        ];
        let mut failures = Vec::new();
        for (i, (name, sys, repairable)) in cases.into_iter().enumerate() {
            let id = format!("20261109-1321{i:02}");
            let dir = interrupted("m7_21", &id, &[("mic.wav", &mic), ("system.wav", &sys)]);
            let before = folder_bytes(&dir);

            let exit = cli_with(&Gone, "recover", &[id.clone()]);

            let after = folder_bytes(&dir);
            let bad = outside_allowlist(&before, &after, repairable, exit == 0);
            eprintln!("m7_21 {name} observed: exit={exit} outside_allowlist={bad:?}");
            if !bad.is_empty() {
                failures.push(format!("{name}: {bad:?}"));
            }
        }
        assert!(failures.is_empty(), "behavior: recover changed bytes outside its allowlist: {failures:?}");
    }

    /// C41 (AC 22), byte half: a second `recover` exits 0 and changes no byte (the recover.done.json half is S6, held).
    #[test]
    fn m7_22_a_second_recover_changes_nothing() {
        let _one_at_a_time = crate::test_lock();
        let mic = k(1000);
        let dir = interrupted("m7_22", "20261109-132200", &[("mic.wav", &mic), ("system.wav", &[])]);
        let first = cli_with(&Gone, "recover", &["20261109-132200".to_owned()]);
        let once = folder_bytes(&dir);

        let second = cli_with(&Gone, "recover", &["20261109-132200".to_owned()]);

        let twice = folder_bytes(&dir);
        let changed: Vec<&String> = once.keys().chain(twice.keys()).filter(|n| !RECOVER_NAMES.contains(&n.as_str()) && once.get(*n) != twice.get(*n)).collect();
        eprintln!("m7_22 observed: first={first} second={second} changed_by_second={changed:?}");
        assert_eq!(second, 0, "behavior: the second recover exits 0");
        assert!(changed.is_empty(), "behavior: the second recover changed {changed:?}");

        let control = interrupted("m7_22c", "20261109-132201", &[("mic.wav", &mic), ("system.wav", &k(500))]);
        let _ = cli_with(&Gone, "recover", &["20261109-132201".to_owned()]);
        let tracks_once = (std::fs::read(control.join("mic.wav")).unwrap(), std::fs::read(control.join("system.wav")).unwrap());
        let _ = cli_with(&Gone, "recover", &["20261109-132201".to_owned()]);
        assert_eq!((std::fs::read(control.join("mic.wav")).unwrap(), std::fs::read(control.join("system.wav")).unwrap()), tracks_once, "control: K(1000) + K(500) twice keeps the track bytes");
    }
}
