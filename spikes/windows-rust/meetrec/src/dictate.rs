//! Dictation (LARIS on Windows): hold the dictate key to talk, release to
//! paste; double-tap for hands-free (each utterance pasted as it ends), any
//! press turns hands-free off. LARIS's orb shows the state.
//!
//!   meetrec dictate
//!
//! Settings (~/.config/voice-at/laris-settings.json, written by the tray):
//!   DICTATE_KEY     f5..f15, default f13 (the Keychron mic key)
//!   INITIAL_PROMPT  vocabulary prompt; unset = the dictation vocabulary
//!   ASSISTANT_MODE  "always_on" starts in hands-free
//! Mic: `mic` in %APPDATA%\voxis\config.toml, else the default input.
//! Orb theme: ~/.config/voice-at/orb-theme.json (the tray's Orb settings).
//!
//! The orb is LARIS's own web page (packages/laris/frontend/overlay.html,
//! three.js themes) in a transparent, click-through WebView2 window; this
//! process serves it, its /theme and the /ws/voice phase + amplitude stream,
//! like laris/overlay_server.py. The tray finds it through
//! ~/.config/voice-at/peers.json and exits it with VOXIS_PARENT_PID.
//! Log: ~/.voxis/logs/dictate-<date>.log.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, StreamTrait};
use serde_json::{json, Value};
use tao::dpi::{LogicalPosition, LogicalSize};
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::platform::windows::WindowBuilderExtWindows;
use tao::window::{Window, WindowBuilder};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperVadParams};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, KBDLLHOOKSTRUCT, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
    WM_SYSKEYUP,
};

const RATE: u32 = 16_000;
const PREROLL_MS: u32 = 200; // LARIS PREROLL_MS
const TAP: Duration = Duration::from_millis(250);
const DOUBLE_TAP: Duration = Duration::from_millis(450);
/// hands-free: this much silence closes an utterance (LARIS uses 400 ms with
/// Silero; an energy gate needs more slack)
const END_SILENCE_MS: u32 = 700;
const MIN_SPEECH_MS: u32 = 400;
const MAX_UTTERANCE_S: u32 = 30;
/// bench.py VOCABULARY_PROMPT, the list that scored best on the owner's voice
const DEFAULT_PROMPT: &str = "LARIS, VOXIS, MIRIS, tray, hands-free, orb, sink, diff, commit, push, pull request, \
master, README, BACKLOG, REST, JSON, venv, pytest, cargo test, Codex, Claude, faster-whisper, Obsidian, vault.";

// ---------------------------------------------------------------- logging

fn log_line(msg: &str) {
    let dir = super::home().join(".voxis").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let now = chrono::Local::now();
    let path = dir.join(format!("dictate-{}.log", now.format("%Y-%m-%d")));
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{} {msg}", now.format("%H:%M:%S%.3f"));
    }
    println!("{msg}");
}

// ---------------------------------------------------------------- orb state

#[derive(Clone, Copy, PartialEq)]
#[repr(u8)]
enum Show {
    Hidden = 0,
    Recording = 1,
    Transcribing = 2,
    Listening = 3, // hands-free, waiting for speech
    Hearing = 4,   // hands-free, speech in progress
}

enum Ui {
    Visible(bool),
    Theme,
}

static SHOWING: AtomicU8 = AtomicU8::new(0);
static PROXY: OnceLock<Mutex<EventLoopProxy<Ui>>> = OnceLock::new();
static CLIENTS: Mutex<Vec<Sender<String>>> = Mutex::new(Vec::new());

fn ui(event: Ui) {
    if let Some(p) = PROXY.get() {
        let _ = p.lock().unwrap().send_event(event);
    }
}

/// One VoiceEvent {phase, amplitude} to every connected orb page.
fn emit(phase: &str, amplitude: f32) {
    let msg = json!({"phase": phase, "amplitude": amplitude}).to_string();
    CLIENTS.lock().unwrap().retain(|tx| tx.send(msg.clone()).is_ok());
}

fn show(s: Show) {
    if SHOWING.swap(s as u8, Ordering::SeqCst) == s as u8 {
        return;
    }
    match s {
        Show::Hidden => emit("idle", 0.0),
        Show::Transcribing => emit("transcribing", 0.3),
        // LARIS sends hands-free listening as "recording" at 0.15
        Show::Listening => emit("recording", 0.15),
        Show::Recording | Show::Hearing => emit("recording", 0.2),
    }
    ui(Ui::Visible(s != Show::Hidden));
}

/// laris/amplitude.py: sqrt lifts conversational RMS so the orb tracks speech
fn amplitude(chunk: &[f32]) -> f32 {
    let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len().max(1) as f32).sqrt();
    (rms * 4.0).sqrt().min(1.0)
}

// ------------------------------------------------- orb theme (laris/orb_theme.py)

const ARCHETYPES: [&str; 3] = ["waveform", "reactor", "faceted"];
const POSITIONS: [&str; 7] = ["top-left", "top-center", "top-right", "bottom-left", "bottom-right", "center", "custom"];

fn palettes() -> Value {
    json!({
        "blade": {"accent": "#3fb6c4", "hot": "#d8f4f7"},
        "noir": {"accent": "#c94f92", "hot": "#f7d8ea"},
        "acid": {"accent": "#79c94f", "hot": "#e9f7d8"},
        "ember": {"accent": "#cf9b3a", "hot": "#f7ecd8"},
    })
}

fn theme_path() -> std::path::PathBuf {
    super::home().join(".config").join("voice-at").join("orb-theme.json")
}

fn theme_defaults() -> Value {
    json!({"archetype": "waveform", "palette": "blade", "position": "center", "custom": [0, 0], "size": 190})
}

fn theme_valid(t: &Value) -> Result<(), String> {
    let s = |k: &str| t[k].as_str().unwrap_or("");
    if !ARCHETYPES.contains(&s("archetype")) {
        return Err(format!("archetype must be one of {ARCHETYPES:?}"));
    }
    if palettes().get(s("palette")).is_none() {
        return Err("palette must be one of blade, noir, acid, ember".into());
    }
    if !POSITIONS.contains(&s("position")) {
        return Err(format!("position must be one of {POSITIONS:?}"));
    }
    if !matches!(t["size"].as_u64(), Some(80..=400)) {
        return Err("size must be an int in (80, 400)".into());
    }
    match t["custom"].as_array() {
        Some(a) if a.len() == 2 && a.iter().all(Value::is_number) => Ok(()),
        _ => Err("custom must be [x, y]".into()),
    }
}

fn merge(base: &mut Value, over: &Value) {
    if let (Some(b), Some(o)) = (base.as_object_mut(), over.as_object()) {
        for (k, v) in o {
            b.insert(k.clone(), v.clone());
        }
    }
}

fn load_theme(path: &std::path::Path) -> Value {
    let mut theme = theme_defaults();
    if let Some(stored) = std::fs::read_to_string(path).ok().and_then(|r| serde_json::from_str::<Value>(&r).ok()) {
        merge(&mut theme, &stored);
    }
    if theme_valid(&theme).is_ok() {
        theme
    } else {
        theme_defaults()
    }
}

fn save_theme(path: &std::path::Path, updates: &Value) -> Result<Value, String> {
    let known = theme_defaults();
    if let Some(bad) = updates.as_object().into_iter().flatten().map(|(k, _)| k).find(|k| known.get(k.as_str()).is_none()) {
        return Err(format!("unknown key: {bad}"));
    }
    let mut theme = load_theme(path);
    merge(&mut theme, updates);
    theme_valid(&theme)?;
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&theme).unwrap()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(theme)
}

/// Top-left of the orb window inside a work area (x, y, w, h), logical px.
fn orb_position(theme: &Value, area: (f64, f64, f64, f64)) -> (f64, f64) {
    let (ax, ay, w, h) = area;
    let size = theme["size"].as_f64().unwrap_or(190.0);
    let pos = theme["position"].as_str().unwrap_or("center");
    if pos == "custom" {
        return (theme["custom"][0].as_f64().unwrap_or(0.0), theme["custom"][1].as_f64().unwrap_or(0.0));
    }
    let x = if pos.ends_with("left") {
        16.0
    } else if pos.ends_with("right") {
        w - size - 16.0
    } else {
        (w - size) / 2.0
    };
    let y = if pos.starts_with("top") {
        40.0
    } else if pos.starts_with("bottom") {
        h - size - 16.0
    } else {
        (h - size) / 2.0
    };
    (ax + x, ay + y)
}

// ---------------------------------------------------------------- keys

enum Key {
    Down(Instant),
    Up(Instant),
}

static KEYS: OnceLock<(u32, Sender<Key>)> = OnceLock::new();

unsafe extern "system" fn hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
        if let Some((vk, tx)) = KEYS.get() {
            if kb.vkCode == *vk {
                let now = Instant::now();
                match wparam as u32 {
                    WM_KEYDOWN | WM_SYSKEYDOWN => drop(tx.send(Key::Down(now))),
                    WM_KEYUP | WM_SYSKEYUP => drop(tx.send(Key::Up(now))),
                    _ => {}
                }
                return 1; // swallowed: no app needs to see the dictate key
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

fn key_vk(name: &str) -> Option<u32> {
    let n: u32 = name.strip_prefix('f')?.parse().ok()?;
    (5..=15).contains(&n).then(|| 0x70 + n - 1) // VK_F1 = 0x70
}

// ---------------------------------------------------------------- audio

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Idle,
    Ptt,
    HandsFree,
}

struct Capture {
    mode: Mode,
    rate: u32,
    buf: Vec<f32>,
    pre: VecDeque<f32>,
    // hands-free energy gate, in device samples
    speaking: bool,
    speech_len: usize,
    silence_len: usize,
    floor: f32,
}

impl Capture {
    fn ms(&self, ms: u32) -> usize {
        (self.rate * ms / 1000) as usize
    }

    fn keep_preroll(&mut self, chunk: &[f32]) {
        self.pre.extend(chunk);
        let excess = self.pre.len().saturating_sub(self.ms(PREROLL_MS));
        self.pre.drain(..excess);
    }

    /// One mic callback. Returns a finished hands-free utterance.
    fn push(&mut self, chunk: &[f32]) -> Option<Vec<f32>> {
        match self.mode {
            Mode::Idle => {
                self.keep_preroll(chunk);
                None
            }
            Mode::Ptt => {
                self.buf.extend(chunk);
                None
            }
            Mode::HandsFree => {
                let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len().max(1) as f32).sqrt();
                let gate = (self.floor * 4.0).max(0.012);
                if !self.speaking {
                    self.keep_preroll(chunk);
                    if rms > gate {
                        self.speaking = true;
                        self.speech_len = chunk.len();
                        self.silence_len = 0;
                        self.buf = self.pre.drain(..).collect();
                        show(Show::Hearing);
                    } else {
                        self.floor = self.floor * 0.97 + rms * 0.03;
                    }
                    return None;
                }
                self.buf.extend(chunk);
                if rms > gate {
                    self.speech_len += chunk.len() + self.silence_len;
                    self.silence_len = 0;
                } else {
                    self.silence_len += chunk.len();
                }
                let ended = self.silence_len >= self.ms(END_SILENCE_MS);
                let too_long = self.buf.len() >= self.rate as usize * MAX_UTTERANCE_S as usize;
                if ended || too_long {
                    self.speaking = false;
                    show(Show::Listening);
                    let utterance = std::mem::take(&mut self.buf);
                    if self.speech_len >= self.ms(MIN_SPEECH_MS) {
                        return Some(utterance);
                    }
                }
                None
            }
        }
    }

    fn start(&mut self, mode: Mode) {
        self.buf = if mode == Mode::Ptt { self.pre.drain(..).collect() } else { Vec::new() };
        self.speaking = false;
        self.mode = mode;
    }

    fn stop(&mut self) -> Vec<f32> {
        self.mode = Mode::Idle;
        self.speaking = false;
        std::mem::take(&mut self.buf)
    }
}

fn to_16k(mono: Vec<f32>, rate: u32) -> Vec<f32> {
    if rate == RATE || mono.is_empty() {
        return mono;
    }
    let ratio = rate as f64 / RATE as f64;
    let k = ratio.floor().max(1.0) as usize;
    let smooth: Vec<f32> = (0..mono.len())
        .map(|i| {
            let lo = i.saturating_sub(k / 2);
            let hi = (lo + k).min(mono.len());
            mono[lo..hi].iter().sum::<f32>() / (hi - lo) as f32
        })
        .collect();
    let n = (mono.len() as f64 / ratio) as usize;
    (0..n)
        .map(|i| {
            let x = i as f64 * ratio;
            let j = x as usize;
            let f = (x - j as f64) as f32;
            smooth[j] * (1.0 - f) + smooth.get(j + 1).copied().unwrap_or(smooth[j]) * f
        })
        .collect()
}

// ---------------------------------------------------------------- paste

/// Ctrl+V into the focused window; the number of events SendInput injected.
fn ctrl_v() -> u32 {
    let key = |vk: u16, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { 0 }, time: 0, dwExtraInfo: 0 },
        },
    };
    let seq = [key(VK_CONTROL, false), key(0x56, false), key(0x56, true), key(VK_CONTROL, true)];
    unsafe { SendInput(seq.len() as u32, seq.as_ptr(), std::mem::size_of::<INPUT>() as i32) }
}

/// The text clipboard, as paste uses it.
trait Clip {
    fn get(&mut self) -> Option<String>;
    fn set(&mut self, text: &str) -> Result<(), String>;
}

impl Clip for arboard::Clipboard {
    fn get(&mut self) -> Option<String> {
        self.get_text().ok()
    }
    fn set(&mut self, text: &str) -> Result<(), String> {
        self.set_text(text.to_owned()).map_err(|e| e.to_string())
    }
}

/// Whether the foreground window belongs to an elevated process while this
/// one is not, or to a process whose token cannot be read. Windows drops
/// keys injected into such a window without telling the sender (UIPI), so
/// the paste is skipped and the text stays on the clipboard. Best effort: a
/// window that blocks input some other way is not detected.
fn foreground_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe fn elevated(process: HANDLE) -> Option<bool> {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let mut e = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut e as *mut TOKEN_ELEVATION as *mut std::ffi::c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        CloseHandle(token);
        (ok != 0).then_some(e.TokenIsElevated != 0)
    }

    unsafe {
        let window = GetForegroundWindow();
        let mut pid = 0u32;
        if window.is_null() || GetWindowThreadProcessId(window, &mut pid) == 0 || pid == GetCurrentProcessId() {
            return false;
        }
        if elevated(GetCurrentProcess()).unwrap_or(false) {
            return false; // an elevated dictate can type anywhere
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return true;
        }
        let target = elevated(process);
        CloseHandle(process);
        target.unwrap_or(true)
    }
}

/// How one paste ended. Only `Pasted` carries timestamps: a failed, skipped
/// or empty paste has no completion time to report.
#[derive(Debug, Clone, PartialEq)]
enum PasteOutcome {
    /// Ctrl+V was sent at `injected_ms`; `restore` says what became of the
    /// user's previous clipboard text.
    Pasted { injected_ms: u128, restore: Restore },
    /// nothing was transcribed, so nothing was pasted
    Empty,
    /// the clipboard could not be opened or set (AC 42)
    ClipboardFailed(String),
    /// `SendInput` reported fewer than 4 events (AC 41)
    InjectFailed(u32),
    /// the target window is elevated; the text stays on the clipboard
    SkippedElevated,
}

/// What became of the clipboard text the user had before a paste.
#[derive(Debug, Clone, PartialEq)]
enum Restore {
    /// the previous text was put back at this unix ms, after the restore wait
    Done(u128),
    /// the clipboard held no text before, so there was nothing to put back
    NoPrevious,
    /// putting it back failed: the dictated text is still on the clipboard
    Failed(String),
}

impl PasteOutcome {
    /// The log lines of this outcome (never the text).
    fn log_lines(&self) -> Vec<String> {
        match self {
            Self::Pasted { restore: Restore::Failed(e), .. } => vec![format!("paste restore failed: clipboard {e}")],
            Self::Pasted { .. } | Self::Empty => Vec::new(),
            Self::ClipboardFailed(e) => vec![format!("paste failed: clipboard {e}")],
            Self::InjectFailed(n) => vec![format!("paste failed: injected {n} of 4")],
            Self::SkippedElevated => vec!["paste skipped: elevated target".to_owned()],
        }
    }
}

/// Clipboard + Ctrl+V, then the user's clipboard text comes back. `now` reads
/// the clock: once right after Ctrl+V was sent, once after the restore.
fn paste_outcome_with(
    clip: Result<&mut dyn Clip, String>,
    text: &str,
    inject: &mut dyn FnMut() -> u32,
    elevated: bool,
    wait: Duration,
    now: &mut dyn FnMut() -> u128,
) -> PasteOutcome {
    let cb = match clip {
        Ok(cb) => cb,
        Err(e) => return PasteOutcome::ClipboardFailed(e),
    };
    let previous = cb.get();
    if let Err(e) = cb.set(text) {
        return PasteOutcome::ClipboardFailed(e);
    }
    if elevated {
        // the text stays on the clipboard for the user to paste by hand
        return PasteOutcome::SkippedElevated;
    }
    let injected = inject();
    if injected < 4 {
        return PasteOutcome::InjectFailed(injected);
    }
    let injected_ms = now();
    std::thread::sleep(wait);
    let restore = match previous {
        Some(p) => match cb.set(&p) {
            Ok(()) => Restore::Done(now()),
            Err(e) => Restore::Failed(e),
        },
        None => Restore::NoPrevious,
    };
    PasteOutcome::Pasted { injected_ms, restore }
}

/// `paste_outcome_with`, reduced to its log lines.
#[cfg(test)]
fn paste_with(clip: Result<&mut dyn Clip, String>, text: &str, inject: &mut dyn FnMut() -> u32, elevated: bool, wait: Duration) -> Vec<String> {
    paste_outcome_with(clip, text, inject, elevated, wait, &mut unix_ms).log_lines()
}

fn paste(text: &str) -> PasteOutcome {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string());
    let clip: Result<&mut dyn Clip, String> = match cb.as_mut() {
        Ok(c) => Ok(c),
        Err(e) => Err(e.clone()),
    };
    let outcome = paste_outcome_with(clip, text, &mut ctrl_v, foreground_elevated(), Duration::from_millis(250), &mut unix_ms);
    for line in outcome.log_lines() {
        log_line(&line);
    }
    outcome
}

/// Whether the orb goes back to hidden after a paste. A clipboard failure
/// hides it in both modes (AC 42); otherwise only a push-to-talk utterance
/// with nothing else being captured does.
fn hide_orb_after(outcome: &PasteOutcome, hands_free: bool, idle: bool) -> bool {
    matches!(outcome, PasteOutcome::ClipboardFailed(_) | PasteOutcome::Pasted { restore: Restore::Failed(_), .. })
        || (!hands_free && idle)
}

/// The log line of one transcribed utterance.
/// Counts and timings only: the transcript text is on screen where it was
/// pasted and never goes to a log (dictate-<date>.log, stdout, laris.log).
/// `release_ms` (key release or utterance end) and `paste_ms` (paste done)
/// time release-to-paste.
fn utterance_log(mode: &str, audio_s: f64, proc_s: f64, text: &str, release_ms: u128, paste_ms: u128) -> String {
    format!(
        "{mode} {audio_s:.1} s -> {proc_s:.2} s, {} chars, release_ms={release_ms} paste_ms={paste_ms}",
        text.chars().count()
    )
}

/// The log line of an utterance with its paste outcome: `paste_ms` is when
/// Ctrl+V was sent and `restore_ms` when the clipboard came back, both only
/// for a paste that happened; otherwise `paste=<why not>`.
fn utterance_line(mode: &str, audio_s: f64, proc_s: f64, text: &str, release_ms: u128, outcome: &PasteOutcome) -> String {
    match outcome {
        PasteOutcome::Pasted { injected_ms, restore } => {
            let line = utterance_log(mode, audio_s, proc_s, text, release_ms, *injected_ms);
            match restore {
                Restore::Done(restored_ms) => format!("{line} restore_ms={restored_ms}"),
                Restore::NoPrevious => format!("{line} restore=none"),
                Restore::Failed(_) => format!("{line} restore=failed"),
            }
        }
        other => {
            let why = match other {
                PasteOutcome::Empty => "empty",
                PasteOutcome::ClipboardFailed(_) => "clipboard_failed",
                PasteOutcome::InjectFailed(_) => "inject_failed",
                _ => "skipped_elevated",
            };
            format!("{mode} {audio_s:.1} s -> {proc_s:.2} s, {} chars, release_ms={release_ms} paste={why}", text.chars().count())
        }
    }
}

fn unix_ms() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

/// A dictation key gesture: what a press or release asks for.
#[derive(Debug, PartialEq)]
enum Gesture {
    /// push-to-talk starts recording
    StartPtt,
    /// a press during hands-free turns it off
    HandsFreeOff,
    /// the second short tap turns hands-free on
    HandsFreeOn,
    /// a held key released with enough audio: transcribe and paste
    Transcribe,
    /// nothing to paste (a short tap, too little audio)
    Hide,
    /// autorepeat, or the release of the press that ended hands-free
    Ignore,
}

/// The key state machine: hold to talk, double-tap for hands-free.
struct Gestures {
    held: bool,
    pressed_at: Option<Instant>,
    last_tap: Option<Instant>,
    swallow_release: bool,
}

impl Gestures {
    fn new() -> Self {
        Self { held: false, pressed_at: None, last_tap: None, swallow_release: false }
    }

    fn down(&mut self, at: Instant, hands_free: bool) -> Gesture {
        if self.held {
            return Gesture::Ignore; // autorepeat
        }
        self.held = true;
        self.pressed_at = Some(at);
        if hands_free {
            self.swallow_release = true;
            self.last_tap = None;
            Gesture::HandsFreeOff
        } else {
            Gesture::StartPtt
        }
    }

    /// `captured` samples at `rate` were recorded while the key was down.
    fn up(&mut self, at: Instant, captured: usize, rate: u32) -> Gesture {
        self.held = false;
        if std::mem::take(&mut self.swallow_release) {
            return Gesture::Ignore;
        }
        let tap = self.pressed_at.is_some_and(|p| at.duration_since(p) < TAP);
        if tap && self.last_tap.is_some_and(|t| at.duration_since(t) < DOUBLE_TAP) {
            self.last_tap = None;
            Gesture::HandsFreeOn
        } else if tap {
            self.last_tap = Some(at);
            Gesture::Hide
        } else if captured > rate as usize * 3 / 10 {
            Gesture::Transcribe
        } else {
            Gesture::Hide
        }
    }
}

/// Speech a key press cut in the middle of a hands-free utterance, when it
/// is long enough to paste.
fn cut_utterance(c: &mut Capture) -> Option<Vec<f32>> {
    let cut_speech = c.speaking && c.speech_len >= c.ms(MIN_SPEECH_MS);
    let rest = c.stop();
    cut_speech.then_some(rest)
}

/// The text a paste types: one trailing space so dictations do not glue.
fn paste_text(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| format!("{text} "))
}

/// Exit code of a dictate whose input device went away: the tray does not
/// respawn it.
pub const EXIT_DEVICE_LOST: i32 = 3;

/// The log line and exit code of an input device that cannot be used when
/// dictate starts (none, or it went away before it opened): the same as one
/// lost while running, so the tray shows the click-to-restart row and does
/// not respawn it.
fn mic_start_failure(e: &str) -> (String, i32) {
    (format!("microfone desconectado: {e}"), EXIT_DEVICE_LOST)
}

/// Load the model, make its state and run the warm-up pass; any step that
/// fails is `modelo não carregou: <path>: <error>` (AC 45), never a panic.
fn prepare_model<C, S, E1: std::fmt::Display, E2: std::fmt::Display, E3: std::fmt::Display>(
    path: &std::path::Path,
    load: impl FnOnce() -> Result<C, E1>,
    make_state: impl FnOnce(&C) -> Result<S, E2>,
    warm: impl FnOnce(&mut S) -> Result<(), E3>,
) -> Result<(C, S), String> {
    let ctx = load_model(path, load)?;
    let mut state = load_model(path, || make_state(&ctx))?;
    load_model(path, || warm(&mut state))?;
    Ok((ctx, state))
}

/// The log line and exit code for an input stream error.
fn stream_lost(e: &str) -> (String, Option<i32>) {
    (format!("microfone desconectado: {e}"), Some(EXIT_DEVICE_LOST))
}

/// The input formats dictate converts.
fn sample_format_ok(f: cpal::SampleFormat) -> Result<(), String> {
    match f {
        cpal::SampleFormat::F32 | cpal::SampleFormat::I16 => Ok(()),
        f => Err(format!("formato de áudio não suportado: {f}")),
    }
}

/// A loaded model, or the log line that says why not.
fn load_model<T, E: std::fmt::Display>(path: &std::path::Path, load: impl FnOnce() -> Result<T, E>) -> Result<T, String> {
    load().map_err(|e| format!("modelo não carregou: {}: {e}", path.display()))
}

/// The orb, or the log line that says dictation runs without it.
fn orb_or_log<T, E: std::fmt::Display>(made: Result<T, E>, log: &mut dyn FnMut(&str)) -> Option<T> {
    made.map_err(|e| log(&format!("overlay unavailable: {e}"))).ok()
}

/// What the ggml device list says about the GPU: the log line when
/// whisper.cpp has only the CPU to run on.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Device {
    Cpu,
    Gpu,
}

fn gpu_note(devices: &[Device]) -> Option<&'static str> {
    (!devices.contains(&Device::Gpu)).then_some("sem GPU: usando CPU")
}

/// What whisper.cpp itself said about the backend it chose, read from its
/// own `whisper_backend_init_gpu` log lines (criterion 46, 47).
#[derive(Debug, Clone, PartialEq)]
enum BackendResult {
    /// `using <name> backend`: CUDA0, Vulkan0, or CPU
    Named(String),
    /// `no GPU found`, or a backend that failed to initialize
    CpuFallback,
}

fn observe_backend(line: &str) -> Option<BackendResult> {
    if let Some(rest) = line.split("using ").nth(1) {
        if let Some(name) = rest.strip_suffix(" backend").or_else(|| rest.trim_end().strip_suffix(" backend")) {
            if !name.is_empty() && !name.contains(' ') {
                return Some(BackendResult::Named(name.to_owned()));
            }
        }
    }
    let lower = line.to_lowercase();
    let failed_init = lower.find("failed to initialize").is_some_and(|i| lower[i..].contains("backend"));
    (lower.contains("no gpu found") || failed_init).then_some(BackendResult::CpuFallback)
}

/// The log line `sem GPU: usando CPU` when the GPU did not come up. The
/// backend whisper.cpp reported decides; with no report, the devices ggml
/// registered do.
fn backend_note(devices: &[Device], observed: Option<&BackendResult>) -> Option<&'static str> {
    match observed {
        Some(BackendResult::CpuFallback) => Some("sem GPU: usando CPU"),
        Some(BackendResult::Named(name)) if name.to_uppercase().starts_with("CPU") => Some("sem GPU: usando CPU"),
        Some(BackendResult::Named(_)) => None,
        None => gpu_note(devices),
    }
}

static BACKEND: Mutex<Option<BackendResult>> = Mutex::new(None);
static LOG_TAIL: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// One line of whisper.cpp's own log: remembered when it names the backend,
/// and kept in a short tail.
fn note_whisper_log(line: &str) {
    if let Some(found) = observe_backend(line) {
        if let Ok(mut b) = BACKEND.lock() {
            *b = Some(found);
        }
    }
    if let Ok(mut tail) = LOG_TAIL.lock() {
        if tail.len() >= 64 {
            tail.pop_front();
        }
        tail.push_back(line.trim_end().to_owned());
    }
}

fn observed_backend() -> Option<BackendResult> {
    BACKEND.lock().ok().and_then(|b| b.clone())
}

/// whisper.cpp's log callback: the line goes to stderr as before (it reaches
/// laris.log there) and through `note_whisper_log`.
unsafe extern "C" fn whisper_log(_level: std::ffi::c_int, text: *const std::ffi::c_char, _user: *mut std::ffi::c_void) {
    if text.is_null() {
        return;
    }
    let line = std::ffi::CStr::from_ptr(text).to_string_lossy().into_owned();
    eprint!("{line}");
    note_whisper_log(&line);
}

fn install_whisper_log() {
    // SAFETY: the callback is a plain function that never unwinds and keeps no pointer to `text`
    unsafe { whisper_log_set(Some(whisper_log), std::ptr::null_mut()) }
}

// ggml's device registry, linked from whisper-rs-sys's static ggml; declared
// here instead of enabling whisper-rs `raw-api`, which would change the
// engine's features (plan criterion 84)
extern "C" {
    fn whisper_log_set(
        callback: Option<unsafe extern "C" fn(std::ffi::c_int, *const std::ffi::c_char, *mut std::ffi::c_void)>,
        user_data: *mut std::ffi::c_void,
    );
    fn ggml_backend_dev_count() -> usize;
    fn ggml_backend_dev_get(index: usize) -> *mut std::ffi::c_void;
    fn ggml_backend_dev_type(device: *mut std::ffi::c_void) -> std::ffi::c_int;
}

/// The ggml devices this build registered (ggml-backend.h: 0 CPU, 1 GPU,
/// 2 integrated GPU, 3 accelerator).
fn ggml_devices() -> Vec<Device> {
    unsafe {
        (0..ggml_backend_dev_count())
            .map(|i| match ggml_backend_dev_type(ggml_backend_dev_get(i)) {
                1 | 2 => Device::Gpu,
                _ => Device::Cpu,
            })
            .collect()
    }
}

// ---------------------------------------------------------------- server

static READY: AtomicBool = AtomicBool::new(false);
static DICTATING: AtomicBool = AtomicBool::new(false);

/// packages/laris/frontend: VOXIS_FRONTEND, else this checkout's copy.
fn frontend_dir() -> std::path::PathBuf {
    std::env::var_os("VOXIS_FRONTEND")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| concat!(env!("CARGO_MANIFEST_DIR"), "/../../../packages/laris/frontend").into())
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        _ => "application/octet-stream",
    }
}

fn respond(stream: &mut std::net::TcpStream, status: &str, ctype: &str, body: &[u8]) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

fn json_response(stream: &mut std::net::TcpStream, status: &str, v: &Value) {
    respond(stream, status, "application/json", v.to_string().as_bytes());
}

/// /ws/voice: VoiceEvents until the page goes away (a ping every 5 s finds out).
fn serve_ws(stream: std::net::TcpStream) {
    let Ok(mut ws) = tungstenite::accept(stream) else { return };
    let (tx, rx) = mpsc::channel::<String>();
    // a page that (re)connects mid-dictation starts from the current state
    let phase = if SHOWING.load(Ordering::SeqCst) == Show::Hidden as u8 { "idle" } else { "recording" };
    let _ = tx.send(json!({"phase": phase, "amplitude": 0.15}).to_string());
    CLIENTS.lock().unwrap().push(tx);
    loop {
        let sent = match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(msg) => ws.send(tungstenite::Message::Text(msg.into())),
            Err(mpsc::RecvTimeoutError::Timeout) => ws.send(tungstenite::Message::Ping(Vec::new().into())),
            Err(_) => break,
        };
        if sent.is_err() {
            break;
        }
    }
}

/// Where `/static/<rel>` may be read from: a file under the frontend root.
#[derive(Debug, PartialEq)]
enum StaticErr {
    BadPath,
    NotFound,
}

/// Names Windows maps to a device in any folder: `NUL.txt` is NUL too.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// `%XX` decoded; None for a bad escape or bytes that are not UTF-8.
fn percent_decode(raw: &str) -> Option<String> {
    let b = raw.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The file `/static/<rel>` names, only when it lies under the canonical
/// frontend root. Every segment, split on both separators after decoding,
/// must be a plain name: no empty segment (rooted or UNC), no `.`/`..`, no
/// `:` (a drive or an alternate data stream), no reserved device name. A
/// link or junction that leaves the root is not found.
fn static_file(root: &std::path::Path, rel: &str) -> Result<std::path::PathBuf, StaticErr> {
    let decoded = percent_decode(rel).ok_or(StaticErr::BadPath)?;
    let mut path = root.to_path_buf();
    for seg in decoded.split(['/', '\\']) {
        let stem = seg.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
        if seg.is_empty() || seg == "." || seg == ".." || seg.contains(':') || RESERVED.contains(&stem.as_str()) {
            return Err(StaticErr::BadPath);
        }
        path.push(seg);
    }
    let root = std::fs::canonicalize(root).map_err(|_| StaticErr::NotFound)?;
    let target = std::fs::canonicalize(&path).map_err(|_| StaticErr::NotFound)?;
    if target.starts_with(&root) && target.is_file() {
        Ok(target)
    } else {
        Err(StaticErr::NotFound)
    }
}

fn serve_http(mut stream: std::net::TcpStream, token: &str, frontend: &std::path::Path, theme: &std::path::Path) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let mut length = 0usize;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() {
            break;
        }
        if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
            length = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; length.min(64 * 1024)];
    let _ = std::io::Read::read_exact(&mut reader, &mut body);
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let path = target.split('?').next().unwrap_or("");
    let authorized = target.contains(&format!("token={token}"));
    match (method, path) {
        ("GET", "/overlay") => match std::fs::read(frontend.join("overlay.html")) {
            Ok(b) => respond(&mut stream, "200 OK", content_type("x.html"), &b),
            Err(_) => respond(&mut stream, "404 Not Found", "text/plain", b"frontend not found"),
        },
        ("GET", p) if p.starts_with("/static/") => {
            let rel = &p["/static/".len()..];
            match static_file(frontend, rel).map(std::fs::read) {
                Ok(Ok(b)) => respond(&mut stream, "200 OK", content_type(rel), &b),
                Err(StaticErr::BadPath) => respond(&mut stream, "400 Bad Request", "text/plain", b"bad path"),
                _ => respond(&mut stream, "404 Not Found", "text/plain", b"not found"),
            }
        }
        (_, "/theme") if !authorized => json_response(&mut stream, "403 Forbidden", &json!({"detail": "invalid token"})),
        ("GET", "/theme") => json_response(
            &mut stream,
            "200 OK",
            &json!({
                "theme": load_theme(theme), "palettes": palettes(), "ptt_trusted": true,
                "ready": READY.load(Ordering::SeqCst), "dictating": DICTATING.load(Ordering::SeqCst),
            }),
        ),
        ("POST", "/theme") => {
            let updates: Value = serde_json::from_slice(&body).unwrap_or_default();
            match save_theme(theme, &updates) {
                Ok(theme) => {
                    ui(Ui::Theme);
                    json_response(&mut stream, "200 OK", &json!({"theme": theme, "palettes": palettes()}))
                }
                Err(e) => json_response(&mut stream, "422 Unprocessable Entity", &json!({"detail": e})),
            }
        }
        _ => json_response(&mut stream, "404 Not Found", &json!({"detail": "not found"})),
    }
}

/// LARIS's overlay server: the orb page, its theme, the voice websocket.
/// peers.json tells the tray where it is.
fn serve() -> (String, String) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let token = format!(
        "{:016x}{:016x}",
        std::process::id() as u64 * 0x9E37_79B9,
        chrono::Local::now().timestamp_nanos_opt().unwrap_or(0)
    );
    let peers = super::home().join(".config").join("voice-at").join("peers.json");
    let _ = std::fs::create_dir_all(peers.parent().unwrap());
    let tmp = peers.with_extension("json.tmp");
    if std::fs::write(&tmp, json!({"laris": {"url": url, "token": token}}).to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, &peers);
    }
    let tok = token.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let tok = tok.clone();
            std::thread::spawn(move || {
                let mut head = [0u8; 512];
                let n = stream.peek(&mut head).unwrap_or(0);
                let first = String::from_utf8_lossy(&head[..n]).into_owned();
                if first.starts_with("GET /ws/voice") {
                    // reject before the handshake, as laris does (no onopen, so the page backs off)
                    if first.lines().next().unwrap_or("").contains(&format!("token={tok}")) {
                        serve_ws(stream);
                    }
                } else {
                    serve_http(stream, &tok, &frontend_dir(), &theme_path());
                }
            });
        }
    });
    (url, token)
}

/// Exit with the tray: a crash or Force Quit must not leave the mic and the
/// key hook behind (LARIS's VOXIS_PARENT_PID contract).
fn watch_parent() {
    let Some(pid) = std::env::var("VOXIS_PARENT_PID").ok().and_then(|p| p.parse::<u32>().ok()) else { return };
    std::thread::spawn(move || unsafe {
        let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if !h.is_null() {
            WaitForSingleObject(h, INFINITE);
        }
        log_line("tray saiu, encerrando o ditado");
        std::process::exit(0);
    });
}

fn place(window: &Window, theme: &Value) {
    let Some(m) = window.primary_monitor() else { return };
    let scale = m.scale_factor();
    let p = m.position().to_logical::<f64>(scale);
    let s = m.size().to_logical::<f64>(scale);
    // 48 px: keep the bottom positions clear of the taskbar
    let (x, y) = orb_position(theme, (p.x, p.y, s.width, s.height - 48.0));
    let size = theme["size"].as_f64().unwrap_or(190.0);
    window.set_inner_size(LogicalSize::new(size, size));
    window.set_outer_position(LogicalPosition::new(x, y));
}

// ---------------------------------------------------------------- run

/// What laris-settings.json asks of the next dictate start.
#[derive(Debug, PartialEq)]
struct DictateSettings {
    /// f5..f15; anything else (cmd_r, shift_r) runs f13
    key_name: String,
    vk: u32,
    /// empty disables priming; unset is the dictation vocabulary
    prompt: String,
    hands_free: bool,
}

fn dictate_settings(settings: &Value) -> DictateSettings {
    let key_name = settings["DICTATE_KEY"].as_str().filter(|k| key_vk(k).is_some()).unwrap_or("f13").to_owned();
    DictateSettings {
        vk: key_vk(&key_name).unwrap(),
        key_name,
        prompt: settings["INITIAL_PROMPT"].as_str().map(str::to_owned).unwrap_or_else(|| DEFAULT_PROMPT.to_owned()),
        hands_free: settings["ASSISTANT_MODE"] == "always_on",
    }
}

pub fn run() {
    watch_parent();
    let home = super::home();
    let settings: Value = std::fs::read_to_string(home.join(".config").join("voice-at").join("laris-settings.json"))
        .ok()
        .and_then(|r| serde_json::from_str(&r).ok())
        .unwrap_or_default();
    let DictateSettings { key_name, vk, prompt, hands_free: start_hands_free } = dictate_settings(&settings);
    let (url, token) = serve(); // the tray shows "Carregando modelo..." until READY
    let (model, vad) = super::models().unwrap_or_else(|e| {
        log_line(&e);
        std::process::exit(1)
    });

    let t = Instant::now();
    install_whisper_log();
    let mut cp = WhisperContextParameters::default();
    cp.use_gpu(true);
    let vad = vad.to_string_lossy().into_owned();
    let params = |prompt: &str, vad: &str| {
        let mut p = FullParams::new(SamplingStrategy::BeamSearch { beam_size: 5, patience: -1.0 });
        p.set_language(Some(super::LANGUAGE));
        p.set_no_context(true);
        p.set_no_timestamps(true);
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_special(false);
        p.set_print_timestamps(false);
        if !prompt.is_empty() {
            p.set_initial_prompt(prompt);
        }
        // a silent clip with a prompt comes back as the prompt's words
        p.set_vad_model_path(Some(vad));
        p.enable_vad(true);
        p.set_vad_params(WhisperVadParams::new());
        p
    };
    let (_ctx, mut state) = prepare_model(
        &model,
        || WhisperContext::new_with_params(&model, cp),
        |ctx| ctx.create_state(),
        |state| state.full(params(&prompt, &vad), &vec![0.0f32; RATE as usize]).map(|_| ()),
    )
    .unwrap_or_else(|e| {
        log_line(&e);
        std::process::exit(1)
    });
    if let Some(note) = backend_note(&ggml_devices(), observed_backend().as_ref()) {
        log_line(note);
    }

    // no input device, or one that went away before it opened, is a lost microphone: exit 3
    let mic_lost = |e: &str| -> ! {
        let (line, code) = mic_start_failure(e);
        log_line(&line);
        std::process::exit(code)
    };
    let mic = super::pick_input_with(super::session::configured_mic().as_deref(), &mut |w| log_line(w))
        .unwrap_or_else(|e| mic_lost(&e));
    log_line(&format!("microfone: {}", mic.name().unwrap_or_default()));
    let cfg = mic.default_input_config().unwrap_or_else(|e| mic_lost(&e.to_string()));
    if let Err(e) = sample_format_ok(cfg.sample_format()) {
        log_line(&e);
        std::process::exit(1);
    }
    let mic_rate = cfg.sample_rate().0;
    let ch = cfg.channels() as usize;
    let cap = Arc::new(Mutex::new(Capture {
        mode: Mode::Idle,
        rate: mic_rate,
        buf: Vec::new(),
        pre: VecDeque::new(),
        speaking: false,
        speech_len: 0,
        silence_len: 0,
        floor: 0.003,
    }));
    // (audio, hands-free, key-release or utterance-end unix ms)
    let (jobs_tx, jobs_rx) = mpsc::channel::<(Vec<f32>, bool, u128)>();

    let cap2 = cap.clone();
    let jobs2 = jobs_tx.clone();
    let mut since_emit = 0usize;
    let mut push = move |mono: Vec<f32>| {
        let mut c = cap2.lock().unwrap();
        let finished = c.push(&mono);
        let listening = c.mode != Mode::Idle;
        drop(c);
        if let Some(utterance) = finished {
            let _ = jobs2.send((to_16k(utterance, mic_rate), true, unix_ms()));
        }
        since_emit += mono.len();
        // ~25 orb updates per second while the mic is open
        if listening && since_emit >= mic_rate as usize / 25 {
            since_emit = 0;
            emit("recording", amplitude(&mono));
        }
    };
    let scfg: cpal::StreamConfig = cfg.clone().into();
    let err = |e: cpal::StreamError| {
        let (line, code) = stream_lost(&e.to_string());
        log_line(&line);
        if let Some(code) = code {
            show(Show::Hidden);
            std::process::exit(code);
        }
    };
    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => mic.build_input_stream(
            &scfg,
            move |d: &[f32], _: &_| push(d.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect()),
            err,
            None,
        ),
        cpal::SampleFormat::I16 => mic.build_input_stream(
            &scfg,
            move |d: &[i16], _: &_| {
                push(d.chunks(ch).map(|f| f.iter().map(|&s| s as f32 / 32768.0).sum::<f32>() / ch as f32).collect())
            },
            err,
            None,
        ),
        f => unreachable!("checked by sample_format_ok: {f:?}"),
    }
    .unwrap_or_else(|e| mic_lost(&e.to_string()));
    stream.play().unwrap_or_else(|e| mic_lost(&e.to_string()));

    // transcription + paste, off the hook thread
    let cap_t = cap.clone();
    std::thread::spawn(move || {
        for (audio, hands_free, released) in jobs_rx {
            let secs = audio.len() as f64 / RATE as f64;
            let t = Instant::now();
            if state.full(params(&prompt, &vad), &audio).is_err() {
                continue;
            }
            let text: String = state.as_iter().map(|s| s.to_string()).collect::<String>().trim().to_owned();
            let proc_s = t.elapsed().as_secs_f64();
            // trailing space: consecutive dictations must not glue together
            let outcome = match paste_text(&text) {
                Some(typed) => paste(&typed),
                None => PasteOutcome::Empty,
            };
            log_line(&utterance_line(if hands_free { "hf" } else { "ptt" }, secs, proc_s, &text, released, &outcome));
            let idle = cap_t.lock().unwrap().mode == Mode::Idle;
            if hide_orb_after(&outcome, hands_free, idle) {
                show(Show::Hidden);
            }
        }
    });

    // gesture state machine
    let (keys_tx, keys_rx) = mpsc::channel::<Key>();
    KEYS.set((vk, keys_tx)).ok();
    let cap3 = cap.clone();
    let jobs4 = jobs_tx.clone();
    let set_hands_free = move |on: bool| {
        let mut c = cap3.lock().unwrap();
        if on {
            c.start(Mode::HandsFree);
        } else if let Some(rest) = cut_utterance(&mut c) {
            // an utterance cut by the key press still gets pasted
            let _ = jobs_tx.send((to_16k(rest, mic_rate), true, unix_ms()));
        }
        drop(c);
        DICTATING.store(on, Ordering::SeqCst);
        show(if on { Show::Listening } else { Show::Hidden });
        log_line(if on { "hands-free: ligado" } else { "hands-free: desligado" });
    };
    let cap4 = cap.clone();

    // the orb window, before anything can show it
    let event_loop = EventLoopBuilder::<Ui>::with_user_event().build();
    PROXY.set(Mutex::new(event_loop.create_proxy())).ok();
    let theme = load_theme(&theme_path());
    let size = theme["size"].as_f64().unwrap_or(190.0);
    let window = orb_or_log(WindowBuilder::new()
        .with_title("VOXIS orb")
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top(true)
        .with_skip_taskbar(true)
        .with_undecorated_shadow(false)
        .with_focusable(false)
        .with_visible(false)
        .with_resizable(false)
        .with_inner_size(LogicalSize::new(size, size))
        .build(&event_loop), &mut |l| log_line(l));
    if let Some(w) = &window {
        let _ = w.set_ignore_cursor_events(true); // clicks go through to the app below
        place(w, &theme);
    }
    let data = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.clone())
        .join("voxis")
        .join("webview");
    let mut web_context = wry::WebContext::new(Some(data));
    let _webview = window.as_ref().and_then(|w| {
        orb_or_log(
            wry::WebViewBuilder::new_with_web_context(&mut web_context)
                .with_transparent(true)
                .with_background_color((0, 0, 0, 0))
                .with_url(format!("{url}/overlay?token={token}"))
                .build(w),
            &mut |l| log_line(l),
        )
    });

    if start_hands_free {
        set_hands_free(true);
    }
    std::thread::spawn(move || {
        let mut g = Gestures::new();
        for k in keys_rx {
            match k {
                Key::Down(at) => {
                    let hands_free = cap4.lock().unwrap().mode == Mode::HandsFree;
                    match g.down(at, hands_free) {
                        Gesture::HandsFreeOff => set_hands_free(false),
                        Gesture::StartPtt => {
                            cap4.lock().unwrap().start(Mode::Ptt);
                            DICTATING.store(true, Ordering::SeqCst);
                            show(Show::Recording);
                        }
                        _ => {}
                    }
                }
                Key::Up(at) => {
                    let released = unix_ms();
                    let captured = cap4.lock().unwrap().buf.len();
                    let gesture = g.up(at, captured, mic_rate);
                    if gesture == Gesture::Ignore {
                        continue;
                    }
                    let raw = cap4.lock().unwrap().stop();
                    DICTATING.store(false, Ordering::SeqCst);
                    match gesture {
                        Gesture::HandsFreeOn => set_hands_free(true),
                        Gesture::Transcribe => {
                            show(Show::Transcribing);
                            let _ = jobs4.send((to_16k(raw, mic_rate), false, released));
                        }
                        _ => show(Show::Hidden),
                    }
                }
            }
        }
    });

    // the hook lives on this thread: tao's loop pumps the messages it needs
    let hk = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), GetModuleHandleW(std::ptr::null()), 0) };
    if hk.is_null() {
        log_line("falha ao instalar o hook do teclado");
        std::process::exit(1);
    }
    READY.store(true, Ordering::SeqCst);
    log_line(&format!(
        "ditado pronto em {:.1} s: segure {key_name} para falar, toque duas vezes para hands-free",
        t.elapsed().as_secs_f64()
    ));
    let _mic = stream;
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        if let (Event::UserEvent(e), Some(window)) = (event, &window) {
            match e {
                Ui::Visible(true) => {
                    window.set_always_on_top(true);
                    window.set_visible(true);
                }
                Ui::Visible(false) => window.set_visible(false),
                Ui::Theme => place(window, &load_theme(&theme_path())),
            }
        }
    });
}

#[cfg(test)]
#[path = "dictate_tests.rs"]
mod tests;
