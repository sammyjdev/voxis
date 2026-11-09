
// M7 CP4 test-only module for session.rs over the S0 pipeline-child seam (checks C30, C31, C32, C39). Needs S0
// 8db41ea (process_with), so its red is measured at S0, not at c2d7b6c, where a process test would start the
// real child (the test binary). The child here is a closure; no process is started. Synthetic fixtures under TEMP.
#[cfg(test)]
mod m7_s2 {
    use super::*;
    use std::os::windows::process::ExitStatusExt;

    fn status(code: u32) -> std::process::ExitStatus {
        std::process::ExitStatus::from_raw(code)
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

    /// A session folder in `state` (dead pipeline owner 4242) holding a synthetic mic track.
    fn session(tag: &str, id: &str, state: &str) -> PathBuf {
        let dir = run_folder(tag).join("sessions").join(id);
        std::fs::create_dir_all(&dir).unwrap();
        write(&dir, &json!({
            "schema_version": 1, "session_id": id, "state": state, "created_at": "x", "updated_at": "x",
            "recorder_pid": null, "processing_pid": 4242, "channels": {"mic": null, "system": null},
            "error": null, "note_path": null, "transcript_ready": false,
        }))
        .unwrap();
        std::fs::write(dir.join("mic.wav"), b"RIFF synthetic track bytes").unwrap();
        dir
    }

    fn folder_bytes(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(dir).unwrap().flatten().filter(|e| e.path().is_file()).map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read(e.path()).unwrap())).collect()
    }

    /// C30 (AC 16): a child that exits 101 while the manifest says `processing` leaves the session `captured`
    /// with `pipeline_error` naming the exit status; files kept; process fails.
    #[test]
    fn m7_16_a_failed_child_settles_captured_with_pipeline_error() {
        let dir = session("m7_16", "20261109-141600", "processing");
        let mic = std::fs::read(dir.join("mic.wav")).unwrap();

        let r = process_with(&dir, &Gone, |_| Ok(status(101)));

        let m = read(&dir).unwrap();
        eprintln!("m7_16 observed: result={r:?} state={} error={}", m["state"], m["error"]);
        assert!(r.is_err(), "behavior: process fails");
        assert_eq!(m["state"], "captured", "behavior: state after a failed child");
        assert_eq!(m["error"]["code"], "pipeline_error", "behavior: error.code");
        assert!(m["error"]["message"].as_str().is_some_and(|s| s.contains("101")), "behavior: the message names exit status 101, got {}", m["error"]);
        assert_eq!(std::fs::read(dir.join("mic.wav")).unwrap(), mic, "behavior: files kept");
    }

    /// C31 (AC 16): a child that already settled `captured` with its own error keeps that code.
    #[test]
    fn m7_16_a_child_written_error_code_is_kept() {
        let dir = session("m7_16k", "20261109-141601", "processing");

        let r = process_with(&dir, &Gone, |d| {
            update(d, |m| {
                m["state"] = json!("captured");
                m["error"] = json!({"code": "provider_timeout", "message": "x"});
            })
            .unwrap();
            Ok(status(101))
        });

        let m = read(&dir).unwrap();
        eprintln!("m7_16k observed: result={r:?} state={} error={}", m["state"], m["error"]);
        assert_eq!((m["state"].as_str(), m["error"]["code"].as_str()), (Some("captured"), Some("provider_timeout")), "behavior: the child's own code is kept");
    }

    /// C32 (AC 17): `process` on an `interrupted` session is refused, changes nothing and starts no child.
    #[test]
    fn m7_17_process_refuses_an_interrupted_session() {
        let dir = session("m7_17", "20261109-141700", "interrupted");
        let before = folder_bytes(&dir);
        let called = std::cell::Cell::new(false);

        let r = process_with(&dir, &Gone, |_| {
            called.set(true);
            Ok(status(101))
        });

        eprintln!("m7_17 observed: result={r:?} child_called={}", called.get());
        assert!(!called.get(), "behavior: no pipeline child is started for an interrupted session");
        assert_eq!(r, Err("session 20261109-141700 is interrupted; run recover first".to_owned()), "behavior: refusal message");
        assert_eq!(folder_bytes(&dir), before, "behavior: nothing in the folder changes");
    }

    /// C39 (AC 21): `process` changes only its allowlisted names (the child writes a transcript, minutes, note and manifest).
    #[test]
    fn m7_21_process_changes_only_its_allowlisted_names() {
        let dir = session("m7_21p", "20261109-142100", "captured");
        let before = folder_bytes(&dir);

        let r = process_with(&dir, &Gone, |d| {
            for (name, bytes) in [("transcript.md", "# Transcrição\n"), ("ata.md", "# Ata\n"), ("minutes.json", "{}"), ("note.path", "x"), ("ata.url", "[InternetShortcut]\n")] {
                std::fs::write(d.join(name), bytes).unwrap();
            }
            update(d, |m| m["state"] = json!("completed")).unwrap();
            Ok(status(0))
        });

        let after = folder_bytes(&dir);
        let allowed = |n: &str| ["transcript.md", "ata.md", "minutes.json", "note.path", "session.json", ".session.json.tmp", "recover.lock"].contains(&n) || n.ends_with(".url");
        let outside: Vec<&String> = before.keys().chain(after.keys()).filter(|n| !allowed(n) && before.get(*n) != after.get(*n)).collect();
        eprintln!("m7_21p observed: result={r:?} outside_allowlist={outside:?}");
        assert_eq!(r, Ok(()), "behavior: process succeeds");
        assert!(outside.is_empty(), "behavior: process changed {outside:?}");
    }
}
