//! LARIS integration: peer discovery, liveness probe, theme API, and the
//! lifecycle of a tray-spawned dictation daemon. An externally started
//! daemon is detected via alive() but never killed by the tray.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct LarisPeer {
    pub url: String,
    pub token: String,
}

/// peers.json is written by the laris daemon on boot: {"laris": {url, token}}.
pub fn read_peer(path: &Path) -> Option<LarisPeer> {
    let raw = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let laris = json.get("laris")?;
    Some(LarisPeer {
        url: laris.get("url")?.as_str()?.to_owned(),
        token: laris.get("token")?.as_str()?.to_owned(),
    })
}

/// TCP probe on the overlay port - cheap enough for the 2s poll tick.
pub fn alive(url: &str) -> bool {
    url.strip_prefix("http://")
        .and_then(|a| a.trim_end_matches('/').parse::<std::net::SocketAddr>().ok())
        .map(|addr| {
            std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
        })
        .unwrap_or(false)
}

/// Loopback-only HTTP agent: hard 2s timeout so a wedged daemon can never
/// freeze a menu handler, and no env proxy (HTTP_PROXY would route the
/// token-bearing loopback call off-box).
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .proxy(None)
        .build()
        .new_agent()
}

pub fn get_theme(peer: &LarisPeer) -> Result<serde_json::Value, String> {
    let mut res = agent()
        .get(format!("{}/theme?token={}", peer.url, peer.token))
        .call()
        .map_err(|e| e.to_string())?;
    res.body_mut().read_json().map_err(|e| e.to_string())
}

pub fn set_theme(
    peer: &LarisPeer,
    updates: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut res = agent()
        .post(format!("{}/theme?token={}", peer.url, peer.token))
        .send_json(&updates)
        .map_err(|e| e.to_string())?;
    res.body_mut().read_json().map_err(|e| e.to_string())
}

pub struct LarisDaemon {
    child: Option<Child>,
    laris_dir: PathBuf,
    engine_bin: Option<PathBuf>, // frozen engine (Windows); None = dev venv python
    log_path: Option<PathBuf>,   // where the daemon's stdout/stderr are captured
}

impl LarisDaemon {
    /// log_path captures the dev-venv daemon's stdout/stderr. On macOS the tray
    /// spawns `python -m laris` with no console, so without this its [Voice]
    /// prints and tracebacks (e.g. an intermittent mic-stream open failure) are
    /// lost - the log is the only diagnostics a field failure leaves behind.
    pub fn new(laris_dir: PathBuf, log_path: Option<PathBuf>) -> Self {
        Self {
            child: None,
            laris_dir,
            engine_bin: None,
            log_path,
        }
    }

    /// Run a self-contained frozen engine binary (Windows packaged build). The
    /// frozen exe is windowed (no console), so its stdout/stderr - including
    /// Python tracebacks and native-lib errors - are redirected to log_path,
    /// the only diagnostics a remote tester's crash leaves behind.
    pub fn with_engine(engine_bin: PathBuf, log_path: PathBuf) -> Self {
        Self {
            child: None,
            laris_dir: engine_bin.parent().map(Path::to_path_buf).unwrap_or_default(),
            engine_bin: Some(engine_bin),
            log_path: Some(log_path),
        }
    }

    pub fn owns_running(&mut self) -> bool {
        match self.child.as_mut().map(|c| c.try_wait()) {
            Some(Ok(None)) => true,
            _ => {
                self.child = None;
                false
            }
        }
    }

    pub fn spawn(&mut self) -> Result<(), String> {
        if self.owns_running() {
            return Ok(()); // don't orphan a live child by overwriting its handle
        }
        let child = match &self.engine_bin {
            Some(bin) => {
                let mut cmd = Command::new(bin);
                cmd.current_dir(&self.laris_dir);
                self.redirect_log(&mut cmd);
                cmd.spawn()
                    .map_err(|e| format!("{}: {e}", bin.display()))?
            }
            None => {
                let python = self.laris_dir.join(".venv/bin/python");
                let mut cmd = Command::new(&python);
                // -u: stdout goes to a file below, and Python block-buffers a
                // non-tty stdout - the [Voice] prints would sit in the buffer
                // instead of reaching the log this exists to produce.
                cmd.args(["-u", "-m", "laris"]).current_dir(&self.laris_dir);
                self.redirect_log(&mut cmd);
                cmd.spawn()
                    .map_err(|e| format!("{}: {e}", python.display()))?
            }
        };
        self.child = Some(child);
        Ok(())
    }

    /// Capture the child's stdout/stderr to log_path; on any failure just
    /// inherit (a missing log must never stop the daemon from starting).
    fn redirect_log(&self, cmd: &mut Command) {
        if let Some(log) = &self.log_path {
            if let Ok(f) = std::fs::File::create(log) {
                if let Ok(f2) = f.try_clone() {
                    cmd.stdout(Stdio::from(f)).stderr(Stdio::from(f2));
                }
            }
        }
    }

    /// SIGTERM first (laris tears down overlay + keyboard listener), SIGKILL
    /// after 3s if it hangs.
    pub fn stop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        #[cfg(unix)]
        unsafe {
            libc::kill(child.id() as i32, libc::SIGTERM);
        }
        for _ in 0..30 {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn read_peer_happy_path() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("peers.json");
        fs::write(
            &p,
            r#"{"laris": {"url": "http://127.0.0.1:8421", "token": "abc"}}"#,
        )
        .unwrap();
        let peer = read_peer(&p).unwrap();
        assert_eq!(peer.url, "http://127.0.0.1:8421");
        assert_eq!(peer.token, "abc");
    }

    #[cfg(unix)]
    #[test]
    fn daemon_spawn_owns_stop_lifecycle() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let bin_dir = tmp.path().join(".venv/bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let py = bin_dir.join("python");
        fs::write(&py, "#!/bin/sh\nsleep 60\n").unwrap();
        fs::set_permissions(&py, fs::Permissions::from_mode(0o755)).unwrap();

        let mut daemon = LarisDaemon::new(tmp.path().to_path_buf(), None);
        assert!(!daemon.owns_running());
        daemon.spawn().unwrap();
        assert!(daemon.owns_running());
        daemon.spawn().unwrap(); // idempotent - must not orphan the first child
        daemon.stop();
        assert!(!daemon.owns_running());

        let mut missing = LarisDaemon::new(tmp.path().join("nope"), None);
        assert!(missing.spawn().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn daemon_venv_branch_captures_stdout_to_log() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let bin_dir = tmp.path().join(".venv/bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let py = bin_dir.join("python");
        fs::write(&py, "#!/bin/sh\necho hello-from-laris\nsleep 60\n").unwrap();
        fs::set_permissions(&py, fs::Permissions::from_mode(0o755)).unwrap();
        let log = tmp.path().join("laris.log");

        let mut daemon = LarisDaemon::new(tmp.path().to_path_buf(), Some(log.clone()));
        daemon.spawn().unwrap();

        // the dev-venv daemon's stdout is redirected to the log (poll: echo races)
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut captured = false;
        while std::time::Instant::now() < deadline {
            if fs::read_to_string(&log).map(|s| s.contains("hello-from-laris")).unwrap_or(false) {
                captured = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(captured, "venv daemon stdout was not captured to the log");
        daemon.stop();
    }

    #[cfg(unix)]
    #[test]
    fn daemon_venv_branch_runs_python_unbuffered() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let bin_dir = tmp.path().join(".venv/bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let py = bin_dir.join("python");
        fs::write(&py, "#!/bin/sh\necho \"argv: $@\"\nsleep 60\n").unwrap();
        fs::set_permissions(&py, fs::Permissions::from_mode(0o755)).unwrap();
        let log = tmp.path().join("laris.log");

        let mut daemon = LarisDaemon::new(tmp.path().to_path_buf(), Some(log.clone()));
        daemon.spawn().unwrap();

        // stdout redirected to a file makes Python block-buffer it, so the
        // [Voice] prints sit in an 8KB buffer instead of reaching the log -
        // which is the only diagnostics a field failure leaves behind.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut argv = String::new();
        while std::time::Instant::now() < deadline {
            argv = fs::read_to_string(&log).unwrap_or_default();
            if argv.contains("argv:") {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        daemon.stop();
        assert!(
            argv.contains("-u"),
            "venv daemon must run python unbuffered, got: {argv:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn daemon_spawns_explicit_engine_binary() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let engine = tmp.path().join("laris-engine");
        fs::write(&engine, "#!/bin/sh\necho booted\nsleep 60\n").unwrap();
        fs::set_permissions(&engine, fs::Permissions::from_mode(0o755)).unwrap();
        let log = tmp.path().join("laris-engine.log");

        // when given an explicit engine binary, the daemon runs it directly
        // instead of `.venv/bin/python -m laris`, capturing its output to the log
        let mut daemon = LarisDaemon::with_engine(engine, log.clone());
        assert!(!daemon.owns_running());
        daemon.spawn().unwrap();
        assert!(daemon.owns_running());

        // the engine's stdout is redirected to the log (poll: the echo races the read)
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut captured = false;
        while std::time::Instant::now() < deadline {
            if fs::read_to_string(&log).map(|s| s.contains("booted")).unwrap_or(false) {
                captured = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(captured, "engine stdout was not captured to the log file");

        daemon.stop();
        assert!(!daemon.owns_running());
    }

    #[test]
    fn read_peer_missing_or_corrupt_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read_peer(&tmp.path().join("nope.json")).is_none());
        let p = tmp.path().join("bad.json");
        fs::write(&p, "{oops").unwrap();
        assert!(read_peer(&p).is_none());
        fs::write(&p, r#"{"laris": {"url": 1}}"#).unwrap();
        assert!(read_peer(&p).is_none());
    }

    #[test]
    fn alive_true_for_bound_listener_false_otherwise() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(alive(&format!("http://127.0.0.1:{port}")));
        drop(listener);
        // macOS may briefly accept into the dead listener's backlog - poll
        // until the close propagates instead of racing a single probe
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut down = false;
        while std::time::Instant::now() < deadline {
            if !alive(&format!("http://127.0.0.1:{port}")) {
                down = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(down, "port still accepting 2s after listener drop");
        assert!(!alive("not-a-url"));
    }
}
