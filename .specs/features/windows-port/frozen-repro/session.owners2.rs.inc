
// ---- appended by scripts/windows/frozen-repro.sh to session.rs of the frozen SHA 889d48c (round 2) ----
// Criterion 64 / review finding P5-6: criteria 86, 14, 16 and the identity step of 86, through the
// frozen `cli`, `start` and `stop` as the CLI calls them, with MIRIS_HOME on a temporary folder.
//
// No microphone or device is opened. The "live owner" is a `ping` process this test starts and ends.
// The frozen `spawn_child` runs `current_exe()`, which in a test run is this test binary: started as
// `<exe> miris.pipeline <dir>` or `<exe> miris.recorder <dir>` it reads both words as test name
// filters, runs no test and exits 0, so no pipeline and no recorder ever run.
#[cfg(test)]
mod frozen_repro_r2 {
    use super::*;

    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// A harmless live process that is not a recorder; ended on drop.
    struct Owner(std::process::Child);

    impl Owner {
        fn start() -> Owner {
            Owner(
                Command::new("ping")
                    .args(["-n", "60", "127.0.0.1"])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .unwrap(),
            )
        }
    }

    impl Drop for Owner {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn exited_pid() -> u32 {
        let mut child = Command::new("cmd").args(["/C", "exit", "0"]).spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        pid
    }

    /// A session folder with a mic.wav holding the zero-size header of a killed recorder.
    fn session(home: &Path, id: &str, state: &str, recorder: Option<u32>, processing: Option<u32>) -> PathBuf {
        let dir = home.join("sessions").join(id);
        std::fs::create_dir_all(&dir).unwrap();
        write(&dir, &json!({
            "schema_version": 1, "session_id": id, "state": state, "created_at": now(), "updated_at": now(),
            "recorder_pid": recorder, "processing_pid": processing, "channels": {"mic": null, "system": null},
            "error": null, "note_path": null, "transcript_ready": false,
        }))
        .unwrap();
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
        dir
    }

    #[test]
    fn control_a_session_with_a_live_owner_is_found_active_by_the_frozen_reader() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        let owner = Owner::start();
        let dir = session(home.path(), "20261107-110000", "recording", Some(owner.0.id()), None);
        std::env::set_var("MIRIS_HOME", home.path());

        assert_eq!(newest_with_state(&["recording"]), Some(dir));
    }

    #[test]
    fn recover_refuses_a_session_whose_owner_is_live_and_leaves_every_byte() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        let owner = Owner::start();
        let dir = session(home.path(), "20261107-110000", "recording", Some(owner.0.id()), None);
        std::env::set_var("MIRIS_HOME", home.path());
        let (wav, manifest) = (std::fs::read(dir.join("mic.wav")).unwrap(), std::fs::read(dir.join("session.json")).unwrap());

        let code = cli("recover", &["20261107-110000".to_owned()]);

        assert_eq!(std::fs::read(dir.join("mic.wav")).unwrap(), wav, "the recording was rewritten under a live owner");
        assert_eq!(std::fs::read(dir.join("session.json")).unwrap(), manifest, "the manifest was rewritten under a live owner");
        assert_eq!(code, 1, "recover ran on a session with a live owner");
    }

    #[test]
    fn process_refuses_a_session_whose_owner_is_live_and_leaves_every_byte() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        let owner = Owner::start();
        let dir = session(home.path(), "20261107-110000", "recording", Some(owner.0.id()), None);
        std::env::set_var("MIRIS_HOME", home.path());
        let (wav, manifest) = (std::fs::read(dir.join("mic.wav")).unwrap(), std::fs::read(dir.join("session.json")).unwrap());

        let code = cli("process", &["20261107-110000".to_owned()]);

        assert_eq!(std::fs::read(dir.join("mic.wav")).unwrap(), wav);
        assert_eq!(std::fs::read(dir.join("session.json")).unwrap(), manifest);
        assert_eq!(code, 1, "process started a pipeline on a session with a live owner");
    }

    #[test]
    fn stop_does_not_signal_a_process_it_has_not_identified_as_the_recorder() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        let unrelated = Owner::start();
        let dir = session(home.path(), "20261107-110000", "recording", Some(unrelated.0.id()), None);
        std::env::set_var("MIRIS_HOME", home.path());
        std::thread::spawn(|| {
            let _ = stop();
        });

        std::thread::sleep(std::time::Duration::from_millis(1500));

        assert!(!dir.join("stop").exists(), "the stop file was written for a process that is not this session's recorder");
    }

    #[test]
    fn a_processing_session_whose_pipeline_is_dead_does_not_block_a_new_start() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        session(home.path(), "20261107-110000", "processing", None, Some(exited_pid()));
        std::env::set_var("MIRIS_HOME", home.path());

        let started = start();

        assert!(!matches!(&started, Err(e) if e.contains("already active")), "start refused: {started:?}");
    }

    #[test]
    fn a_recorder_that_exits_before_recording_leaves_no_session_in_starting() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        std::env::set_var("MIRIS_HOME", home.path());

        let started = start();

        let states: Vec<String> = std::fs::read_dir(home.path().join("sessions"))
            .unwrap()
            .filter_map(|e| read(&e.unwrap().path()).ok())
            .map(|m| m["state"].as_str().unwrap_or("").to_owned())
            .collect();
        assert!(started.is_err(), "the recorder never reached recording, start must fail: {started:?}");
        assert!(!states.iter().any(|s| s == "starting"), "a session was left in starting: {states:?}");
    }
}
