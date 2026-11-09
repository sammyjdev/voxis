
// M7 CP4 test-only module for main.rs over the S0 per-track read (check C27). Needs S0 240045b (track_audio),
// so its red is measured at S0, not at c2d7b6c. Tests only; fixtures are synthetic bytes under the run's TEMP.
#[cfg(test)]
mod m7_s2_tests {
    use super::*;

    fn le32(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    /// V(n) at 48 kHz, finalized; 48000 samples are 1 s.
    fn v48k(n: usize) -> Vec<u8> {
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
        assert_eq!((b.len(), &b[0..4], &b[60..64], le32(&b, 64) as usize), (68 + 4 * n, &b"RIFF"[..], &b"data"[..], 4 * n), "fixture: V({n}) L, tags, D");
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

    /// `track_audio` with any panic caught: Ok((samples or None, printed text)) or Err(panic text).
    fn read_caught(dir: &Path, file: &str) -> Result<(Option<usize>, String), String> {
        let (d, f) = (dir.to_path_buf(), file.to_owned());
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let r = std::panic::catch_unwind(move || {
            let mut out = Vec::new();
            let a = track_audio(&d, &f, &mut out).map(|a| a.len());
            (a, String::from_utf8_lossy(&out).into_owned())
        });
        std::panic::set_hook(prev);
        r.map_err(|e| e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default())
    }

    /// C27 (AC 14): the per-track read skips an EMPTY, FRAGMENT or HEADER-ONLY track with its line, without a
    /// panic, and still reads the other track; a VALID track under 1 s keeps today's line.
    #[test]
    fn m7_14_the_per_track_read_skips_a_track_with_no_audio() {
        let _one_at_a_time = crate::test_lock();
        let one_second = v48k(48_000);
        let mut hk = v48k(1);
        hk.truncate(68);
        hk[4..8].copy_from_slice(&0u32.to_le_bytes());
        hk[64..68].copy_from_slice(&0u32.to_le_bytes());
        let cases: [(&str, Vec<u8>); 4] = [("0 bytes", Vec::new()), ("1 byte", hk[..1].to_vec()), ("67 bytes", hk[..67].to_vec()), ("68 bytes", hk.clone())];
        let mut failures = Vec::new();
        for (name, sys) in cases {
            let dir = run_folder("m7_14s");
            std::fs::write(dir.join("mic.wav"), &one_second).unwrap();
            std::fs::write(dir.join("system.wav"), &sys).unwrap();

            let system = read_caught(&dir, "system.wav");
            let mic = read_caught(&dir, "mic.wav");

            eprintln!("m7_14s {name} observed: system={system:?} mic={mic:?}");
            match system {
                Ok((None, out)) if out.lines().any(|l| l == "system.wav: no audio, skipped") => {}
                other => failures.push(format!("{name}: system.wav must be skipped with 'system.wav: no audio, skipped', got {other:?}")),
            }
            if !matches!(mic, Ok((Some(n), _)) if n >= 16_000) {
                failures.push(format!("{name}: mic.wav must still be read, got {mic:?}"));
            }
        }
        let dir = run_folder("m7_14u");
        std::fs::write(dir.join("system.wav"), v48k(100)).unwrap();
        let short = read_caught(&dir, "system.wav");
        eprintln!("m7_14s under 1 s observed: {short:?}");
        if !matches!(&short, Ok((None, out)) if out.lines().any(|l| l == "system.wav: under 1 s of audio, skipped")) {
            failures.push(format!("a VALID track under 1 s keeps its line, got {short:?}"));
        }
        assert!(failures.is_empty(), "behavior: {failures:?}");
    }
}
