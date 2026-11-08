
// ---- appended by scripts/windows/frozen-repro.sh to session.rs of the frozen SHA 889d48c ----
// Criterion 64 / review finding P5-6: F19 and criterion 86 through the frozen `start` and `stop`,
// called as the CLI calls them, with MIRIS_HOME pointing at a temporary folder holding one
// session whose recorder is a process that has already exited.
//
// The script refuses to run unless the tree is exactly the frozen SHA: on the candidate `start`
// admits the session and spawns a recorder, which opens the microphone.
#[cfg(test)]
mod frozen_repro {
    use super::*;

    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// The pid of a process that has exited.
    fn dead_pid() -> u32 {
        let mut child = Command::new("cmd").args(["/C", "exit", "0"]).spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        pid
    }

    fn home_with_a_dead_recorder(state: &str) -> (tempfile::TempDir, PathBuf) {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join("sessions").join("20261107-100000");
        std::fs::create_dir_all(&dir).unwrap();
        write(&dir, &json!({
            "schema_version": 1, "session_id": "20261107-100000", "state": state,
            "created_at": now(), "updated_at": now(), "recorder_pid": dead_pid(), "processing_pid": null,
            "channels": {"mic": null, "system": null}, "error": null, "note_path": null, "transcript_ready": false,
        }))
        .unwrap();
        std::env::set_var("MIRIS_HOME", home.path());
        (home, dir)
    }

    #[test]
    fn control_an_empty_home_has_no_active_session() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        std::env::set_var("MIRIS_HOME", home.path());

        assert_eq!(newest_with_state(&["starting", "recording", "stopping", "processing"]), None);
    }

    #[test]
    fn a_session_whose_recorder_is_dead_does_not_block_a_new_start() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let (_home, _dir) = home_with_a_dead_recorder("recording");

        let started = start();

        assert!(!matches!(&started, Err(e) if e.contains("already active")), "start refused: {started:?}");
    }

    #[test]
    fn stop_settles_a_session_whose_recorder_is_dead_without_waiting_60_seconds() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let (_home, dir) = home_with_a_dead_recorder("recording");
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = done_tx.send(stop());
        });

        let finished = done_rx.recv_timeout(std::time::Duration::from_secs(5));

        assert!(finished.is_ok(), "stop was still waiting for a recorder that is gone after 5 s");
        let state = read(&dir).unwrap()["state"].clone();
        assert_ne!(state, "stopping", "the session stayed in stopping");
    }
}
