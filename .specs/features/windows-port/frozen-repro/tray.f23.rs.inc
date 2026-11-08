
// ---- appended by scripts/windows/frozen-repro.sh to apps/tray/src-tauri/src/miris.rs of the frozen SHA 889d48c ----
// Criterion 15 / F23 through the frozen tray reader: a recorder whose session folder sits under a
// profile path with a space. The stand-in is a `powershell` process started as the frozen
// `a_live_recorder_keeps_the_meeting_recording_on_windows` test starts one (an inline -Command,
// no script file); its command line names the marker and the session folder as one argument.
#[cfg(all(test, windows))]
mod frozen_repro {
    use super::*;

    struct StandIn(std::process::Child);

    impl Drop for StandIn {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn recording_under(sessions: &Path) -> SessionState {
        let dir = sessions.join("20261107-143200");
        std::fs::create_dir_all(&dir).unwrap();
        let stand_in = StandIn(
            Command::new("powershell")
                .args(["-NoProfile", "-Command", "Start-Sleep 20;#", "miris.recorder"])
                .arg(&dir)
                .spawn()
                .unwrap(),
        );
        let body = serde_json::json!({
            "schema_version": 1, "session_id": "20261107-143200", "state": "recording",
            "created_at": "2026-11-07T14:32:00-03:00", "updated_at": "2026-11-07T14:32:01-03:00",
            "recorder_pid": stand_in.0.id(), "processing_pid": serde_json::Value::Null,
            "channels": {"mic": null, "system": null}, "error": serde_json::Value::Null,
            "note_path": serde_json::Value::Null, "transcript_ready": false
        });
        std::fs::write(dir.join("session.json"), body.to_string()).unwrap();
        snapshot(sessions).active.map(|a| a.derived_state).unwrap_or(SessionState::Interrupted)
    }

    #[test]
    fn control_a_live_recorder_under_a_path_without_spaces_is_recording() {
        let tmp = tempfile::tempdir().unwrap();

        assert_eq!(recording_under(&tmp.path().join("SamDoe")), SessionState::Recording);
    }

    #[test]
    fn a_live_recorder_under_a_profile_path_with_a_space_is_recording() {
        let tmp = tempfile::tempdir().unwrap();

        assert_eq!(recording_under(&tmp.path().join("Sam Doe")), SessionState::Recording);
    }
}
