//! First-run model download on Windows: the tray runs `meetrec models fetch`,
//! shows its progress, and keeps dictation off until the models are there.

use crate::laris::LarisDaemon;
use crate::AppState;
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelState {
    /// found or downloaded; also every platform that does not fetch
    #[default]
    Ready,
    Downloading(u8),
    Failed,
    /// dictate exited with "modelo não carregou": the files need a hash check
    LoadFailed,
}

/// What the tray does once `meetrec models fetch` exits.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    StartDictation,
    Toast(&'static str),
}

/// `progress <file> <bytes> <total>` -> percent of that file.
pub fn progress_pct(line: &str) -> Option<u8> {
    let mut f = line.split_whitespace();
    if f.next()? != "progress" {
        return None;
    }
    let _file = f.next()?;
    let got: u64 = f.next()?.parse().ok()?;
    let total: u64 = f.next()?.parse().ok()?;
    Some((got * 100 / total.max(1)).min(100) as u8)
}

pub fn download_tooltip(pct: u8) -> String {
    format!("VOXIS - baixando modelos de voz ({pct}%)")
}

/// Exit codes of `meetrec models fetch`: 0 ready, 2 hash mismatch, anything
/// else (1, a crash, no code) is treated as a network failure.
pub fn outcome(exit_code: Option<i32>) -> Outcome {
    match exit_code {
        Some(0) => Outcome::StartDictation,
        Some(2) => Outcome::Toast("Falha ao baixar modelos: hash diferente; tente de novo"),
        _ => Outcome::Toast("Falha ao baixar modelos: sem conexão; tente de novo"),
    }
}

/// The dictation row of the menu while the models are not ready.
pub fn dictation_label(state: ModelState) -> Option<String> {
    match state {
        ModelState::Ready => None,
        ModelState::Downloading(p) => Some(format!("Ditado indisponível: baixando modelos ({p}%)")),
        ModelState::Failed => Some("Ditado indisponível: falha ao baixar modelos".to_owned()),
        ModelState::LoadFailed => Some("Ditado indisponível: modelo não carregou".to_owned()),
    }
}

/// Starts the dictation daemon only once the models are there. Ok(true) when
/// it was spawned.
pub fn spawn_dictation(state: ModelState, daemon: &mut LarisDaemon) -> Result<bool, String> {
    if state != ModelState::Ready {
        return Ok(false);
    }
    daemon.spawn().map(|_| true)
}

/// Runs `meetrec models fetch`, feeding its progress into the tray state, then
/// starts dictation or shows why it could not.
pub fn fetch_models(app: &AppHandle, bin: &str) {
    fetch_models_with(app, bin, &["models", "fetch"]);
}

/// `fetch_models` with the `meetrec` arguments given (`--verify` after a
/// model that would not load).
pub fn fetch_models_with(app: &AppHandle, bin: &str, args: &[&str]) {
    use std::io::BufRead;
    let state = app.state::<AppState>();
    let (stdout, stderr) = match state.fetch.start(bin, args) {
        Ok(pipes) => pipes,
        Err(e) => {
            crate::tray_log(&format!("models fetch: {bin}: {e}"));
            *state.models.lock().unwrap() = ModelState::Failed;
            state.menu_dirty.store(true, Ordering::SeqCst);
            crate::notify(app, "Falha ao baixar modelos: sem conexão; tente de novo");
            return;
        }
    };
    let err_log = std::thread::spawn(move || {
        if let Some(e) = stderr {
            for line in std::io::BufReader::new(e).lines().map_while(Result::ok) {
                crate::tray_log(&format!("models fetch: {line}"));
            }
        }
    });
    if let Some(out) = stdout {
        let mut last = None;
        for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            if let Some(p) = progress_pct(&line) {
                if last != Some(p) {
                    last = Some(p);
                    *state.models.lock().unwrap() = ModelState::Downloading(p);
                }
            } else {
                crate::tray_log(&format!("models fetch: {line}"));
            }
        }
    }
    let code = state.fetch.wait();
    let _ = err_log.join();
    if state.fetch.stopped() {
        return; // the tray is quitting
    }
    match outcome(code) {
        Outcome::StartDictation => {
            *state.models.lock().unwrap() = ModelState::Ready;
            if !crate::external_daemon_alive() {
                if let Err(e) = spawn_dictation(ModelState::Ready, &mut state.daemon.lock().unwrap()) {
                    crate::notify(app, &format!("Falha ao iniciar ditado: {e}"));
                }
            }
        }
        Outcome::Toast(msg) => {
            *state.models.lock().unwrap() = ModelState::Failed;
            crate::notify(app, msg);
        }
    }
    state.menu_dirty.store(true, Ordering::SeqCst);
}

/// The `meetrec` arguments the retry row runs.
pub fn fetch_args(state: ModelState) -> &'static [&'static str] {
    match state {
        // a model that would not load is hashed against its pin and
        // replaced only by a verified download
        ModelState::LoadFailed => &["models", "fetch", "--verify"],
        _ => &["models", "fetch"],
    }
}

/// The menu row that downloads the models again, shown only after a failure.
pub fn retry_label(state: ModelState) -> Option<&'static str> {
    match state {
        ModelState::Failed => Some("Baixar modelos de novo"),
        ModelState::LoadFailed => Some("Verificar modelos"),
        _ => None,
    }
}

/// The menu's "Baixar modelos de novo": fetches again unless one is running.
pub fn retry(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut models = state.models.lock().unwrap();
    if !matches!(*models, ModelState::Failed | ModelState::LoadFailed) || state.fetch.running() {
        return;
    }
    let args = fetch_args(*models);
    *models = ModelState::Downloading(0);
    drop(models);
    let (app, bin) = (app.clone(), state.config.laris_bin.clone());
    std::thread::spawn(move || fetch_models_with(&app, &bin, args));
}

/// What quitting the tray stops: a model download still running, and the
/// dictation daemon.
pub fn quit(state: &AppState) {
    state.fetch.stop();
    state.daemon.lock().unwrap().stop();
}

/// The fetch child, held so quitting the tray can stop it: a download left
/// running would race the next launch's fetch on the same `.part` file.
#[derive(Default)]
pub struct FetchRun {
    child: Mutex<Option<Child>>,
    stopped: AtomicBool,
}

impl FetchRun {
    /// Spawns `program args`, without a console window, and hands back its
    /// output. Refused once the tray has stopped it: quitting wins a race
    /// with a fetch that was about to start.
    pub fn start(&self, program: &str, args: &[&str]) -> std::io::Result<(Option<ChildStdout>, Option<ChildStderr>)> {
        let mut slot = self.child.lock().unwrap();
        if self.stopped() {
            return Err(std::io::Error::other("the tray is quitting"));
        }
        let mut cmd = Command::new(program);
        cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = cmd.spawn()?;
        let pipes = (child.stdout.take(), child.stderr.take());
        *slot = Some(child);
        Ok(pipes)
    }

    pub fn running(&self) -> bool {
        self.child.lock().unwrap().is_some()
    }

    /// Kills the child; its `wait` then returns at once.
    pub fn stop(&self) {
        let mut slot = self.child.lock().unwrap();
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(c) = slot.as_mut() {
            let _ = c.kill();
        }
    }

    pub fn stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// Waits for the child to exit: its exit code, None when killed or never started.
    pub fn wait(&self) -> Option<i32> {
        let child = self.child.lock().unwrap().take();
        child.and_then(|mut c| c.wait().ok()?.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_model_download_drives_the_tooltip_and_its_outcome() {
        let pct = progress_pct("progress ggml-large-v3-turbo.bin 400 1000");

        assert_eq!(pct, Some(40));
        assert_eq!(download_tooltip(40), "VOXIS - baixando modelos de voz (40%)");
        assert_eq!(progress_pct("ready C:\\x"), None);
        assert_eq!(outcome(Some(0)), Outcome::StartDictation);
        assert_eq!(outcome(Some(1)), Outcome::Toast("Falha ao baixar modelos: sem conexão; tente de novo"));
        assert_eq!(outcome(Some(2)), Outcome::Toast("Falha ao baixar modelos: hash diferente; tente de novo"));
    }

    #[test]
    fn a_model_load_failure_offers_verify_in_the_retry_row() {
        assert_eq!(retry_label(ModelState::LoadFailed), Some("Verificar modelos"));
        assert_eq!(dictation_label(ModelState::LoadFailed), Some("Ditado indisponível: modelo não carregou".to_owned()));
        assert_eq!(fetch_args(ModelState::LoadFailed), ["models", "fetch", "--verify"]);
        assert_eq!(fetch_args(ModelState::Failed), ["models", "fetch"], "a failed download keeps the plain fetch");
    }

    #[test]
    fn only_a_failed_download_offers_to_download_again() {
        let failed = retry_label(ModelState::Failed);

        assert_eq!(failed, Some("Baixar modelos de novo"));
        assert_eq!(retry_label(ModelState::Downloading(40)), None);
        assert_eq!(retry_label(ModelState::Ready), None);
    }

    /// A process that runs for about 30 s unless killed.
    fn long_running() -> (&'static str, Vec<&'static str>) {
        if cfg!(windows) {
            ("ping", vec!["-n", "30", "127.0.0.1"])
        } else {
            ("sleep", vec!["30"])
        }
    }

    #[test]
    fn quitting_the_tray_stops_its_model_download() {
        let tmp = tempfile::tempdir().unwrap();
        let state = AppState {
            config: Default::default(),
            daemon: Mutex::new(LarisDaemon::new(tmp.path().to_path_buf(), None)),
            processing: AtomicBool::new(false),
            menu_dirty: AtomicBool::new(false),
            models: Mutex::new(ModelState::Downloading(10)),
            fetch: FetchRun::default(),
        };
        let (program, args) = long_running();
        let _pipes = state.fetch.start(program, &args).unwrap();
        let started = std::time::Instant::now();

        quit(&state);
        let code = state.fetch.wait();

        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        assert_ne!(code, Some(0));
    }

    #[test]
    fn a_download_asked_for_after_quit_never_starts() {
        let run = FetchRun::default();
        run.stop();
        let (program, args) = long_running();

        let started = run.start(program, &args);

        assert!(started.is_err());
        assert!(!run.running());
    }

    #[test]
    fn a_download_refused_after_quit_runs_no_process() {
        let tmp = tempfile::tempdir().unwrap();
        let marker = tmp.path().join("ran.txt");
        // a script that leaves a file behind when it runs
        let script = if cfg!(windows) {
            let s = tmp.path().join("probe.cmd");
            std::fs::write(&s, format!("@echo ran> \"{}\"\r\n", marker.display())).unwrap();
            s
        } else {
            let s = tmp.path().join("probe.sh");
            std::fs::write(&s, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&s, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            s
        };
        let script = script.to_string_lossy().into_owned();
        let control = FetchRun::default();
        let _ = control.start(&script, &[]).unwrap();
        control.wait();
        assert!(marker.exists(), "the probe must leave its file when it runs");
        std::fs::remove_file(&marker).unwrap();
        let run = FetchRun::default();
        run.stop();

        let _ = run.start(&script, &[]);
        std::thread::sleep(std::time::Duration::from_secs(2));

        assert!(!marker.exists());
    }

    #[test]
    fn quitting_stops_a_download_still_running() {
        let run = FetchRun::default();
        #[cfg(windows)]
        let (program, args) = ("ping", ["-n", "30", "127.0.0.1"]);
        #[cfg(not(windows))]
        let (program, args) = ("sleep", ["30", "", ""]);
        let args: Vec<&str> = args.into_iter().filter(|a| !a.is_empty()).collect();
        let _pipes = run.start(program, &args).unwrap();
        let started = std::time::Instant::now();

        run.stop();
        let code = run.wait();

        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        assert_ne!(code, Some(0));
        assert!(!run.running());
    }

    #[test]
    fn after_a_failed_download_the_menu_says_the_models_failed() {
        let label = dictation_label(ModelState::Failed);

        assert_eq!(label.as_deref(), Some("Ditado indisponível: falha ao baixar modelos"));
    }

    #[cfg(windows)]
    #[test]
    fn while_models_download_dictation_stays_off_and_the_menu_says_why() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("fake-meetrec.cmd");
        std::fs::write(&bin, "@echo started\r\n").unwrap();
        let mut daemon = LarisDaemon::new(tmp.path().to_path_buf(), None).with_bin(&bin.to_string_lossy());

        let spawned = spawn_dictation(ModelState::Downloading(40), &mut daemon).unwrap();

        assert!(!spawned);
        assert!(!daemon.owns_running());
        assert_eq!(
            dictation_label(ModelState::Downloading(40)).as_deref(),
            Some("Ditado indisponível: baixando modelos (40%)")
        );
        assert_eq!(dictation_label(ModelState::Ready), None);
        assert!(spawn_dictation(ModelState::Ready, &mut daemon).unwrap());
    }
}
