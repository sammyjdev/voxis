
// ---- appended by scripts/windows/frozen-repro.sh to dictate.rs of the frozen SHA 889d48c ----
// Criterion 64 / review finding P5-6: F17 through the real `serve_http` over loopback
// (no seam: the function is the frozen one, and it reads VOXIS_FRONTEND on every request).
#[cfg(test)]
mod frozen_repro {
    use super::*;
    use std::io::Read;

    static ENV: Mutex<()> = Mutex::new(());

    /// One request through the frozen `serve_http`: (status line, body).
    fn get(target: &str) -> (String, Vec<u8>) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            serve_http(s, "tok");
        });
        let mut c = std::net::TcpStream::connect(addr).unwrap();
        write!(c, "GET {target} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut raw = Vec::new();
        c.read_to_end(&mut raw).unwrap();
        server.join().unwrap();
        let split = raw.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4).unwrap_or(raw.len());
        let head = String::from_utf8_lossy(&raw[..split]).into_owned();
        (head.lines().next().unwrap_or("").to_owned(), raw[split..].to_vec())
    }

    /// A frontend folder with overlay.js, beside a sentinel file that must never be served.
    fn frontend() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("frontend");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("overlay.js"), b"js").unwrap();
        let sentinel = tmp.path().join("sentinel.txt");
        std::fs::write(&sentinel, b"SECRET").unwrap();
        (tmp, root, sentinel)
    }

    #[test]
    fn control_a_file_inside_the_frontend_is_served() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let (_tmp, root, _) = frontend();
        std::env::set_var("VOXIS_FRONTEND", &root);

        let (status, body) = get("/static/overlay.js");

        assert!(status.contains("200"), "{status}");
        assert_eq!(body, b"js");
    }

    #[test]
    fn an_absolute_windows_path_under_static_is_not_served() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let (_tmp, root, sentinel) = frontend();
        std::env::set_var("VOXIS_FRONTEND", &root);

        let (status, body) = get(&format!("/static/{}", sentinel.display().to_string().replace('\\', "/")));

        assert_ne!(body, b"SECRET", "the sentinel outside the frontend was served: {status}");
    }

    #[test]
    fn a_junction_inside_the_frontend_does_not_lead_outside_it() {
        let _env = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let (tmp, root, _sentinel) = frontend();
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), b"SECRET").unwrap();
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J", &root.join("link").to_string_lossy(), &outside.to_string_lossy()])
            .output()
            .unwrap();
        assert!(made.status.success(), "mklink /J failed: {}", String::from_utf8_lossy(&made.stdout));
        std::env::set_var("VOXIS_FRONTEND", &root);

        let (status, body) = get("/static/link/secret.txt");

        assert_ne!(body, b"SECRET", "a file behind a junction was served: {status}");
    }
}
