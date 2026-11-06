//! The whisper model and the Silero VAD: where meetrec looks for them, and
//! `meetrec models fetch`, which downloads them on a fresh install.

use std::io::Write;
use std::path::{Path, PathBuf};

pub const MODEL: &str = "ggml-large-v3-turbo.bin";
pub const VAD: &str = "ggml-silero-v5.1.2.bin";

/// A file pinned to one commit of its Hugging Face repo and to its SHA-256.
pub struct Pin {
    pub name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
}

pub const PINS: [Pin; 2] = [
    Pin {
        name: MODEL,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo.bin",
        sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
    },
    Pin {
        name: VAD,
        url: "https://huggingface.co/ggml-org/whisper-vad/resolve/9ffd54a1e1ee413ddf265af9913beaf518d1639b/ggml-silero-v5.1.2.bin",
        sha256: "29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf",
    },
];

/// Where models are looked for, in order: VOXIS_MODELS, the installed app's
/// %LOCALAPPDATA%\voxis\models, then the benchmark cache of a dev machine.
pub fn places(voxis_models: Option<PathBuf>, local_app_data: Option<PathBuf>, home: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    v.extend(voxis_models);
    v.extend(local_app_data.map(|d| d.join("voxis").join("models")));
    v.push(home.join(".cache").join("voxis-bench").join("models"));
    v
}

/// Where `meetrec models fetch` saves: VOXIS_MODELS when set, else the
/// installed app's %LOCALAPPDATA%\voxis\models.
pub fn fetch_target(voxis_models: Option<PathBuf>, local_app_data: Option<PathBuf>, home: &Path) -> PathBuf {
    voxis_models
        .or_else(|| local_app_data.map(|d| d.join("voxis").join("models")))
        .unwrap_or_else(|| home.join(".voxis").join("models"))
}

/// The first place that holds both files.
pub fn find(places: &[PathBuf]) -> Result<PathBuf, String> {
    places
        .iter()
        .find(|d| d.join(MODEL).is_file() && d.join(VAD).is_file())
        .cloned()
        .ok_or_else(|| {
            let looked: Vec<String> = places.iter().map(|p| p.display().to_string()).collect();
            format!("models missing ({MODEL}, {VAD}); looked in {}", looked.join(", "))
        })
}

/// `meetrec models fetch`: models already found anywhere are left alone;
/// otherwise each pin is downloaded into `target`. Returns the exit code:
/// 0 ready, 1 network or HTTP error, 2 hash mismatch.
pub fn fetch(places: &[PathBuf], target: &Path, pins: &[Pin], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if let Ok(dir) = find(places) {
        let _ = writeln!(out, "ready {}", dir.display());
        return 0;
    }
    if let Err(e) = std::fs::create_dir_all(target) {
        let _ = writeln!(err, "{}: {e}", target.display());
        return 1;
    }
    for pin in pins {
        let code = download(pin, target, out, err);
        if code != 0 {
            return code;
        }
    }
    let _ = writeln!(out, "ready {}", target.display());
    0
}

/// One pin into `<target>\<name>`, through `<name>.part`: a download cut
/// short never sits under the final name, and neither does a file whose
/// hash is not the pinned one.
fn download(pin: &Pin, target: &Path, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let dest = target.join(pin.name);
    let part = target.join(format!("{}.part", pin.name));
    let mut resp = match ureq::get(pin.url).call() {
        Ok(r) => r,
        Err(e) => {
            let _ = writeln!(err, "{}: {e}", pin.url);
            return 1;
        }
    };
    let total: Option<u64> = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok()?.parse().ok());
    let mut file = match std::fs::File::create(&part) {
        Ok(f) => f,
        Err(e) => {
            let _ = writeln!(err, "{}: {e}", part.display());
            return 1;
        }
    };
    let mut reader = resp.body_mut().as_reader();
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let (mut got, mut shown) = (0u64, u64::MAX);
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                let _ = writeln!(err, "{}: {e} after {got} bytes", pin.url);
                return 1;
            }
        };
        if let Err(e) = file.write_all(&buf[..n]) {
            let _ = writeln!(err, "{}: {e}", part.display());
            return 1;
        }
        hash.update(&buf[..n]);
        got += n as u64;
        // a line per percent at most: the tray turns these into the tooltip
        let pct = total.map(|t| got * 100 / t.max(1)).unwrap_or(0);
        if pct != shown {
            shown = pct;
            let _ = writeln!(out, "progress {} {got} {}", pin.name, total.unwrap_or(0));
        }
    }
    drop(file);
    if total.is_some_and(|t| got < t) {
        let _ = writeln!(err, "{}: connection closed after {got} of {} bytes", pin.url, total.unwrap_or(0));
        return 1;
    }
    let digest: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if digest != pin.sha256 {
        let _ = std::fs::remove_file(&part);
        let _ = writeln!(err, "{}: sha256 {digest}, expected {}", pin.name, pin.sha256);
        return 2;
    }
    if let Err(e) = std::fs::rename(&part, &dest) {
        let _ = writeln!(err, "{}: {e}", dest.display());
        return 1;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
    }

    /// A server that answers every GET with `body`, declaring `declared` bytes
    /// of Content-Length, and counts the requests it got.
    fn serve(body: Vec<u8>, declared: usize) -> (String, Arc<AtomicUsize>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
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
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {declared}\r\nConnection: close\r\n\r\n");
                let _ = s.write_all(&body);
                // dropping the stream closes it, short or not
            }
        });
        (base, hits)
    }

    fn pins_at(base: &str, a: &[u8], b: &[u8]) -> Vec<Pin> {
        let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
        vec![
            Pin { name: MODEL, url: leak(format!("{base}/{MODEL}")), sha256: leak(hex(a)) },
            Pin { name: VAD, url: leak(format!("{base}/{VAD}")), sha256: leak(hex(b)) },
        ]
    }

    #[test]
    fn the_shipped_pins_name_the_reviewed_commits_and_hashes() {
        let pins: Vec<(&str, &str, &str)> = PINS.iter().map(|p| (p.name, p.url, p.sha256)).collect();

        assert_eq!(
            pins,
            vec![
                (
                    "ggml-large-v3-turbo.bin",
                    "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo.bin",
                    "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
                ),
                (
                    "ggml-silero-v5.1.2.bin",
                    "https://huggingface.co/ggml-org/whisper-vad/resolve/9ffd54a1e1ee413ddf265af9913beaf518d1639b/ggml-silero-v5.1.2.bin",
                    "29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf",
                ),
            ]
        );
    }

    #[test]
    fn a_fetch_on_an_installed_machine_saves_into_local_app_data() {
        let home = Path::new(r"C:\Users\u");

        let installed = fetch_target(None, Some(PathBuf::from(r"C:\Users\u\AppData\Local")), home);
        let overridden = fetch_target(Some(PathBuf::from(r"D:\m")), Some(PathBuf::from(r"C:\Users\u\AppData\Local")), home);

        assert_eq!(installed, PathBuf::from(r"C:\Users\u\AppData\Local\voxis\models"));
        assert_eq!(overridden, PathBuf::from(r"D:\m"));
    }

    #[test]
    fn models_are_found_in_the_first_place_that_holds_both_files() {
        let root = tempfile::tempdir().unwrap();
        let (env, local, home) = (root.path().join("env"), root.path().join("local"), root.path().join("home"));
        let bench = home.join(".cache").join("voxis-bench").join("models");
        let installed = local.join("voxis").join("models");
        let fill = |d: &Path, names: &[&str]| {
            std::fs::create_dir_all(d).unwrap();
            for n in names {
                std::fs::write(d.join(n), b"x").unwrap();
            }
        };
        let order = places(Some(env.clone()), Some(local.clone()), &home);
        assert_eq!(order, vec![env.clone(), installed.clone(), bench.clone()]);

        // none holds both files: the error names every place
        let e = find(&order).unwrap_err();
        for p in &order {
            assert!(e.contains(&p.display().to_string()), "{e}");
        }
        // only the bench cache holds both
        fill(&bench, &[MODEL, VAD]);
        assert_eq!(find(&order).unwrap(), bench);
        // the installed folder with one file is skipped
        fill(&installed, &[MODEL]);
        assert_eq!(find(&order).unwrap(), bench);
        // once complete, the installed folder wins over the bench cache
        fill(&installed, &[VAD]);
        assert_eq!(find(&order).unwrap(), installed);
        // VOXIS_MODELS wins over both
        fill(&env, &[MODEL, VAD]);
        assert_eq!(find(&order).unwrap(), env);
    }

    #[test]
    fn a_fetch_whose_hashes_match_saves_both_models() {
        let body = b"same bytes for both files".to_vec();
        let (base, _) = serve(body.clone(), body.len());
        let target = tempfile::tempdir().unwrap();
        let (mut out, mut err) = (Vec::new(), Vec::new());

        let code = fetch(&[], target.path(), &pins_at(&base, &body, &body), &mut out, &mut err);

        assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
        assert_eq!(std::fs::read(target.path().join(MODEL)).unwrap(), body);
        assert_eq!(std::fs::read(target.path().join(VAD)).unwrap(), body);
        assert!(!target.path().join(format!("{MODEL}.part")).exists());
        assert!(String::from_utf8_lossy(&out).contains(&format!("ready {}", target.path().display())));
    }

    #[test]
    fn a_model_whose_hash_differs_is_deleted_and_the_fetch_exits_2() {
        let body = b"tampered".to_vec();
        let (base, _) = serve(body.clone(), body.len());
        let target = tempfile::tempdir().unwrap();
        let pins = pins_at(&base, b"what the pin expects", &body);

        let code = fetch(&[], target.path(), &pins, &mut Vec::new(), &mut Vec::new());

        assert_eq!(code, 2);
        assert!(!target.path().join(MODEL).exists());
        assert!(!target.path().join(format!("{MODEL}.part")).exists());
    }

    #[test]
    fn an_interrupted_download_leaves_only_the_part_file() {
        let body = vec![7u8; 400];
        let (base, _) = serve(body.clone(), 1000); // promises 1000, sends 400
        let target = tempfile::tempdir().unwrap();
        let pins = pins_at(&base, &vec![7u8; 1000], &body);

        let code = fetch(&[], target.path(), &pins, &mut Vec::new(), &mut Vec::new());

        assert_eq!(code, 1);
        assert!(!target.path().join(MODEL).exists());
        assert!(target.path().join(format!("{MODEL}.part")).exists());
    }

    #[test]
    fn an_unreachable_model_server_exits_1() {
        // bind and drop: nothing listens on this port any more
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let base = format!("http://127.0.0.1:{port}");
        let target = tempfile::tempdir().unwrap();
        let mut err = Vec::new();

        let code = fetch(&[], target.path(), &pins_at(&base, b"a", b"b"), &mut Vec::new(), &mut err);

        assert_eq!(code, 1);
        assert!(String::from_utf8_lossy(&err).contains(&format!("{base}/{MODEL}")));
    }

    #[test]
    fn fetching_models_that_are_already_there_downloads_nothing() {
        let body = b"model".to_vec();
        let (base, hits) = serve(body.clone(), body.len());
        let have = tempfile::tempdir().unwrap();
        for n in [MODEL, VAD] {
            std::fs::write(have.path().join(n), &body).unwrap();
        }
        let target = tempfile::tempdir().unwrap();
        let mut out = Vec::new();

        let code = fetch(&[have.path().to_path_buf()], target.path(), &pins_at(&base, &body, &body), &mut out, &mut Vec::new());

        assert_eq!(code, 0);
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        assert!(String::from_utf8_lossy(&out).contains(&format!("ready {}", have.path().display())));
    }
}
