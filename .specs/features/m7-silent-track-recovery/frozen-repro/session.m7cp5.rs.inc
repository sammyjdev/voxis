
// M7 CP5 test-only module for session.rs: the classifier truth table (C6, C7) over virtual readers.
// Needs the S0 classify stub (4ea06d8), so its red is measured at S0, not at c2d7b6c. No real large file.
#[cfg(test)]
mod m7_cp5 {
    use super::*;

    /// A virtual track of `len` bytes: `head`, then zeros. Read and Seek only.
    struct Virtual {
        len: u64,
        head: Vec<u8>,
        pos: u64,
    }

    impl std::io::Read for Virtual {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = (buf.len() as u64).min(self.len.saturating_sub(self.pos)) as usize;
            for (i, b) in buf[..n].iter_mut().enumerate() {
                let at = self.pos + i as u64;
                *b = if at < self.head.len() as u64 { self.head[at as usize] } else { 0 };
            }
            self.pos += n as u64;
            Ok(n)
        }
    }

    impl std::io::Seek for Virtual {
        fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
            self.pos = match to {
                std::io::SeekFrom::Start(p) => p,
                std::io::SeekFrom::End(d) => (self.len as i64 + d) as u64,
                std::io::SeekFrom::Current(d) => (self.pos as i64 + d) as u64,
            };
            Ok(self.pos)
        }
    }

    /// The 68-byte product header (hound 3.5.1, 32-bit float mono) with the given R and D; fixed fields checked.
    fn header(r: u32, d: u32) -> Vec<u8> {
        let spec = hound::WavSpec { channels: 1, sample_rate: 48_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut c = std::io::Cursor::new(Vec::new());
        hound::WavWriter::new(&mut c, spec).unwrap().finalize().unwrap();
        let mut b = c.into_inner();
        assert_eq!((b.len(), &b[0..4], &b[8..12], &b[12..16], &b[60..64]), (68, &b"RIFF"[..], &b"WAVE"[..], &b"fmt "[..], &b"data"[..]), "fixture: product header");
        b[4..8].copy_from_slice(&r.to_le_bytes());
        b[64..68].copy_from_slice(&d.to_le_bytes());
        b
    }

    fn class_of(len: u64, head: Vec<u8>) -> Class {
        classify(&mut Virtual { len, head, pos: 0 })
    }

    /// The boundary table of the plan, each row with the class the truth table gives (the oracle).
    fn boundary_rows() -> Vec<(&'static str, u64, Vec<u8>, Class)> {
        let h = header(0, 0);
        vec![
            ("L=0", 0, Vec::new(), Class::Empty),
            ("L=1 F(1)", 1, h[..1].to_vec(), Class::Fragment),
            ("L=43 F(43)", 43, h[..43].to_vec(), Class::Fragment),
            ("L=67 F(67)", 67, h[..67].to_vec(), Class::Fragment),
            ("L=30 Z(30)", 30, vec![b'x'; 30], Class::Unreadable),
            ("L=68 D=0 R=0", 68, header(0, 0), Class::HeaderOnly),
            ("L=68 D=0 R=60", 68, header(60, 0), Class::HeaderOnly),
            ("L=68 D=4294967295", 68, header(0, u32::MAX), Class::HeaderOnly),
            ("L=69", 69, header(0, 0), Class::HeaderOnly),
            ("L=71", 71, header(0, 8), Class::HeaderOnly),
            ("L=72 D=0", 72, header(0, 0), Class::Repairable),
            ("L=72 D=4 R=64", 72, header(64, 4), Class::Valid),
            ("L=72 D=4 R=0", 72, header(0, 4), Class::Valid),
            ("L=72 D=8", 72, header(0, 8), Class::Repairable),
            ("L=76 D=4", 76, header(0, 4), Class::Mismatch),
            ("L=76 D=6", 76, header(0, 6), Class::Mismatch),
            ("L=73 D=4", 73, header(0, 4), Class::Valid),
            ("L=75 D=4", 75, header(0, 4), Class::Valid),
            ("L=73 D=0", 73, header(0, 0), Class::Repairable),
            ("L=75 D=0", 75, header(0, 0), Class::Repairable),
            ("L=4294967304", 4_294_967_304, header(0, 0), Class::Unreadable),
            ("L=4294967303 D=0", 4_294_967_303, header(0, 0), Class::Repairable),
            ("L=100 Y (no product header)", 100, vec![b'x'; 100], Class::Unreadable),
        ]
    }

    /// C6 (AC 29): every row of the boundary table gives exactly its class.
    #[test]
    fn m7_29_every_boundary_row_gives_its_class() {
        let rows = boundary_rows();
        assert_eq!(rows.len(), 23, "fixture: 23 rows cover the 16 rows of the boundary table");
        let wrong: Vec<String> = rows
            .into_iter()
            .filter_map(|(name, len, head, want)| {
                let got = class_of(len, head);
                (got != want).then(|| format!("{name}: {got:?}, want {want:?}"))
            })
            .collect();
        eprintln!("m7_29 observed wrong rows: {wrong:?}");
        assert!(wrong.is_empty(), "behavior: {} rows give the wrong class: {wrong:?}", wrong.len());
    }

    /// C7 (AC 29): R in {0, 60, D + 60} gives the oracle class on every product-header row.
    #[test]
    fn m7_29_riff_size_alone_never_changes_the_class() {
        let mut wrong = Vec::new();
        for (name, len, head, want) in boundary_rows() {
            if head.len() < 68 {
                continue;
            }
            let d = u32::from_le_bytes([head[64], head[65], head[66], head[67]]);
            for r in [0u32, 60, d.wrapping_add(60)] {
                let mut h = head.clone();
                h[4..8].copy_from_slice(&r.to_le_bytes());
                let got = class_of(len, h);
                if got != want {
                    wrong.push(format!("{name} R={r}: {got:?}, want {want:?}"));
                }
            }
        }
        eprintln!("m7_29r observed wrong rows: {wrong:?}");
        assert!(wrong.is_empty(), "behavior: {} cases give the wrong class: {wrong:?}", wrong.len());
    }
}
