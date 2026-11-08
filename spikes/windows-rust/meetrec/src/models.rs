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

/// `fetch_target` fed from the environment variables `meetrec models fetch`
/// reads; `env` looks one up by name.
pub fn fetch_target_from_env(env: &dyn Fn(&str) -> Option<PathBuf>, home: &Path) -> PathBuf {
    fetch_target(env("VOXIS_MODELS"), env("LOCALAPPDATA"), home)
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

/// How long a download may go without receiving a byte before it fails.
pub const STALL: std::time::Duration = std::time::Duration::from_secs(60);

/// `meetrec models fetch`: models already found anywhere are left alone;
/// otherwise each pin is downloaded into `target`. Returns the exit code:
/// 0 ready, 1 network or HTTP error (a stall included), 2 hash mismatch.
pub fn fetch(places: &[PathBuf], target: &Path, pins: &[Pin], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    fetch_with(places, target, pins, STALL, out, err)
}

/// Opens the file a download writes into.
pub type Opener = dyn Fn(&Path) -> std::io::Result<Box<dyn Write>>;

fn create_file(path: &Path) -> std::io::Result<Box<dyn Write>> {
    Ok(Box::new(std::fs::File::create(path)?))
}

/// `meetrec models fetch --verify`: each present file is hashed against its pin.
pub fn fetch_verify(places: &[PathBuf], target: &Path, pins: &[Pin], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    fetch_verify_with(places, target, pins, STALL, &create_file, out, err)
}

/// `fetch_verify` with the stall limit and the file opener passed in.
///
/// Checks the folder the app loads from (the first place holding both
/// files, else `target`). A file that matches its pin stays; one that
/// differs or is missing is downloaded through `<name>.part`, verified, and
/// only then put under its name. A download cut short, or a full disk,
/// leaves the old file as it was, deletes only its own `.part`, and exits 1;
/// a downloaded file whose hash differs exits 2.
pub fn fetch_verify_with(places: &[PathBuf], target: &Path, pins: &[Pin], stall: std::time::Duration, open: &Opener, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let dir = find(places).unwrap_or_else(|_| target.to_path_buf());
    if let Err(e) = std::fs::create_dir_all(&dir) {
        let _ = writeln!(err, "{}: {e}", dir.display());
        return 1;
    }
    for pin in pins {
        let path = dir.join(pin.name);
        let started = std::time::Instant::now();
        match sha256_of(&path) {
            Ok(digest) if digest == pin.sha256 => {
                let _ = writeln!(out, "verified {} in {:.1} s", pin.name, started.elapsed().as_secs_f64());
                continue;
            }
            Ok(digest) => {
                let _ = writeln!(out, "mismatch {}: sha256 {digest}, expected {}", pin.name, pin.sha256);
            }
            Err(_) => {
                let _ = writeln!(out, "missing {}", pin.name);
            }
        }
        let code = download(pin, &dir, stall, open, out, err);
        if code != 0 {
            let _ = std::fs::remove_file(dir.join(format!("{}.part", pin.name)));
            return code;
        }
    }
    let _ = writeln!(out, "ready {}", dir.display());
    0
}

fn sha256_of(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// `fetch` with the stall limit passed in.
pub fn fetch_with(places: &[PathBuf], target: &Path, pins: &[Pin], stall: std::time::Duration, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if let Ok(dir) = find(places) {
        let _ = writeln!(out, "ready {}", dir.display());
        return 0;
    }
    if let Err(e) = std::fs::create_dir_all(target) {
        let _ = writeln!(err, "{}: {e}", target.display());
        return 1;
    }
    for pin in pins {
        let code = download(pin, target, stall, &create_file, out, err);
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
fn download(pin: &Pin, target: &Path, stall: std::time::Duration, open: &Opener, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let dest = target.join(pin.name);
    let part = target.join(format!("{}.part", pin.name));
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(stall))
        .timeout_per_read(Some(stall))
        .build()
        .into();
    let mut resp = match agent.get(pin.url).call() {
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
    let mut file = match open(&part) {
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
    fn models_fetch_reads_the_install_folder_from_the_environment() {
        let home = Path::new(r"C:\Users\u");
        let installed = |k: &str| (k == "LOCALAPPDATA").then(|| PathBuf::from(r"C:\Users\u\AppData\Local"));
        let overridden = |k: &str| match k {
            "VOXIS_MODELS" => Some(PathBuf::from(r"D:\m")),
            "LOCALAPPDATA" => Some(PathBuf::from(r"C:\Users\u\AppData\Local")),
            _ => None,
        };

        let from_install = fetch_target_from_env(&installed, home);
        let from_override = fetch_target_from_env(&overridden, home);

        assert_eq!(from_install, PathBuf::from(r"C:\Users\u\AppData\Local\voxis\models"));
        assert_eq!(from_override, PathBuf::from(r"D:\m"));
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
    fn a_download_that_stops_sending_gives_up_and_exits_1() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            while r.read_line(&mut line).unwrap() > 2 {
                line.clear();
            }
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n");
            let _ = s.write_all(&[7u8; 100]);
            std::thread::sleep(std::time::Duration::from_secs(30)); // stalls, connection open
        });
        let target = tempfile::tempdir().unwrap();
        let path = target.path().to_path_buf();
        let pins = pins_at(&base, &[7u8; 1000], b"x");
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            let code = fetch_with(&[], &path, &pins, std::time::Duration::from_secs(1), &mut Vec::new(), &mut Vec::new());
            let _ = tx.send(code);
        });

        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(10)), Ok(1));
        assert!(!target.path().join(MODEL).exists());
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

    /// A model folder holding `model` and `vad` bytes.
    fn holding(model: &[u8], vad: &[u8]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(MODEL), model).unwrap();
        std::fs::write(d.path().join(VAD), vad).unwrap();
        d
    }

    #[test]
    fn verify_replaces_a_file_whose_hash_differs_after_a_verified_download() {
        let good = b"the pinned model bytes".to_vec();
        let (base, hits) = serve(good.clone(), good.len());
        let dir = holding(b"corrupted on disk", b"vad");
        let mut out = Vec::new();

        let code = fetch_verify(&[dir.path().to_path_buf()], dir.path(), &pins_at(&base, &good, b"vad"), &mut out, &mut Vec::new());

        assert_eq!(code, 0, "{}", String::from_utf8_lossy(&out));
        assert_eq!(std::fs::read(dir.path().join(MODEL)).unwrap(), good);
        assert_eq!(std::fs::read(dir.path().join(VAD)).unwrap(), b"vad");
        assert_eq!(hits.load(Ordering::SeqCst), 1, "only the differing file is downloaded");
        assert!(!dir.path().join(format!("{MODEL}.part")).exists());
    }

    #[test]
    fn verify_leaves_files_that_match_their_pin() {
        let (base, hits) = serve(b"unused".to_vec(), 6);
        let dir = holding(b"model", b"vad");

        let code = fetch_verify(&[dir.path().to_path_buf()], dir.path(), &pins_at(&base, b"model", b"vad"), &mut Vec::new(), &mut Vec::new());

        assert_eq!(code, 0);
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        assert_eq!(std::fs::read(dir.path().join(MODEL)).unwrap(), b"model");
    }

    #[test]
    fn a_cut_verify_download_keeps_the_old_file_and_removes_its_part() {
        let full = vec![7u8; 1000];
        let (base, _) = serve(vec![7u8; 400], 1000); // promises 1000, sends 400
        let dir = holding(b"corrupted on disk", b"vad");

        let code = fetch_verify(&[dir.path().to_path_buf()], dir.path(), &pins_at(&base, &full, b"vad"), &mut Vec::new(), &mut Vec::new());

        assert_eq!(code, 1);
        assert_eq!(std::fs::read(dir.path().join(MODEL)).unwrap(), b"corrupted on disk", "the old file is untouched");
        assert!(!dir.path().join(format!("{MODEL}.part")).exists(), "its own .part is deleted");
    }

    #[test]
    fn a_cut_download_with_no_old_file_leaves_nothing_under_the_final_name() {
        let full = vec![7u8; 1000];
        let (base, _) = serve(vec![7u8; 400], 1000);
        let dir = tempfile::tempdir().unwrap();

        let code = fetch_verify(&[dir.path().to_path_buf()], dir.path(), &pins_at(&base, &full, b"vad"), &mut Vec::new(), &mut Vec::new());

        assert_eq!(code, 1);
        assert!(!dir.path().join(MODEL).exists());
        assert!(!dir.path().join(format!("{MODEL}.part")).exists());
    }

    /// A file that takes `cap` bytes, then fails like a full disk.
    struct Full(std::fs::File, usize);

    impl Write for Full {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if buf.len() > self.1 {
                return Err(std::io::Error::other("There is not enough space on the disk"));
            }
            self.1 -= buf.len();
            self.0.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.flush()
        }
    }

    #[test]
    fn a_full_disk_during_verify_keeps_the_old_file() {
        let good = vec![9u8; 4000];
        let (base, _) = serve(good.clone(), good.len());
        let dir = holding(b"corrupted on disk", b"vad");
        let full = |p: &Path| -> std::io::Result<Box<dyn Write>> { Ok(Box::new(Full(std::fs::File::create(p)?, 100))) };
        let mut err = Vec::new();

        let code = fetch_verify_with(&[dir.path().to_path_buf()], dir.path(), &pins_at(&base, &good, b"vad"), STALL, &full, &mut Vec::new(), &mut err);

        assert_eq!(code, 1, "{}", String::from_utf8_lossy(&err));
        assert_eq!(std::fs::read(dir.path().join(MODEL)).unwrap(), b"corrupted on disk");
        assert!(!dir.path().join(format!("{MODEL}.part")).exists());
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
