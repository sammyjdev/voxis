
// ---- appended by scripts/windows/frozen-repro.sh to session.rs of the frozen SHA 889d48c (certificates round) ----
// Criterion 10 through the frozen `start`: `start` makes `sessions/<YYYYMMDD-HHMMSS>` for the second it
// runs in, which cannot be known in advance, so a folder exists for each of the next three seconds
// (this second and the two after it). Whichever second `start` picks, its folder already exists. Each
// folder holds a finished manifest, a recording and a sentinel; the whole tree is compared byte for
// byte afterwards. A `start` that moved to a free second (it cannot in this window) would add a
// folder, and the test fails on that too instead of passing.
//
// No device is opened: the frozen `spawn_child` starts this test binary, which runs no test.
#[cfg(test)]
mod frozen_repro_r3 {
    use super::*;

    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

    fn folders_at_the_generated_second(sessions: &Path) -> Vec<PathBuf> {
        (0..3)
            .map(|i| {
                let id = (chrono::Local::now() + chrono::Duration::seconds(i)).format("%Y%m%d-%H%M%S").to_string();
                let dir = sessions.join(&id);
                std::fs::create_dir_all(&dir).unwrap();
                write(&dir, &json!({
                    "schema_version": 1, "session_id": id, "state": "completed", "created_at": "x", "updated_at": "x",
                    "recorder_pid": null, "processing_pid": null, "channels": {"mic": null, "system": null},
                    "error": null, "note_path": null, "transcript_ready": false,
                }))
                .unwrap();
                std::fs::write(dir.join("mic.wav"), b"audio").unwrap();
                std::fs::write(dir.join("sentinel.txt"), format!("must survive {id}")).unwrap();
                dir
            })
            .collect()
    }

    #[test]
    fn control_finished_sessions_do_not_block_admission() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        folders_at_the_generated_second(&home.path().join("sessions"));
        std::env::set_var("MIRIS_HOME", home.path());

        assert_eq!(newest_with_state(&["starting", "recording", "stopping", "processing"]), None);
    }

    #[test]
    fn start_does_not_reuse_a_session_folder_at_the_generated_second() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        let sessions = home.path().join("sessions");
        let taken = folders_at_the_generated_second(&sessions);
        std::env::set_var("MIRIS_HOME", home.path());
        let before = tree_bytes(&sessions);

        let started = start();

        let after = tree_bytes(&sessions);
        let changed: Vec<&String> = after.keys().filter(|k| before.get(*k) != after.get(*k)).collect();
        assert!(changed.is_empty(), "files of an existing session were rewritten or added: {changed:?}");
        assert_eq!(std::fs::read_dir(&sessions).unwrap().count(), taken.len(), "a new folder appeared: no collision was exercised");
        assert!(started.is_err(), "start succeeded on an existing folder: {started:?}");
    }
}
