
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
}
