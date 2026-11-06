//! First-run model download on Windows: the tray runs `meetrec models fetch`,
//! shows its progress, and keeps dictation off until the models are there.

use crate::laris::LarisDaemon;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelState {
    /// found or downloaded; also every platform that does not fetch
    #[default]
    Ready,
    Downloading(u8),
    Failed,
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
