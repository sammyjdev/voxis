mod autostart;
mod commands;
mod laris;
mod miris;
mod models;
mod settings;
#[cfg(windows)]
mod toast;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Wry};
#[cfg(not(windows))]
use tauri_plugin_notification::NotificationExt;

const ARCHETYPES: [&str; 3] = ["waveform", "reactor", "faceted"];
const PALETTES: [&str; 4] = ["blade", "noir", "acid", "ember"];
const POSITIONS: [&str; 6] = [
    "center", "top-left", "top-center", "top-right", "bottom-left", "bottom-right",
];

pub struct AppState {
    pub config: settings::TrayConfig,
    pub daemon: Mutex<laris::LarisDaemon>,
    pub processing: AtomicBool,
    /// Set on every menu click: native check items auto-toggle visually even
    /// when the action fails, so the next poll tick must rebuild regardless
    /// of whether the snapshot changed.
    pub menu_dirty: AtomicBool,
    /// Windows installs fetch the whisper models on first run; dictation
    /// waits for them.
    pub models: Mutex<models::ModelState>,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Snapshot {
    /// Active session: (id, minutes, state). None when nothing is running.
    meeting: Option<(String, i64, miris::SessionState)>,
    /// Newest pending session: (id, displayed state, error message).
    pending: Option<(String, miris::SessionState, Option<String>)>,
    laris_alive: bool,
    ptt_trusted: bool, // false = daemon up but macOS denies keyboard monitoring
    laris_ready: bool, // false = daemon up, model still loading
    hands_free: bool,
    dictating: bool, // the daemon's toggle left the mic listening (Linux)
    has_last_note: bool,
    theme: Option<(String, String, String)>, // (archetype, palette, position)
    models: models::ModelState,
}

/// One primary action represents only the active session; a second item
/// represents the newest pending one. Disabled rows explain why.
fn primary_action(active: Option<miris::SessionState>) -> (&'static str, &'static str, bool) {
    match active {
        None => ("miris_start", "Gravar reunião", true),
        Some(miris::SessionState::Starting) => ("noop", "Iniciando gravação...", false),
        Some(miris::SessionState::Recording) => ("miris_stop", "Parar gravação", true),
        Some(miris::SessionState::Stopping) => ("noop", "Parando gravação...", false),
        Some(miris::SessionState::Processing) => ("noop", "Processando reunião...", false),
        // captured/interrupted/completed/failed are not active work
        Some(_) => ("miris_start", "Gravar reunião", true),
    }
}

fn pending_action(state: miris::SessionState) -> Option<(&'static str, &'static str)> {
    match state {
        miris::SessionState::Captured => Some(("miris_process", "Processar gravação")),
        miris::SessionState::Interrupted => Some(("miris_recover", "Recuperar gravação")),
        _ => None,
    }
}

/// A detached recording must never be orphaned by a quit: Sair is only
/// offered when the latest session reached a stable state (captured,
/// completed, interrupted, failed - or none at all).
fn quit_enabled(active: Option<miris::SessionState>) -> bool {
    !active.is_some_and(miris::SessionState::is_active)
}

/// Session ids are %Y%m%d-%H%M%S. Menu ids embed them; never let anything
/// that is not exactly that reach the CLI as a path component.
fn safe_session_id(raw: &str) -> Option<String> {
    let id = raw.trim();
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_digit() || c == '-'))
        .then(|| id.to_owned())
}

// Warn once when the daemon comes up without keyboard permission: the edge
// INTO the bad state, so the tray nags on transition, not every poll tick.
fn should_warn_untrusted(last: &Snapshot, current: &Snapshot) -> bool {
    current.laris_alive && !current.ptt_trusted && (!last.laris_alive || last.ptt_trusted)
}

/// Only an active recording session paints the red icon - a pending
/// interruption or a captured session does not.
fn is_recording(snap: &Snapshot) -> bool {
    meeting_state(snap) == Some(miris::SessionState::Recording)
}

/// The red icon means the mic is open: a meeting recording, or dictation
/// listening after a toggle (Linux, where there is no orb to show it).
fn shows_live_icon(snap: &Snapshot) -> bool {
    is_recording(snap) || snap.dictating
}

fn meeting_state(snap: &Snapshot) -> Option<miris::SessionState> {
    snap.meeting.as_ref().map(|(_, _, s)| *s)
}

/// Liveness is not readiness: uvicorn answers on :8421 seconds before the
/// model is loaded and the key listener attached.
fn dictation_status_label(alive: bool, ready: bool) -> &'static str {
    match (alive, ready) {
        (false, _) => "Ditado indisponível",
        (true, false) => "Carregando modelo...",
        (true, true) => "Ditado ativo",
    }
}

fn hands_free_enabled(alive: bool, ready: bool) -> bool {
    alive && ready
}

fn peers_path() -> PathBuf {
    settings::config_dir().join("peers.json")
}

/// True when a daemon we did not spawn is already answering on the peer url
/// (e.g. `python -m laris` started by hand per docs/INSTALL.md). Spawning a
/// second daemon on top of it fights over peers.json and the keyboard
/// listener - the tray must defer instead.
fn external_daemon_alive() -> bool {
    laris::read_peer(&peers_path()).map(|p| laris::alive(&p.url)).unwrap_or(false)
}

/// Un-owned and unreachable is the only state worth recovering: an external
/// daemon (alive=true) is not ours to manage, and an owned daemon is either
/// alive or on its way up. Capped so a daemon that fails on every start
/// (bad laris_dir, model download failing repeatedly) doesn't spin forever.
fn should_respawn(owns: bool, alive: bool, attempts: u32, cap: u32) -> bool {
    !owns && !alive && attempts < cap
}

fn sessions_dir(state: &AppState) -> PathBuf {
    PathBuf::from(&state.config.miris_home).join("sessions")
}

fn take_snapshot(state: &AppState) -> Snapshot {
    let sessions = sessions_dir(state);
    let snap = miris::snapshot(&sessions);
    let meeting = snap.active.as_ref().map(|v| {
        let mins = miris::recording_minutes(&v.manifest.session_id, chrono::Local::now())
            .unwrap_or(0);
        (v.manifest.session_id.clone(), mins, v.manifest.state)
    });
    let pending = snap.pending.as_ref().map(|v| {
        (
            v.manifest.session_id.clone(),
            v.derived_state,
            v.manifest.error.as_ref().map(|e| e.message.clone()),
        )
    });
    let peer = laris::read_peer(&peers_path());
    let laris_alive = peer.as_ref().map(|p| laris::alive(&p.url)).unwrap_or(false);
    // ponytail: theme fetched every tick while laris is up - localhost HTTP,
    // negligible; cache only if it ever shows up in a profile
    let theme_body = peer.filter(|_| laris_alive).and_then(|p| laris::get_theme(&p).ok());
    // absent field = old daemon: default trusted so we never nag on unknowns
    let ptt_trusted = theme_body
        .as_ref()
        .and_then(|b| b.get("ptt_trusted")?.as_bool())
        .unwrap_or(true);
    let laris_ready = theme_body
        .as_ref()
        .and_then(|b| b.get("ready")?.as_bool())
        .unwrap_or(true);
    // absent field = macOS or an old daemon: never claim the mic is open
    let dictating = theme_body
        .as_ref()
        .and_then(|b| b.get("dictating")?.as_bool())
        .unwrap_or(false);
    let theme = theme_body.and_then(|body| {
        let t = body.get("theme")?;
        Some((
            t.get("archetype")?.as_str()?.to_owned(),
            t.get("palette")?.as_str()?.to_owned(),
            t.get("position")?.as_str()?.to_owned(),
        ))
    });
    let laris_settings = settings::read_json_map(&settings::laris_settings_path());
    let hands_free =
        laris_settings.get("ASSISTANT_MODE").and_then(|v| v.as_str()) == Some("always_on");
    Snapshot {
        meeting,
        pending,
        laris_alive,
        ptt_trusted,
        laris_ready,
        hands_free,
        dictating,
        has_last_note: snap.last_note.is_some(),
        theme,
        models: *state.models.lock().unwrap(),
    }
}

/// Hover text of the tray icon: how long the meeting has been recording.
fn tray_tooltip(recording_session: Option<&str>, now: chrono::DateTime<chrono::Local>) -> String {
    match recording_session.and_then(|id| recording_clock(id, now)) {
        Some((_, elapsed)) => format!("VOXIS - gravando reunião há {elapsed}"),
        None => "VOXIS".to_owned(),
    }
}

/// `<meetrec> models fetch`: progress goes to the tooltip and the menu, and
/// a successful fetch starts dictation.
fn fetch_models(app: &AppHandle, bin: &str) {
    use std::io::BufRead;
    let state = app.state::<AppState>();
    let mut cmd = std::process::Command::new(bin);
    cmd.args(["models", "fetch"]).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            tray_log(&format!("models fetch: {bin}: {e}"));
            *state.models.lock().unwrap() = models::ModelState::Failed;
            state.menu_dirty.store(true, Ordering::SeqCst);
            notify(app, "Falha ao baixar modelos: sem conexão; tente de novo");
            return;
        }
    };
    let stderr = child.stderr.take();
    let err_log = std::thread::spawn(move || {
        if let Some(e) = stderr {
            for line in std::io::BufReader::new(e).lines().map_while(Result::ok) {
                tray_log(&format!("models fetch: {line}"));
            }
        }
    });
    if let Some(out) = child.stdout.take() {
        let mut last = None;
        for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            if let Some(p) = models::progress_pct(&line) {
                if last != Some(p) {
                    last = Some(p);
                    *state.models.lock().unwrap() = models::ModelState::Downloading(p);
                }
            } else {
                tray_log(&format!("models fetch: {line}"));
            }
        }
    }
    let code = child.wait().ok().and_then(|s| s.code());
    let _ = err_log.join();
    match models::outcome(code) {
        models::Outcome::StartDictation => {
            *state.models.lock().unwrap() = models::ModelState::Ready;
            if !external_daemon_alive() {
                if let Err(e) = models::spawn_dictation(models::ModelState::Ready, &mut state.daemon.lock().unwrap()) {
                    notify(app, &format!("Falha ao iniciar ditado: {e}"));
                }
            }
        }
        models::Outcome::Toast(msg) => {
            *state.models.lock().unwrap() = models::ModelState::Failed;
            notify(app, msg);
        }
    }
    state.menu_dirty.store(true, Ordering::SeqCst);
}

/// ~/.voxis/logs/tray-<date>.log: the tray has no console on Windows.
fn tray_log(msg: &str) {
    let Some(home) = dirs::home_dir() else { return };
    let dir = home.join(".voxis").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let now = chrono::Local::now();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("tray-{}.log", now.format("%Y-%m-%d"))))
    {
        use std::io::Write;
        let _ = writeln!(f, "{} {msg}", now.format("%H:%M:%S"));
    }
}

fn build_menu(app: &AppHandle, snap: &Snapshot) -> tauri::Result<Menu<Wry>> {
    let active_state = meeting_state(snap);
    let (id, base_label, enabled) = primary_action(active_state);
    let label = if active_state == Some(miris::SessionState::Recording) {
        let mins = snap.meeting.as_ref().map(|(_, m, _)| *m).unwrap_or(0);
        format!("{base_label} ({mins} min)")
    } else {
        base_label.to_owned()
    };
    let meeting = MenuItem::with_id(app, id, label, enabled, None::<&str>)?;

    let mut items: Vec<Box<dyn IsMenuItem<Wry>>> = vec![Box::new(meeting)];
    if let Some((session_id, state, _)) = &snap.pending {
        if let Some((base, text)) = pending_action(*state) {
            items.push(Box::new(MenuItem::with_id(
                app,
                format!("{base}:{session_id}"),
                text,
                true,
                None::<&str>,
            )?));
        }
    }

    let last_note =
        MenuItem::with_id(app, "last_note", "Última nota", snap.has_last_note, None::<&str>)?;
    items.push(Box::new(last_note));
    let sep1 = PredefinedMenuItem::separator(app)?;
    items.push(Box::new(sep1));
    // status, not a switch: dictation is always on and cmd_r is the control.
    // Own id, not "noop" - the meeting item already uses that one while
    // processing, and two items sharing an id in one menu is asking for a
    // handler to fire on the wrong row.
    let dictation = MenuItem::with_id(
        app,
        "dictation_status",
        models::dictation_label(snap.models)
            .unwrap_or_else(|| dictation_status_label(snap.laris_alive, snap.laris_ready).to_owned()),
        false, // disabled: nothing to click
        None::<&str>,
    )?;
    items.push(Box::new(dictation));
    let hands_free = CheckMenuItem::with_id(
        app, "hands_free", "Hands-free",
        hands_free_enabled(snap.laris_alive, snap.laris_ready),
        snap.hands_free, None::<&str>,
    )?;
    items.push(Box::new(hands_free));

    let (cur_arch, cur_pal, cur_pos) = snap.theme.clone().unwrap_or_default();
    let mut theme_children: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    for a in ARCHETYPES {
        theme_children.push(Box::new(CheckMenuItem::with_id(
            app, format!("theme:archetype:{a}"), a, true, a == cur_arch, None::<&str>,
        )?));
    }
    theme_children.push(Box::new(PredefinedMenuItem::separator(app)?));
    for p in PALETTES {
        theme_children.push(Box::new(CheckMenuItem::with_id(
            app, format!("theme:palette:{p}"), p, true, p == cur_pal, None::<&str>,
        )?));
    }
    theme_children.push(Box::new(PredefinedMenuItem::separator(app)?));
    for p in POSITIONS {
        theme_children.push(Box::new(CheckMenuItem::with_id(
            app, format!("theme:position:{p}"), p, true, p == cur_pos, None::<&str>,
        )?));
    }
    let refs: Vec<&dyn IsMenuItem<Wry>> = theme_children.iter().map(|b| b.as_ref()).collect();
    let theme_menu = Submenu::with_id_and_items(
        app, "theme", "Tema do orb",
        hands_free_enabled(snap.laris_alive, snap.laris_ready),
        &refs,
    )?;
    items.push(Box::new(theme_menu));

    let sep2 = PredefinedMenuItem::separator(app)?;
    items.push(Box::new(sep2));
    #[cfg(windows)]
    {
        let autostart = CheckMenuItem::with_id(
            app, "autostart", "Iniciar com o Windows", true,
            autostart::menu_checked(&run_key()), None::<&str>,
        )?;
        items.push(Box::new(autostart));
    }
    let settings_item =
        MenuItem::with_id(app, "settings", "Configurações...", true, None::<&str>)?;
    items.push(Box::new(settings_item));
    let quit = MenuItem::with_id(
        app, "quit", "Sair",
        quit_enabled(active_state),
        None::<&str>,
    )?;
    items.push(Box::new(quit));

    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|b| b.as_ref()).collect();
    Menu::with_items(app, &refs)
}

fn set_tray_icon(tray: &TrayIcon, recording: bool) {
    let (bytes, template): (&[u8], bool) = if recording {
        (include_bytes!("../icons/tray-rec.png"), false)
    } else {
        (include_bytes!("../icons/tray-idle.png"), true)
    };
    if let Ok(img) = tauri::image::Image::from_bytes(bytes) {
        // icon first, template after: macOS applies the template attribute to
        // the CURRENT image - the reverse order leaves the new image raw-color
        // and it vanishes against a similarly colored wallpaper
        let _ = tray.set_icon(Some(img));
        let _ = tray.set_icon_as_template(template);
    }
}

fn notify(app: &AppHandle, body: &str) {
    #[cfg(windows)]
    {
        let _ = app;
        toast::notify(body);
    }
    #[cfg(not(windows))]
    let _ = app.notification().builder().title("VOXIS").body(body).show();
}

/// (start "HH:MM", elapsed "HH:MM:SS") of a recording session id.
fn recording_clock(session_id: &str, now: chrono::DateTime<chrono::Local>) -> Option<(String, String)> {
    let started = chrono::NaiveDateTime::parse_from_str(session_id, "%Y%m%d-%H%M%S")
        .ok()?
        .and_local_timezone(chrono::Local)
        .single()?;
    let s = (now - started).num_seconds().max(0);
    Some((started.format("%H:%M").to_string(), format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)))
}

/// Raise the macOS microphone prompt when it hasn't been decided yet. laris
/// opens the mic from a spawned, bundle-less Python daemon that TCC won't show a
/// dialog for; an in-process objc2 `requestAccess` also stayed a silent no-op.
/// Reading the status via objc2 works, so we do that here and, only when it's
/// notDetermined, spawn the bundled Swift sidecar (grant-mic) which CAN present
/// the prompt. It is signed with the app identity, so the grant lands on
/// VOXIS.app and unblocks the daemon's capture. No-op once decided.
#[cfg(target_os = "macos")]
fn ensure_mic_access() {
    use objc2::msg_send;
    use objc2::runtime::AnyClass;
    use objc2_foundation::NSString;

    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {
        static AVMediaTypeAudio: *const NSString;
    }
    let audio: &NSString = unsafe { &*AVMediaTypeAudio };
    let Some(av_capture_device) = AnyClass::get(c"AVCaptureDevice") else {
        return;
    };
    // AVAuthorizationStatus: 0 = notDetermined, 3 = authorized
    let status: isize =
        unsafe { msg_send![av_capture_device, authorizationStatusForMediaType: audio] };
    if status != 0 {
        return;
    }
    // Raise the prompt via the bundled GrantMic.app - a short-lived Regular-policy
    // Swift app sharing VOXIS's identity, launched through LaunchServices (the
    // tray's own objc2 requestAccess and bare-subprocess spawns both stayed
    // silent no-ops). It MUST run de-nested: a .app nested inside another bundle
    // is auto-denied mic, a standalone copy gets the dialog - so ditto it out
    // (ditto preserves the signature) and open that. Absent in dev (unbundled).
    let Ok(exe) = std::env::current_exe() else { return };
    let Some(bundled) = exe
        .parent()
        .and_then(|p| p.parent())
        .map(|contents| contents.join("Resources/resources/GrantMic.app"))
    else {
        return;
    };
    if !bundled.exists() {
        return;
    }
    let dest = std::env::temp_dir().join("VOXIS-GrantMic.app");
    if std::process::Command::new("ditto").arg(&bundled).arg(&dest).status().is_ok() {
        let _ = std::process::Command::new("open").arg("-n").arg(&dest).spawn();
    }
}

/// The HKCU Run value that starts this VOXIS.exe at logon.
#[cfg(windows)]
fn run_key() -> autostart::RunKey {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("VOXIS.exe"));
    autostart::RunKey::new("VOXIS", &exe)
}

fn handle_menu(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    state.menu_dirty.store(true, Ordering::SeqCst);
    match id {
        #[cfg(windows)]
        "autostart" => {
            if let Err(e) = autostart::toggle(&run_key()) {
                notify(app, &format!("Falha ao configurar o início com o Windows: {e}"));
            }
        }
        "quit" => {
            // the menu item is disabled during active work, but a click can
            // race a session that started since the last 2s poll - re-check
            if miris::snapshot(&sessions_dir(&state)).active.is_some() {
                return;
            }
            state.daemon.lock().unwrap().stop();
            app.exit(0);
        }
        "settings" => commands::show_settings_window(app),
        "miris_start" => {
            // off the menu thread: the miris CLI takes seconds just to import
            let app2 = app.clone();
            let bin = state.config.miris_bin.clone();
            std::thread::spawn(move || {
                if let Err(e) = miris::start(&bin) {
                    notify(&app2, &format!("Falha ao iniciar gravação: {e}"));
                }
            });
        }
        "miris_stop" => {
            // the menu item only disables on the next poll tick - a double
            // click must not launch two concurrent `miris stop` pipelines
            if state
                .processing
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                return;
            }
            let app2 = app.clone();
            let bin = state.config.miris_bin.clone();
            std::thread::spawn(move || {
                let msg = match miris::stop(&bin) {
                    Ok(miris::StopOutcome::Note(note)) => format!(
                        "Nota salva: {}",
                        note.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
                    ),
                    Ok(miris::StopOutcome::Interrupted) => {
                        "Gravação interrompida: recupere pelo menu".to_owned()
                    }
                    Err(e) => format!("Falha ao processar reunião: {e}"),
                };
                app2.state::<AppState>().processing.store(false, Ordering::SeqCst);
                notify(&app2, &msg);
            });
        }
        id if id.starts_with("miris_recover:") || id.starts_with("miris_process:") => {
            let (base, raw) = id.split_once(':').unwrap_or(("", ""));
            let Some(session_id) = safe_session_id(raw) else { return };
            // same suppression as stop: one CLI action at a time
            if state
                .processing
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                return;
            }
            let app2 = app.clone();
            let bin = state.config.miris_bin.clone();
            let recovering = base == "miris_recover";
            std::thread::spawn(move || {
                let result = if recovering {
                    miris::recover(&bin, &session_id).map(|_| None)
                } else {
                    miris::process(&bin, &session_id).map(Some)
                };
                let msg = if recovering {
                    match result {
                        Ok(_) => return, // the poll flips the row to Processar
                        Err(e) => format!("Falha ao recuperar gravação: {e}"),
                    }
                } else {
                    match result {
                        Ok(Some(note)) => format!(
                            "Nota salva: {}",
                            note.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
                        ),
                        Ok(None) => unreachable!("process maps to Some"),
                        Err(e) => format!("Falha ao processar reunião: {e}"),
                    }
                };
                app2.state::<AppState>().processing.store(false, Ordering::SeqCst);
                notify(&app2, &msg);
            });
        }
        "last_note" => {
            if let Some(note) = miris::last_note(&sessions_dir(&state)) {
                let _ = tauri_plugin_opener::open_path(note, None::<&str>);
            }
        }
        "hands_free" => {
            let app2 = app.clone();
            std::thread::spawn(move || {
                let path = settings::laris_settings_path();
                let mut map = settings::read_json_map(&path);
                let now_on =
                    map.get("ASSISTANT_MODE").and_then(|v| v.as_str()) == Some("always_on");
                let next = if now_on { "ptt" } else { "always_on" };
                map.insert("ASSISTANT_MODE".into(), serde_json::Value::String(next.into()));
                if let Err(e) = settings::write_json_atomic(&path, &serde_json::Value::Object(map)) {
                    notify(&app2, &format!("Falha ao salvar: {e}"));
                    return;
                }
                // settings load at daemon start only - restart our own daemon to apply
                let state = app2.state::<AppState>();
                let mut daemon = state.daemon.lock().unwrap();
                if daemon.owns_running() {
                    daemon.stop();
                    if let Err(e) = daemon.spawn() {
                        notify(&app2, &format!("Falha ao reiniciar ditado: {e}"));
                    }
                }
            });
        }
        id if id.starts_with("theme:") => {
            let parts: Vec<&str> = id.splitn(3, ':').collect();
            if let [_, key, value] = parts[..] {
                if let Some(peer) = laris::read_peer(&peers_path()) {
                    if let Err(e) = laris::set_theme(&peer, serde_json::json!({ key: value })) {
                        notify(app, &format!("Falha ao aplicar tema: {e}"));
                    }
                }
            }
        }
        _ => {}
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // must be the first registered plugin; a second launch lands here in
        // the FIRST instance and we just surface the settings window
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            commands::show_settings_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_laris_settings,
            commands::get_audio_devices,
            commands::save_audio_devices,
            commands::save_miris_config,
            commands::save_theme,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            #[cfg(target_os = "macos")]
            ensure_mic_access();

            let config = settings::load_tray_config();
            let daemon = laris::LarisDaemon::new(
                PathBuf::from(&config.laris_dir),
                Some(settings::config_dir().join("laris.log")),
            )
            .with_bin(&config.laris_bin)
            // the installer ships the orb page as a resource; a dev build
            // without it lets meetrec fall back to its checkout copy
            .with_frontend(
                app.path()
                    .resource_dir()
                    .ok()
                    .map(|d| d.join("frontend"))
                    .filter(|d| d.join("overlay.html").is_file()),
            );
            // the native binary looks for its models and fetches them when
            // missing; the Python daemon manages its own
            let fetch_with = (cfg!(windows) && !config.laris_bin.is_empty()).then(|| config.laris_bin.clone());
            let model_state = if fetch_with.is_some() {
                models::ModelState::Downloading(0)
            } else {
                models::ModelState::Ready
            };
            app.manage(AppState {
                config,
                daemon: Mutex::new(daemon),
                processing: AtomicBool::new(false),
                menu_dirty: AtomicBool::new(false),
                models: Mutex::new(model_state),
            });

            // The model costs ~3.6s and 276MB to load. Loading it once per
            // session beats reloading it per toggle; "Sair" is what releases it.
            {
                let state = app.state::<AppState>();
                let mut daemon = state.daemon.lock().unwrap();
                if external_daemon_alive() {
                    // don't spawn on top of it: second process overwrites
                    // peers.json with a token the first server rejects, and
                    // still starts its own keyboard listener alongside it
                    notify(app.handle(), "Ditado foi iniciado fora do VOXIS - encerre-o por lá");
                } else if let Err(e) = models::spawn_dictation(*state.models.lock().unwrap(), &mut daemon) {
                    // a bad laris_dir would otherwise fail silently at boot -
                    // the old code path surfaced this from the menu handler
                    notify(app.handle(), &format!("Falha ao iniciar ditado: {e}"));
                }
            }

            if let Some(bin) = fetch_with {
                let handle = app.handle().clone();
                std::thread::spawn(move || fetch_models(&handle, &bin));
            }

            let snap = take_snapshot(&app.state::<AppState>());
            let menu = build_menu(app.handle(), &snap)?;
            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-idle.png"))?;
            let tray = TrayIconBuilder::with_id("voxis")
                .icon(icon)
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| handle_menu(app, event.id().as_ref()))
                .build(app)?;
            set_tray_icon(&tray, shows_live_icon(&snap));
            // a tray (re)started mid-meeting sees no start transition: show it now
            tray_log(&format!("tray started, meeting={:?}", snap.meeting));
            #[cfg(windows)]
            toast::register();
            if is_recording(&snap) {
                if let Some((id, _, _)) = &snap.meeting {
                    let _ = tray.set_tooltip(Some(tray_tooltip(Some(id), chrono::Local::now())));
                    // a tray restarted mid-meeting puts the live toast back
                    #[cfg(windows)]
                    if let Some((started, elapsed)) = recording_clock(id, chrono::Local::now()) {
                        toast::recording_ended();
                        toast::recording_started(&started, &elapsed);
                    }
                }
            }

            // poll loop: snapshot off the main thread, menu mutations on it
            // (macOS requires menu construction on the main thread)
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut last = snap;
                // process-lifetime cap, not per-tick: a daemon that fails on
                // every start (bad install, model download broken) must stop
                // retrying, not burn CPU/notifications until the app quits
                let mut respawn_attempts: u32 = 0;
                const RESPAWN_CAP: u32 = 3;
                loop {
                    std::thread::sleep(Duration::from_secs(2));
                    let state = handle.state::<AppState>();
                    let dirty = state.menu_dirty.swap(false, Ordering::SeqCst);
                    let current = take_snapshot(&state);

                    // recover from a dead tray-owned daemon (crash, OOM, a
                    // model download that failed on first boot) - every other
                    // restart path is gated on owns_running() and stays a
                    // permanent no-op once the child has exited
                    let owns = state.daemon.lock().unwrap().owns_running();
                    // a first run still fetching its models has nothing to respawn
                    if current.models == models::ModelState::Ready
                        && should_respawn(owns, current.laris_alive, respawn_attempts, RESPAWN_CAP)
                    {
                        respawn_attempts += 1;
                        if let Err(e) = models::spawn_dictation(current.models, &mut state.daemon.lock().unwrap()) {
                            notify(&handle, &format!("Falha ao reiniciar ditado: {e}"));
                        }
                    }

                    // the tooltip clock moves every tick, even when nothing else does
                    let recording_id = current
                        .meeting
                        .as_ref()
                        .filter(|_| is_recording(&current))
                        .map(|(id, _, _)| id.clone());
                    let tip = match current.models {
                        models::ModelState::Downloading(p) if recording_id.is_none() => models::download_tooltip(p),
                        _ => tray_tooltip(recording_id.as_deref(), chrono::Local::now()),
                    };
                    let h3 = handle.clone();
                    let _ = handle.run_on_main_thread(move || {
                        if let Some(tray) = h3.tray_by_id("voxis") {
                            let _ = tray.set_tooltip(Some(tip));
                        }
                    });
                    // status lives in the notification area only: red icon,
                    // tooltip clock, and a live toast in the notification center
                    let clock = recording_id.as_deref().and_then(|id| recording_clock(id, chrono::Local::now()));
                    if is_recording(&current) != is_recording(&last) {
                        let ended = last.meeting.as_ref().map(|(id, _, _)| id.clone());
                        match (&clock, &ended) {
                            (Some((started, elapsed)), _) => {
                                #[cfg(windows)]
                                toast::recording_started(started, elapsed);
                                #[cfg(not(windows))]
                                notify(&handle, &format!("Gravando reunião desde {started} ({elapsed})"));
                            }
                            (None, Some(id)) => {
                                #[cfg(windows)]
                                toast::recording_ended();
                                let dur = recording_clock(id, chrono::Local::now()).map(|c| c.1).unwrap_or_default();
                                notify(&handle, &format!("Gravação encerrada ({dur}) - gerando a ata"));
                            }
                            _ => {}
                        }
                    } else if let Some((_started, _elapsed)) = &clock {
                        #[cfg(windows)]
                        toast::recording_tick(_started, _elapsed);
                    }
                    if current == last && !dirty {
                        continue;
                    }
                    let recording_changed = shows_live_icon(&current) != shows_live_icon(&last);
                    // an active capture that turned into a pending interrupted
                    // session is the user's four-second notification target
                    if last.meeting.is_some() && !current.meeting.is_some() {
                        if let Some((session, state, error)) = &current.pending {
                            if *state == miris::SessionState::Interrupted {
                                let reason = error.clone().unwrap_or_else(|| {
                                    "falha de captura".to_owned()
                                });
                                notify(&handle, &format!("Gravação interrompida: {reason}"));
                                let _ = session; // the pending row offers the recovery
                            }
                        }
                    }
                    if should_warn_untrusted(&last, &current) {
                        notify(&handle, "Ditado sem permissão: ative VOXIS em Ajustes › \
                            Privacidade e Segurança › Acessibilidade E Monitoramento de \
                            Entrada, depois reinicie o VOXIS.");
                    }
                    last = current.clone();
                    let handle2 = handle.clone();
                    let _ = handle.run_on_main_thread(move || {
                        if let Some(tray) = handle2.tray_by_id("voxis") {
                            if let Ok(menu) = build_menu(&handle2, &current) {
                                let _ = tray.set_menu(Some(menu));
                            }
                            if recording_changed {
                                set_tray_icon(&tray, shows_live_icon(&current));
                            }
                        }
                    });
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { code, api, .. } = event {
                if should_prevent_exit(code) {
                    api.prevent_exit();
                }
            }
        });
}

/// The tray owns no permanent window, so closing Settings leaves the runtime
/// with none and it asks to exit - taking dictation down with it. Exit code
/// None is that request; Some means "Sair" called app.exit and must proceed.
fn should_prevent_exit(code: Option<i32>) -> bool {
    code.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tray_tooltip_shows_how_long_the_meeting_has_been_recording() {
        let now = chrono::NaiveDateTime::parse_from_str("20261105-221502", "%Y%m%d-%H%M%S")
            .unwrap()
            .and_local_timezone(chrono::Local)
            .unwrap();

        let tip = tray_tooltip(Some("20261105-210228"), now);

        assert_eq!(tip, "VOXIS - gravando reunião há 01:12:34");
    }

    #[test]
    fn the_tray_tooltip_is_just_the_name_when_nothing_records() {
        assert_eq!(tray_tooltip(None, chrono::Local::now()), "VOXIS");
    }

    #[test]
    fn closing_the_settings_window_must_not_exit_the_tray() {
        // VOXIS is an Accessory app with no permanent window: when Settings
        // closes, the runtime asks to exit because no window is left. code
        // None = that runtime request; Some = the user picked "Sair"
        // (app.exit), which must still go through.
        assert!(should_prevent_exit(None), "closing a window must not kill the tray");
        assert!(!should_prevent_exit(Some(0)), "the Sair menu item must still exit");
    }


    fn snap(laris_alive: bool, ptt_trusted: bool) -> Snapshot {
        Snapshot { laris_alive, ptt_trusted, ..Default::default() }
    }

    #[test]
    fn warns_on_transition_into_untrusted() {
        // daemon just came up (menu -> Ditado) without keyboard permission
        assert!(should_warn_untrusted(&snap(false, true), &snap(true, false)));
        // was trusted, permission lost/re-probed as denied
        assert!(should_warn_untrusted(&snap(true, true), &snap(true, false)));
    }

    #[test]
    fn silent_when_already_untrusted_or_fine() {
        // steady bad state must not nag every 2s tick
        assert!(!should_warn_untrusted(&snap(true, false), &snap(true, false)));
        // trusted daemon
        assert!(!should_warn_untrusted(&snap(false, true), &snap(true, true)));
        // daemon down
        assert!(!should_warn_untrusted(&snap(true, false), &snap(false, true)));
    }

    #[test]
    fn dictation_label_distinguishes_loading_from_ready() {
        // The whole point of this work: "Ditado" must not appear while the model
        // is still loading, because cmd_r does nothing during that window.
        assert_eq!(dictation_status_label(true, true), "Ditado ativo");
        assert_eq!(dictation_status_label(true, false), "Carregando modelo...");
        assert_eq!(dictation_status_label(false, false), "Ditado indisponível");
        // a dead daemon is never ready, whatever a stale payload claims
        assert_eq!(dictation_status_label(false, true), "Ditado indisponível");
    }

    #[test]
    fn the_icon_turns_red_while_dictation_is_listening() {
        // Linux has no orb: with no meeting running, the tray icon is the
        // only sign that a toggle left the mic open
        let listening = Snapshot { dictating: true, ..Default::default() };
        assert!(shows_live_icon(&listening));
    }

    #[test]
    fn the_icon_stays_idle_with_no_meeting_and_no_dictation() {
        assert!(!shows_live_icon(&Snapshot::default()));
    }

    #[test]
    fn hands_free_is_only_offered_once_the_model_is_loaded() {
        // toggling hands-free against a daemon that has not finished warming
        // silently does nothing, which is the failure mode being removed
        assert!(!hands_free_enabled(true, false));
        assert!(hands_free_enabled(true, true));
        assert!(!hands_free_enabled(false, true));
    }

    #[test]
    fn respawns_a_dead_unowned_daemon_below_the_cap() {
        // this is the recovery path itself: our child exited (crash, OOM,
        // failed model download) and nothing else will bring it back
        assert!(should_respawn(false, false, 0, 3));
        assert!(should_respawn(false, false, 2, 3));
    }

    #[test]
    fn stops_respawning_once_the_cap_is_reached() {
        // a daemon that fails on every start must not spin forever
        assert!(!should_respawn(false, false, 3, 3));
        assert!(!should_respawn(false, false, 10, 3));
    }

    #[test]
    fn never_respawns_over_a_reachable_external_daemon() {
        // alive but not owned by us = someone else's process (docs/INSTALL.md
        // standalone run) - not ours to manage, whatever the attempt count
        assert!(!should_respawn(false, true, 0, 3));
    }

    #[test]
    fn never_respawns_a_live_owned_daemon() {
        assert!(!should_respawn(true, true, 0, 3));
        // owned but not yet answering the TCP probe - still starting, not dead
        assert!(!should_respawn(true, false, 0, 3));
    }

    #[test]
    fn menu_labels_follow_lifecycle() {
        assert_eq!(primary_action(None), ("miris_start", "Gravar reunião", true));
        assert_eq!(
            primary_action(Some(miris::SessionState::Starting)).1,
            "Iniciando gravação..."
        );
        assert_eq!(
            primary_action(Some(miris::SessionState::Recording)).0,
            "miris_stop"
        );
        assert_eq!(
            primary_action(Some(miris::SessionState::Stopping)).1,
            "Parando gravação..."
        );
        assert_eq!(
            primary_action(Some(miris::SessionState::Processing)).1,
            "Processando reunião..."
        );
        assert_eq!(
            pending_action(miris::SessionState::Interrupted).map(|(id, _)| id),
            Some("miris_recover")
        );
        assert_eq!(
            pending_action(miris::SessionState::Captured).map(|(id, _)| id),
            Some("miris_process")
        );
        assert!(pending_action(miris::SessionState::Completed).is_none());
    }

    #[test]
    fn quit_is_disabled_only_during_active_work() {
        assert!(!quit_enabled(Some(miris::SessionState::Recording)));
        assert!(!quit_enabled(Some(miris::SessionState::Processing)));
        assert!(!quit_enabled(Some(miris::SessionState::Starting)));
        assert!(!quit_enabled(Some(miris::SessionState::Stopping)));
        assert!(quit_enabled(None));
        assert!(quit_enabled(Some(miris::SessionState::Interrupted)));
        assert!(quit_enabled(Some(miris::SessionState::Captured)));
    }

    #[test]
    fn safe_session_id_only_accepts_timestamp_names() {
        assert_eq!(
            safe_session_id("20260914-143000").as_deref(),
            Some("20260914-143000")
        );
        assert_eq!(safe_session_id("../etc"), None);
        assert_eq!(safe_session_id("x; rm -rf /"), None);
        assert_eq!(safe_session_id(""), None);
    }
}
