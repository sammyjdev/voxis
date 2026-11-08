//! Tauri commands backing the settings window.

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::{laris, settings, AppState};

const KNOWN_LARIS_KEYS: [&str; 5] = [
    "ASSISTANT_MODE",
    "DICTATE_KEY",
    "STT_MODEL",
    "NOISE_REDUCTION",
    "INITIAL_PROMPT",
];

// mirror of laris/hotkey.py _ALLOWED
const DICTATE_KEYS: [&str; 15] = [
    "cmd_r", "cmd_l", "shift_r", "shift_l", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12",
    "f13", "f14", "f15",
];

/// Value-level validation: a select with no matching option submits "" - the
/// key check alone would happily persist it and break the engine at boot.
fn valid_value(key: &str, value: &serde_json::Value) -> bool {
    let Some(s) = value.as_str() else { return false };
    match key {
        "ASSISTANT_MODE" => matches!(s, "ptt" | "always_on"),
        "NOISE_REDUCTION" => matches!(s, "on" | "off"),
        "DICTATE_KEY" => DICTATE_KEYS.contains(&s),
        "STT_MODEL" => !s.trim().is_empty(),
        // empty is meaningful: it is how the user disables priming
        "INITIAL_PROMPT" => s.chars().count() <= 900,
        _ => false,
    }
}

/// The keys meetrec's dictate hook reads on Windows (dictate.rs `key_vk`).
const WINDOWS_DICTATE_KEYS: [&str; 11] = ["f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12", "f13", "f14", "f15"];

/// Settings the Windows binary does not apply; kept on disk, never written.
const NOT_ON_WINDOWS: [&str; 2] = ["STT_MODEL", "NOISE_REDUCTION"];

/// The dictation keys the settings window offers on this platform.
pub fn dictate_keys(windows: bool) -> &'static [&'static str] {
    if windows {
        &WINDOWS_DICTATE_KEYS
    } else {
        &DICTATE_KEYS
    }
}

/// The checks a `save_laris_settings` call passes before anything is written.
/// On Windows a setting the native binary ignores is refused rather than
/// saved with a false success, and the key must be one it reads.
fn validate_laris_updates(updates: &serde_json::Map<String, serde_json::Value>, windows: bool) -> Result<(), String> {
    if let Some(bad) = updates.keys().find(|k| !KNOWN_LARIS_KEYS.contains(&k.as_str())) {
        return Err(format!("unknown setting: {bad}"));
    }
    if let Some(key) = updates.keys().find(|k| windows && NOT_ON_WINDOWS.contains(&k.as_str())) {
        return Err(format!("unsupported on Windows: {key}"));
    }
    if let Some((key, value)) = updates.iter().find(|(k, v)| !valid_value(k, v)) {
        return Err(format!("invalid value for {key}: {value}"));
    }
    if let Some(key) = updates.get("DICTATE_KEY").and_then(|v| v.as_str()).filter(|k| !dictate_keys(windows).contains(k)) {
        return Err(format!("invalid value for DICTATE_KEY: {key} (Windows reads f5 to f15)"));
    }
    Ok(())
}

/// Validates, then merges `updates` into the settings file at `path`.
fn apply_laris_updates(
    path: &std::path::Path,
    updates: serde_json::Map<String, serde_json::Value>,
    windows: bool,
) -> Result<(), String> {
    validate_laris_updates(&updates, windows)?;
    let mut map = settings::read_json_map(path);
    for (k, v) in updates {
        map.insert(k, v);
    }
    settings::write_json_atomic(path, &serde_json::Value::Object(map))
}

/// Whether the meetings settings (vault, LLM command, categories) save here:
/// the Windows binary writes notes to ata.md or Google Drive, not a vault.
fn miris_config_save(windows: bool) -> Result<(), String> {
    if windows {
        Err("unsupported on Windows: vault_root, llm_command, categories".to_owned())
    } else {
        Ok(())
    }
}

#[derive(serde::Serialize)]
pub struct SettingsPayload {
    laris: serde_json::Map<String, serde_json::Value>,
    miris: serde_json::Value,
    theme: Option<serde_json::Value>,
    laris_alive: bool,
    version: String,
    /// "windows" makes the window show only what the native binary applies
    platform: &'static str,
    dictate_keys: Vec<&'static str>,
}

fn peer() -> Option<laris::LarisPeer> {
    laris::read_peer(&settings::config_dir().join("peers.json"))
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> SettingsPayload {
    let state = app.state::<AppState>();
    let miris_toml = PathBuf::from(&state.config.miris_home).join("config.toml");
    let peer = peer();
    let laris_alive = peer.as_ref().map(|p| laris::alive(&p.url)).unwrap_or(false);
    let theme = peer
        .filter(|_| laris_alive)
        .and_then(|p| laris::get_theme(&p).ok())
        .and_then(|mut body| body.get_mut("theme").map(serde_json::Value::take));
    SettingsPayload {
        laris: settings::read_json_map(&settings::laris_settings_path()),
        miris: settings::read_miris_config(&miris_toml)
            .unwrap_or_else(|_| serde_json::json!({})),
        theme,
        laris_alive,
        version: app.package_info().version.to_string(),
        platform: if cfg!(windows) { "windows" } else { "other" },
        dictate_keys: dictate_keys(cfg!(windows)).to_vec(),
    }
}

#[tauri::command]
pub async fn save_laris_settings(
    app: AppHandle,
    updates: serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    apply_laris_updates(&settings::laris_settings_path(), updates, cfg!(windows))?;
    // apply immediately when the tray owns the daemon (settings load at boot)
    let state = app.state::<AppState>();
    let mut daemon = state.daemon.lock().unwrap();
    if daemon.owns_running() {
        daemon.stop();
        daemon.spawn()?;
    }
    Ok(())
}

#[tauri::command]
pub async fn save_miris_config(
    app: AppHandle,
    vault_root: String,
    llm_command: Vec<String>,
    categories: Vec<(String, String)>,
) -> Result<(), String> {
    miris_config_save(cfg!(windows))?;
    let state = app.state::<AppState>();
    let path = PathBuf::from(&state.config.miris_home).join("config.toml");
    settings::update_miris_config(&path, &vault_root, &llm_command, &categories)
}

/// Recording and playback devices as the native binary sees them
/// (`<miris_bin> devices --json`), plus the current choice from config.toml.
#[tauri::command]
pub async fn get_audio_devices(app: AppHandle) -> Result<serde_json::Value, String> {
    let bin = app.state::<AppState>().config.miris_bin.clone();
    let mut cmd = std::process::Command::new(&bin);
    cmd.args(["devices", "--json"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().map_err(|e| format!("{bin}: {e}"))?;
    let mut list: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("lista de dispositivos: {e}"))?;
    let (mic, speaker) = settings::read_audio_devices(&settings::voxis_config_path());
    list["mic"] = mic.into();
    list["speaker"] = speaker.into();
    Ok(list)
}

/// Empty = system default. The dictation daemon opens the mic at boot, so a
/// running one is restarted; a meeting reads the choice when it starts.
#[tauri::command]
pub async fn save_audio_devices(app: AppHandle, mic: String, speaker: String) -> Result<(), String> {
    settings::write_audio_devices(&settings::voxis_config_path(), &mic, &speaker)?;
    let state = app.state::<AppState>();
    let mut daemon = state.daemon.lock().unwrap();
    if daemon.owns_running() {
        daemon.stop();
        daemon.spawn()?;
    }
    Ok(())
}

#[tauri::command]
pub async fn save_theme(updates: serde_json::Value) -> Result<serde_json::Value, String> {
    let peer = peer().ok_or("laris offline")?;
    laris::set_theme(&peer, updates)
}

pub fn show_settings_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let _ = tauri::WebviewWindowBuilder::new(app, "settings", tauri::WebviewUrl::default())
        .title("VOXIS - Configurações")
        .inner_size(560.0, 680.0)
        .build();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_prompt_is_an_accepted_setting() {
        assert!(valid_value("INITIAL_PROMPT", &serde_json::json!("PortAudio, AXON")));
        // empty is how the user turns it off, so it must round-trip
        assert!(valid_value("INITIAL_PROMPT", &serde_json::json!("")));
        assert!(!valid_value("INITIAL_PROMPT", &serde_json::json!(42)));
        assert!(KNOWN_LARIS_KEYS.contains(&"INITIAL_PROMPT"));
    }

    fn updates(pairs: &[(&str, &str)]) -> serde_json::Map<String, serde_json::Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), serde_json::json!(v))).collect()
    }

    /// A laris-settings.json as an older tray or macOS left it.
    fn saved(dir: &std::path::Path) -> std::path::PathBuf {
        let p = dir.join("laris-settings.json");
        std::fs::write(&p, r#"{"DICTATE_KEY": "cmd_r", "STT_MODEL": "mlx-community/whisper-large-v3-turbo-q4", "NOISE_REDUCTION": "on"}"#).unwrap();
        p
    }

    #[test]
    fn windows_offers_exactly_f5_to_f15_for_dictation() {
        let keys = dictate_keys(true);

        assert_eq!(keys, ["f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12", "f13", "f14", "f15"]);
        assert!(dictate_keys(false).contains(&"cmd_r"), "macOS and Linux keep their keys");
    }

    #[test]
    fn windows_rejects_a_dictate_key_outside_f5_to_f15_and_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let p = saved(tmp.path());
        let before = std::fs::read(&p).unwrap();

        let e = apply_laris_updates(&p, updates(&[("DICTATE_KEY", "shift_r")]), true).unwrap_err();

        assert!(e.contains("shift_r"), "{e}");
        assert_eq!(std::fs::read(&p).unwrap(), before);
    }

    #[test]
    fn windows_saves_f13() {
        let tmp = tempfile::tempdir().unwrap();
        let p = saved(tmp.path());

        apply_laris_updates(&p, updates(&[("DICTATE_KEY", "f13")]), true).unwrap();

        assert_eq!(settings::read_json_map(&p)["DICTATE_KEY"], "f13");
    }

    #[test]
    fn saving_f13_from_the_windows_form_keeps_stt_model_and_noise_reduction() {
        let tmp = tempfile::tempdir().unwrap();
        let p = saved(tmp.path());
        let before = settings::read_json_map(&p);

        // what the Windows form sends: no STT_MODEL, no NOISE_REDUCTION
        apply_laris_updates(&p, updates(&[("DICTATE_KEY", "f13"), ("ASSISTANT_MODE", "ptt"), ("INITIAL_PROMPT", "")]), true).unwrap();

        let after = settings::read_json_map(&p);
        assert_eq!(after["STT_MODEL"], before["STT_MODEL"]);
        assert_eq!(after["NOISE_REDUCTION"], before["NOISE_REDUCTION"]);
    }

    #[test]
    fn windows_refuses_stt_model_or_noise_reduction_from_any_caller() {
        let tmp = tempfile::tempdir().unwrap();
        let p = saved(tmp.path());
        let before = std::fs::read(&p).unwrap();

        let stt = apply_laris_updates(&p, updates(&[("DICTATE_KEY", "f13"), ("STT_MODEL", "turbo")]), true);
        let noise = apply_laris_updates(&p, updates(&[("NOISE_REDUCTION", "off")]), true);

        assert_eq!(stt, Err("unsupported on Windows: STT_MODEL".to_owned()));
        assert_eq!(noise, Err("unsupported on Windows: NOISE_REDUCTION".to_owned()));
        assert_eq!(std::fs::read(&p).unwrap(), before);
        assert!(apply_laris_updates(&p, updates(&[("STT_MODEL", "turbo")]), false).is_ok(), "other platforms keep it");
    }

    #[test]
    fn windows_refuses_the_meetings_config_save() {
        assert_eq!(miris_config_save(true), Err("unsupported on Windows: vault_root, llm_command, categories".to_owned()));
        assert_eq!(miris_config_save(false), Ok(()));
    }

    #[test]
    fn windows_accepts_dictate_key_assistant_mode_and_initial_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        let p = saved(tmp.path());

        apply_laris_updates(&p, updates(&[("DICTATE_KEY", "f7"), ("ASSISTANT_MODE", "always_on"), ("INITIAL_PROMPT", "AXON")]), true).unwrap();

        let m = settings::read_json_map(&p);
        assert_eq!((m["DICTATE_KEY"].as_str(), m["ASSISTANT_MODE"].as_str(), m["INITIAL_PROMPT"].as_str()), (Some("f7"), Some("always_on"), Some("AXON")));
    }
}
