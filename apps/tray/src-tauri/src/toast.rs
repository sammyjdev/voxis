//! Windows notifications under VOXIS's own AppUserModelID, plus the live
//! "Gravando reunião" toast that sits in the notification center while a
//! meeting records and leaves it when the recording ends.
//!
//! A build run from target\release is not an installed app, so Windows only
//! shows its toasts once the id is registered under
//! HKCU\Software\Classes\AppUserModelId (per user, no admin).

use tauri_winrt_notification::{Progress, Toast};

pub const AUMID: &str = "dev.sammyj.voxis";
const REC_TAG: &str = "voxis-rec";

/// Idempotent: DisplayName and icon the notification center shows.
pub fn register() {
    let icon = dirs::data_local_dir().unwrap_or_default().join("voxis").join("voxis.png");
    if let Some(dir) = icon.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&icon, include_bytes!("../icons/128x128.png"));
    let key = format!(r"HKCU\Software\Classes\AppUserModelId\{AUMID}");
    let icon = icon.to_string_lossy().into_owned();
    for (name, value) in [("DisplayName", "VOXIS"), ("IconUri", icon.as_str())] {
        let mut cmd = std::process::Command::new("reg");
        cmd.args(["add", &key, "/v", name, "/t", "REG_SZ", "/d", value, "/f"]);
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let _ = cmd.output();
    }
}

pub fn notify(body: &str) {
    let _ = Toast::new(AUMID).title("VOXIS").text1(body).show();
}

fn rec_progress(started: &str, elapsed: &str) -> Progress {
    Progress {
        tag: REC_TAG.to_owned(),
        title: "Tempo de gravação".to_owned(),
        status: format!("desde {started}"),
        value: 0.0,
        value_string: elapsed.to_owned(),
    }
}

/// Pops once, then stays in the notification center.
pub fn recording_started(started: &str, elapsed: &str) {
    let _ = Toast::new(AUMID)
        .title("Gravando reunião")
        .text1("Pare pelo ícone do VOXIS na bandeja")
        .progress(&rec_progress(started, elapsed))
        .show();
}

/// Silent in-place update of the clock.
pub fn recording_tick(started: &str, elapsed: &str) {
    let _ = Toast::new(AUMID).set_progress(&rec_progress(started, elapsed));
}

pub fn recording_ended() {
    use windows::core::HSTRING;
    use windows::UI::Notifications::ToastNotificationManager;
    if let Ok(history) = ToastNotificationManager::History() {
        let _ = history.RemoveGroupedTagWithId(&HSTRING::from(REC_TAG), &HSTRING::new(), &HSTRING::from(AUMID));
    }
}
