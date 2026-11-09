
// M7 CP4 test-only module for main.rs (checks C26, C28 and C37 of .specs/features/m7-silent-track-recovery/checks.md).
// Appended unchanged to main.rs at c2d7b6c and at HEAD; tests only, no production line. Fixtures are synthetic
// bytes in a new folder under the run's TEMP, never removed by the test.
#[cfg(test)]
mod m7_tests {
    use super::*;
    use std::io::{Seek, Write};

    fn le32(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    /// The 68-byte product header (hound 3.5.1, 32-bit float mono) with R = 0 and D = 0; fixed fields checked.
    fn hk() -> Vec<u8> {
        let spec = hound::WavSpec { channels: 1, sample_rate: 48_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut c = std::io::Cursor::new(Vec::new());
        hound::WavWriter::new(&mut c, spec).unwrap().finalize().unwrap();
        let mut b = c.into_inner();
        assert_eq!((b.len(), &b[0..4], &b[8..12], &b[60..64], le32(&b, 16)), (68, &b"RIFF"[..], &b"WAVE"[..], &b"data"[..], 40), "fixture: product header");
        b[4..8].copy_from_slice(&0u32.to_le_bytes());
        b
    }

    /// V(n): a finalized product WAV of n samples.
    fn v(n: usize) -> Vec<u8> {
        let spec = hound::WavSpec { channels: 1, sample_rate: 48_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut c = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut c, spec).unwrap();
            for i in 0..n {
                w.write_sample(((i % 89) as f32 / 89.0) - 0.5).unwrap();
            }
            w.finalize().unwrap();
        }
        let b = c.into_inner();
        assert_eq!((b.len(), le32(&b, 64) as usize), (68 + 4 * n, 4 * n), "fixture: V({n}) L and D");
        b
    }

    fn run_folder(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir();
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = root.join(format!("{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        assert!(dir.starts_with(&root), "fixture: {} is outside {}", dir.display(), root.display());
        dir
    }

    /// `load_16k` on a file, with any panic caught: Ok(debug text of the return value) or Err(panic text).
    fn load_caught(path: &Path) -> Result<String, String> {
        let p = path.to_path_buf();
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let r = std::panic::catch_unwind(move || {
            let shown = format!("{:?}", load_16k(&p));
            shown.chars().take(120).collect::<String>()
        });
        std::panic::set_hook(prev);
        r.map_err(|e| e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default())
    }

    /// C26 (AC 14): `load_16k` on an empty or partial track returns without a panic; Hf is the control.
    #[test]
    fn m7_14_load_16k_does_not_panic_on_an_empty_or_partial_track() {
        let _one_at_a_time = crate::test_lock();
        let dir = run_folder("m7_14");
        let h = hk();
        let cases: [(&str, Vec<u8>); 4] = [("E", Vec::new()), ("F(1)", h[..1].to_vec()), ("F(67)", h[..67].to_vec()), ("Hf", {
            let mut f = h.clone();
            f[4..8].copy_from_slice(&60u32.to_le_bytes());
            f
        })];
        let mut panicked = Vec::new();
        for (name, bytes) in &cases {
            let p = dir.join(format!("{}.wav", name.replace(['(', ')'], "_")));
            std::fs::write(&p, bytes).unwrap();
            let r = load_caught(&p);
            eprintln!("m7_14 {name} observed: {r:?}");
            if r.is_err() {
                panicked.push(*name);
            }
        }
        assert!(panicked.is_empty(), "behavior: load_16k panicked on {panicked:?}");
    }

    /// C28 (AC 15): an unreadable track (X, or T(1000, 400) whose samples end early) gives an error naming the file, no panic.
    #[test]
    fn m7_15_an_unreadable_track_returns_an_error_naming_the_file() {
        let _one_at_a_time = crate::test_lock();
        let dir = run_folder("m7_15");
        let mut x = vec![b'x'; 40];
        x.extend_from_slice(b"data");
        x.extend_from_slice(&[b'x'; 56]);
        let mut t = v(400);
        t[64..68].copy_from_slice(&4000u32.to_le_bytes());
        t[4..8].copy_from_slice(&4060u32.to_le_bytes());
        assert_eq!((x.len(), t.len(), le32(&t, 64)), (100, 1668, 4000), "fixture: X length, T(1000, 400) L and D");
        for (name, bytes) in [("X", x), ("T(1000, 400)", t)] {
            let sub = dir.join(name.replace(['(', ')', ',', ' '], "_"));
            std::fs::create_dir(&sub).unwrap();
            let p = sub.join("system.wav");
            std::fs::write(&p, &bytes).unwrap();
            let r = load_caught(&p);
            eprintln!("m7_15 {name} observed: {r:?}");
            let shown = r.unwrap_or_else(|e| panic!("behavior: {name} panicked: {e}"));
            assert!(shown.starts_with("Err(") && shown.contains("system.wav"), "behavior: {name} must give an error naming system.wav, got {shown}");
        }
    }

    /// An in-memory file shared with the test, so the bytes that reached the underlying writer can be read.
    #[derive(Clone)]
    struct Shared(std::sync::Arc<std::sync::Mutex<std::io::Cursor<Vec<u8>>>>);

    impl std::io::Write for Shared {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().write(b)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl std::io::Seek for Shared {
        fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
            self.0.lock().unwrap().seek(to)
        }
    }

    /// C37 (AC 20): the 68-byte header reaches the underlying writer before the first chunk is received.
    #[test]
    fn m7_20_the_header_reaches_the_file_before_the_first_chunk() {
        let file = Shared(std::sync::Arc::new(std::sync::Mutex::new(std::io::Cursor::new(Vec::new()))));
        let (tx, rx) = mpsc::channel::<Chunk>();
        let inner = file.clone();
        let writer = std::thread::spawn(move || write_wav(inner, 48_000, rx, || Ok(std::io::Cursor::new(Vec::new()))));
        let started = std::time::Instant::now();
        let mut seen = Vec::new();
        while started.elapsed() < std::time::Duration::from_secs(2) {
            seen = file.0.lock().unwrap().get_ref().clone();
            if seen.len() >= 68 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        drop(tx);
        let _ = writer.join();
        eprintln!("m7_20 observed: {} bytes reached the writer within {:?}", seen.len(), started.elapsed());
        assert!(seen.len() >= 68, "behavior: the underlying writer holds {} bytes within 2 s, not the 68-byte header", seen.len());
        assert_eq!(&seen[60..64], b"data", "behavior: data tag at 60..64");
    }
}
