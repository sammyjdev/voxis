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

fn start() -> Result<(), String> {
    if let Some(dir) = newest_with_state(&["starting", "recording", "stopping", "processing"]) {
        return Err(format!("a session is already active: {}", dir.display()));
    }
    let id = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let dir = sessions_dir().join(&id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let t = now();
    write(&dir, &json!({
        "schema_version": 1, "session_id": id, "state": "starting", "created_at": t, "updated_at": t,
        "recorder_pid": null, "processing_pid": null, "channels": {"mic": null, "system": null},
        "error": null, "note_path": null, "transcript_ready": false,
    }))?;
    let child = spawn_child("miris.recorder", &dir, true)?;
    update(&dir, |m| m["recorder_pid"] = json!(child.id()))?;
    // opening WASAPI takes about a second
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        match read(&dir)?["state"].as_str() {
            Some("recording") => {
                println!("recording: {}", dir.display());
                return Ok(());
            }
            Some("failed") | Some("interrupted") => break,
            _ => {}
        }
    }
    let log = std::fs::read_to_string(dir.join("recorder.log")).unwrap_or_default();
    Err(format!("recorder did not start: {}", log.lines().last().unwrap_or("no output")))
}

fn recorder(dir: &Path) -> Result<(), String> {
    let stop_file = dir.join("stop");
    let mic = configured_mic();
    let started = || {
        let _ = update(dir, |m| m["state"] = json!("recording"));
    };
    let (mic_t, sys_t) = super::record(dir, mic.as_deref(), super::Stop::File(stop_file.clone()), started);
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
    update(dir, |m| {
        m["state"] = json!("processing");
        m["processing_pid"] = json!(std::process::id());
        m["error"] = Value::Null;
    })?;
    let (model, vad) = super::models()?;
    super::transcribe(dir, &model.to_string_lossy(), &vad.to_string_lossy(), "pt");
    update(dir, |m| m["transcript_ready"] = json!(true))?;
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
    update(&dir, |m| m["state"] = json!("stopping"))?;
    std::fs::write(dir.join("stop"), b"").map_err(|e| e.to_string())?;
    // the recorder flips the manifest to captured once both WAVs are final
    for _ in 0..600 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if read(&dir)?["state"] == "captured" {
            return run_pipeline(&dir);
        }
    }
    fail(&dir, "interrupted", "recorder_timeout", "recorder did not finish within 60 s");
    println!("recording interrupted: {}", dir.display());
    Ok(())
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
    let arg = |i: usize| args.get(i).map(String::as_str).ok_or_else(|| format!("{cmd}: missing argument"));
    let result = match cmd {
        "start" => start(),
        "stop" => stop(),
        "process" => arg(0).and_then(session_dir).and_then(|d| run_pipeline(&d)),
        "recover" => arg(0).and_then(session_dir).and_then(|d| {
            for f in ["mic.wav", "system.wav"] {
                if d.join(f).exists() {
                    repair_wav(&d.join(f))?;
                }
            }
            update(&d, |m| {
                m["state"] = json!("captured");
                m["error"] = Value::Null;
            })
        }),
        "miris.recorder" => arg(0).map(PathBuf::from).and_then(|d| {
            recorder(&d).inspect_err(|e| fail(&d, "failed", "recorder_error", e))
        }),
        "miris.pipeline" => arg(0).map(PathBuf::from).and_then(|d| match pipeline(&d) {
            Ok(note) => {
                println!("note: {}", note.display());
                Ok(())
            }
            Err(e) => {
                // back to captured so the tray offers "Processar gravação"
                fail(&d, "captured", "pipeline_error", &e);
                Err(e)
            }
        }),
        _ => Err(format!("unknown command {cmd}")),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
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
}
