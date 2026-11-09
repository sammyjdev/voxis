
// M7 CP5 test-only module for session.rs: C20, the MISMATCH refusal before any write. Uses the CP3 S0 output
// seam cli_to (3184165), so it cannot run at c2d7b6c. Synthetic fixtures under the run's TEMP, never removed.
#[cfg(test)]
mod m7_cp5b {
    use super::*;

    fn le32(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    /// V(n): a finalized product WAV of n samples.
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
        assert_eq!((b.len(), &b[60..64], le32(&b, 64) as usize), (68 + 4 * n, &b"data"[..], 4 * n), "fixture: V({n}) L, data tag, D");
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

    /// C20 (AC 30, AC 9): a MISMATCH track refuses the whole recovery before any byte of either track is written.
    #[test]
    fn m7_30_a_mismatch_track_refuses_the_whole_recovery() {
        let _one_at_a_time = crate::test_lock();
        let mut mic = v(1000);
        mic[4..8].copy_from_slice(&0u32.to_le_bytes());
        mic[64..68].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!((mic.len(), le32(&mic, 64)), (4068, 0), "fixture: K(1000) L, D (REPAIRABLE)");
        let mut sys = v(1000);
        sys[64..68].copy_from_slice(&1600u32.to_le_bytes());
        sys[4..8].copy_from_slice(&1660u32.to_le_bytes());
        assert_eq!((le32(&sys, 64), sys.len(), sys.len() - 68), (1600, 4068, 4000), "fixture: T(400, 1000) D, L, W (MISMATCH: 0 < D < W)");
        let home = run_folder("m7_30");
        let id = "20261109-143000";
        let dir = home.join("sessions").join(id);
        std::fs::create_dir_all(&dir).unwrap();
        write(&dir, &json!({
            "schema_version": 1, "session_id": id, "state": "interrupted", "created_at": "x", "updated_at": "x",
            "recorder_pid": 4242, "processing_pid": null, "channels": {"mic": null, "system": null},
            "error": {"code": "owner_died", "message": "x"}, "note_path": null, "transcript_ready": false,
        }))
        .unwrap();
        std::fs::write(dir.join("mic.wav"), &mic).unwrap();
        std::fs::write(dir.join("system.wav"), &sys).unwrap();
        let manifest = std::fs::read(dir.join("session.json")).unwrap();
        std::env::set_var("MIRIS_HOME", &home);
        let (mut out, mut err) = (Vec::new(), Vec::new());

        let exit = cli_to(&Gone, "recover", &[id.to_owned()], &mut out, &mut err);

        let (out, err) = (String::from_utf8_lossy(&out).into_owned(), String::from_utf8_lossy(&err).into_owned());
        let journal: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("recover.journal")).collect();
        eprintln!("m7_30 observed: exit={exit} stdout={out:?} stderr={err:?} journal={journal:?}");
        assert_eq!(std::fs::read(dir.join("mic.wav")).unwrap(), mic, "behavior: mic.wav byte-identical (no write before the refusal)");
        assert_eq!(std::fs::read(dir.join("system.wav")).unwrap(), sys, "behavior: system.wav byte-identical (declaration neither shortened nor extended)");
        assert_eq!(std::fs::read(dir.join("session.json")).unwrap(), manifest, "behavior: session.json unchanged, state interrupted");
        assert!(journal.is_empty(), "behavior: no recover.journal* file");
        assert!(err.lines().any(|l| l == "system.wav: declared 1600 bytes but 4000 bytes of samples are present"), "behavior: exact refusal text, got {err:?}");
        assert_eq!(exit, 1, "behavior: exit");
    }
}
