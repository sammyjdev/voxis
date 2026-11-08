
// ---- appended by scripts/windows/frozen-repro.sh to models.rs of the frozen SHA 889d48c (round 2) ----
// Criterion 49 has no frozen entry point: `models fetch --verify` does not exist at 889d48c. This is
// a CHARACTERIZATION of the frozen plain fetch (it passes there): a download cut short leaves only
// `<name>.part`, never the final name. It is not a criterion 64 red and is listed apart from them.
#[cfg(test)]
mod frozen_repro_models {
    use super::*;
    use std::io::{BufRead, BufReader, Write as _};

    /// A loopback server that promises 1000 bytes, sends 400 and closes.
    fn cut_server() -> String {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        std::thread::spawn(move || {
            for s in l.incoming() {
                let mut s = s.unwrap();
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut line = String::new();
                while r.read_line(&mut line).unwrap() > 2 {
                    line.clear();
                }
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\nConnection: close\r\n\r\n");
                let _ = s.write_all(&[7u8; 400]);
            }
        });
        base
    }

    #[test]
    fn characterization_a_cut_plain_download_leaves_only_the_part_file() {
        let base = cut_server();
        let target = tempfile::tempdir().unwrap();
        let pin = Pin {
            name: "ggml-test.bin",
            url: Box::leak(format!("{base}/ggml-test.bin").into_boxed_str()),
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        };

        let code = fetch_with(&[], target.path(), &[pin], std::time::Duration::from_secs(5), &mut Vec::new(), &mut Vec::new());

        assert_eq!(code, 1);
        assert!(!target.path().join("ggml-test.bin").exists(), "a cut download never sits under the final name");
        assert!(target.path().join("ggml-test.bin.part").exists());
    }
}
