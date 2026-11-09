
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
