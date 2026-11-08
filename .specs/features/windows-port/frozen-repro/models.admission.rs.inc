
// ---- appended by scripts/windows/frozen-repro.sh to models.rs of the frozen SHA 889d48c (certificates round) ----
// Criterion 49 has no frozen entry point (`--verify` does not exist at 889d48c), so nothing here is a red
// of the verify rule. This CHARACTERIZES the old rule the verify step was added to cover, and it passes
// on the frozen code: the name-based "ready" admits a model file whatever its bytes are. MODEL and VAD
// are corrupted on disk, a loopback endpoint that would cut any download is offered as the pin's URL,
// and the frozen `fetch_with` answers `ready`, exit 0, without hashing a file and without one request.
#[cfg(test)]
mod frozen_repro_models_admission {
    use super::*;
    use std::io::{BufRead, BufReader, Write as _};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn counting_cut_server() -> (String, Arc<AtomicUsize>) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let h = hits.clone();
        std::thread::spawn(move || {
            for s in l.incoming() {
                let mut s = s.unwrap();
                h.fetch_add(1, Ordering::SeqCst);
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut line = String::new();
                while r.read_line(&mut line).unwrap() > 2 {
                    line.clear();
                }
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\nConnection: close\r\n\r\n");
                let _ = s.write_all(&[7u8; 400]);
            }
        });
        (base, hits)
    }

    #[test]
    fn characterization_the_name_based_ready_admits_corrupt_models_and_never_asks_the_endpoint() {
        let (base, hits) = counting_cut_server();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(MODEL), b"corrupted on disk").unwrap();
        std::fs::write(dir.path().join(VAD), b"also corrupted").unwrap();
        let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
        let pins = vec![
            Pin { name: MODEL, url: leak(format!("{base}/{MODEL}")), sha256: "0000000000000000000000000000000000000000000000000000000000000000" },
            Pin { name: VAD, url: leak(format!("{base}/{VAD}")), sha256: "1111111111111111111111111111111111111111111111111111111111111111" },
        ];
        let (mut out, mut err) = (Vec::new(), Vec::new());

        let code = fetch_with(&[dir.path().to_path_buf()], dir.path(), &pins, std::time::Duration::from_secs(5), &mut out, &mut err);

        assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
        assert!(String::from_utf8_lossy(&out).contains("ready"), "{}", String::from_utf8_lossy(&out));
        assert_eq!(std::fs::read(dir.path().join(MODEL)).unwrap(), b"corrupted on disk");
        assert_eq!(std::fs::read(dir.path().join(VAD)).unwrap(), b"also corrupted");
        assert_eq!(hits.load(Ordering::SeqCst), 0, "the endpoint was never asked");
    }
}
