//! Tests of dictate.rs: the overlay server's boundary and the pure rules the
//! daemon runs (gestures, paste, logs, failure exits).

use super::*;
use std::io::Read;
use std::path::{Path, PathBuf};

/// A frontend root holding overlay.html and a subfolder, beside a sentinel
/// file that must never be served.
fn frontend() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("frontend");
    std::fs::create_dir_all(root.join("themes")).unwrap();
    std::fs::write(root.join("overlay.html"), b"<title>orb</title>").unwrap();
    std::fs::write(root.join("overlay.js"), b"js").unwrap();
    std::fs::write(root.join("themes").join("blade.js"), b"blade").unwrap();
    let sentinel = tmp.path().join("sentinel.txt");
    std::fs::write(&sentinel, b"SECRET").unwrap();
    (tmp, root, sentinel)
}

/// One request through serve_http over loopback: (status line, body).
fn http(root: &Path, theme: &Path, request: &str) -> (String, Vec<u8>) {
    let _one_at_a_time = crate::test_lock();
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let (root, theme) = (root.to_path_buf(), theme.to_path_buf());
    let server = std::thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        serve_http(s, "tok", &root, &theme);
    });
    let mut c = std::net::TcpStream::connect(addr).unwrap();
    c.write_all(request.as_bytes()).unwrap();
    let mut raw = Vec::new();
    c.read_to_end(&mut raw).unwrap();
    server.join().unwrap();
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    (head.lines().next().unwrap().to_owned(), raw[split + 4..].to_vec())
}

fn get(root: &Path, theme: &Path, target: &str) -> (String, Vec<u8>) {
    http(root, theme, &format!("GET {target} HTTP/1.1\r\nHost: x\r\n\r\n"))
}

fn post_theme(root: &Path, theme: &Path, query: &str, body: &str) -> String {
    http(root, theme, &format!("POST /theme{query} HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}", body.len())).0
}

// ------------------------------------------------------------ S4 settings the daemon applies

#[test]
fn the_next_dictate_start_applies_assistant_mode_and_initial_prompt() {
    let on = dictate_settings(&json!({"DICTATE_KEY": "f9", "ASSISTANT_MODE": "always_on", "INITIAL_PROMPT": "AXON"}));
    let off = dictate_settings(&json!({"ASSISTANT_MODE": "ptt", "INITIAL_PROMPT": ""}));
    let unset = dictate_settings(&json!({}));

    assert_eq!((on.key_name.as_str(), on.hands_free, on.prompt.as_str()), ("f9", true, "AXON"));
    assert_eq!((off.hands_free, off.prompt.as_str()), (false, ""), "an empty prompt disables priming");
    assert_eq!(unset.prompt, DEFAULT_PROMPT);
}

#[test]
fn the_daemon_uses_f13_when_the_saved_key_is_unsupported() {
    for saved in ["cmd_r", "shift_r", "f4", "f16"] {
        let s = dictate_settings(&json!({ "DICTATE_KEY": saved }));

        assert_eq!((s.key_name.as_str(), s.vk), ("f13", 0x7C), "{saved}");
    }
    assert_eq!(dictate_settings(&json!({})).key_name, "f13");
}

// ------------------------------------------------------------ S1 overlay boundary

#[test]
fn a_file_under_the_frontend_root_is_served() {
    let (tmp, root, _) = frontend();

    let (status, body) = get(&root, &tmp.path().join("orb-theme.json"), "/static/overlay.js");

    assert_eq!(status, "HTTP/1.1 200 OK");
    assert_eq!(body, b"js");
}

#[test]
fn a_file_in_a_subfolder_of_the_frontend_root_is_served() {
    let (tmp, root, _) = frontend();

    let (status, body) = get(&root, &tmp.path().join("orb-theme.json"), "/static/themes/blade.js");

    assert_eq!(status, "HTTP/1.1 200 OK");
    assert_eq!(body, b"blade");
}

#[test]
fn unsafe_static_paths_are_refused_with_400_bad_path() {
    let (_tmp, root, sentinel) = frontend();
    let drive = sentinel.to_string_lossy().replace('\\', "/");
    let rooted = drive.split_once(':').map(|(_, rest)| rest.to_owned()).unwrap();
    let cases = [
        drive.clone(),                       // drive-prefixed: C:/Users/x/sentinel.txt
        "%2e%2e/sentinel.txt".to_owned(),    // encoded ..
        "%5C%5Chost%5Cshare%5Cf".to_owned(), // UNC
        "NUL".to_owned(),                    // reserved device
        "CON".to_owned(),                    // reserved device
        "overlay.html:stream".to_owned(),    // alternate data stream
        rooted,                              // rooted: /Users/x/sentinel.txt
        "../sentinel.txt".to_owned(),        // plain ..
    ];

    for rel in &cases {
        assert_eq!(static_file(&root, rel), Err(StaticErr::BadPath), "{rel}");
    }
}

#[test]
fn a_sentinel_outside_the_root_is_never_served_over_http() {
    let (tmp, root, sentinel) = frontend();
    let theme = tmp.path().join("orb-theme.json");
    let drive = sentinel.to_string_lossy().replace('\\', "/");

    for target in [format!("/static/{drive}"), "/static/%2e%2e/sentinel.txt".to_owned(), "/static/..%5Csentinel.txt".to_owned()] {
        let (status, body) = get(&root, &theme, &target);

        assert_eq!(status, "HTTP/1.1 400 Bad Request", "{target}");
        assert_eq!(body, b"bad path", "{target}");
    }
}

#[test]
fn a_junction_inside_the_root_pointing_outside_answers_404() {
    let (tmp, root, _) = frontend();
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("sentinel.txt"), b"SECRET").unwrap();
    // mklink /J needs no privilege, unlike a symbolic link
    let made = {
        let _one_at_a_time = crate::test_lock();
        std::process::Command::new("cmd").args(["/c", "mklink", "/J"]).arg(root.join("link")).arg(&outside).output().unwrap()
    };
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));

    let (status, body) = get(&root, &tmp.path().join("orb-theme.json"), "/static/link/sentinel.txt");

    assert_eq!(status, "HTTP/1.1 404 Not Found");
    assert_eq!(body, b"not found");
}

#[test]
fn a_missing_frontend_folder_answers_404_for_overlay_and_static() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("no-such-frontend");
    let theme = tmp.path().join("orb-theme.json");

    let (overlay, _) = get(&missing, &theme, "/overlay");
    let (stat, _) = get(&missing, &theme, "/static/overlay.js");

    assert_eq!(overlay, "HTTP/1.1 404 Not Found");
    assert_eq!(stat, "HTTP/1.1 404 Not Found");
}

#[test]
fn the_overlay_page_is_served_without_a_token() {
    let (tmp, root, _) = frontend();

    let (status, body) = get(&root, &tmp.path().join("orb-theme.json"), "/overlay");

    assert_eq!(status, "HTTP/1.1 200 OK");
    assert_eq!(body, b"<title>orb</title>");
}

#[test]
fn theme_requests_without_the_token_get_403_and_leave_the_theme_file_unchanged() {
    let (tmp, root, _) = frontend();
    let theme = tmp.path().join("orb-theme.json");
    std::fs::write(&theme, br#"{"palette": "noir"}"#).unwrap();
    let body = r#"{"palette": "acid"}"#;

    let (get_status, _) = get(&root, &theme, "/theme");
    let no_token = post_theme(&root, &theme, "", body);
    let wrong_token = post_theme(&root, &theme, "?token=nope", body);

    assert_eq!(get_status, "HTTP/1.1 403 Forbidden");
    assert_eq!(no_token, "HTTP/1.1 403 Forbidden");
    assert_eq!(wrong_token, "HTTP/1.1 403 Forbidden");
    assert_eq!(std::fs::read(&theme).unwrap(), br#"{"palette": "noir"}"#);
}

#[test]
fn a_theme_update_with_the_token_answers_200_and_an_invalid_one_422() {
    let (tmp, root, _) = frontend();
    let theme = tmp.path().join("orb-theme.json");

    let saved = post_theme(&root, &theme, "?token=tok", r#"{"palette": "acid"}"#);
    let refused = post_theme(&root, &theme, "?token=tok", r#"{"palette": "neon"}"#);

    assert_eq!(saved, "HTTP/1.1 200 OK");
    assert_eq!(refused, "HTTP/1.1 422 Unprocessable Entity");
    assert_eq!(load_theme(&theme)["palette"], "acid");
}

// ------------------------------------------------------------ S6 gestures (AC 35-38)

fn capture(rate: u32) -> Capture {
    Capture { mode: Mode::Idle, rate, buf: Vec::new(), pre: VecDeque::new(), speaking: false, speech_len: 0, silence_len: 0, floor: 0.003 }
}

#[test]
fn a_hold_of_250_ms_with_audio_is_transcribed_and_pasted_with_a_space() {
    let mut g = Gestures::new();
    let t0 = Instant::now();

    assert_eq!(g.down(t0, false), Gesture::StartPtt);
    // held 250 ms with 400 ms of audio at 16 kHz
    assert_eq!(g.up(t0 + Duration::from_millis(250), 6400, 16_000), Gesture::Transcribe);
    assert_eq!(paste_text("abre um pull request"), Some("abre um pull request ".to_owned()));
    assert_eq!(paste_text(""), None, "an empty transcript pastes nothing");
    // 300 ms of audio is not more than 300 ms
    g.down(t0 + Duration::from_secs(5), false);
    assert_eq!(g.up(t0 + Duration::from_secs(6), 4800, 16_000), Gesture::Hide);
}

#[test]
fn a_short_tap_pastes_nothing_and_two_taps_turn_on_hands_free() {
    let mut g = Gestures::new();
    let t0 = Instant::now();

    g.down(t0, false);
    let first = g.up(t0 + Duration::from_millis(100), 1600, 16_000);
    g.down(t0 + Duration::from_millis(300), false);
    let second = g.up(t0 + Duration::from_millis(400), 1600, 16_000);
    // a third tap long after is a single tap again
    g.down(t0 + Duration::from_secs(3), false);
    let late = g.up(t0 + Duration::from_millis(3100), 1600, 16_000);

    assert_eq!(first, Gesture::Hide);
    assert_eq!(second, Gesture::HandsFreeOn);
    assert_eq!(late, Gesture::Hide);
}

/// `ms` of audio at `level`, in 10 ms chunks pushed into the capture.
fn feed(c: &mut Capture, ms: u32, level: f32) -> Option<Vec<f32>> {
    let chunk = vec![level; (c.rate / 100) as usize];
    let mut done = None;
    for _ in 0..ms / 10 {
        if let Some(u) = c.push(&chunk) {
            done = Some(u);
        }
    }
    done
}

#[test]
fn hands_free_closes_an_utterance_after_700_ms_of_silence() {
    let mut c = capture(16_000);
    c.start(Mode::HandsFree);

    assert_eq!(feed(&mut c, 500, 0.0), None, "silence never opens an utterance");
    assert_eq!(feed(&mut c, 400, 0.2), None, "speech alone does not close it");
    assert_eq!(feed(&mut c, 690, 0.0), None, "690 ms of silence is not enough");
    let utterance = feed(&mut c, 10, 0.0).expect("700 ms of silence closes it");

    assert!(utterance.len() >= 16_000 * 1100 / 1000, "speech and its silence: {}", utterance.len());
}

#[test]
fn hands_free_closes_an_utterance_at_30_s() {
    let mut c = capture(16_000);
    c.start(Mode::HandsFree);

    let utterance = feed(&mut c, 31_000, 0.2).expect("30 s closes a long utterance");

    assert!(utterance.len() <= 16_000 * 30 + 160 * 2, "{}", utterance.len());
}

#[test]
fn a_press_during_hands_free_ends_it_and_keeps_the_cut_speech() {
    let mut c = capture(16_000);
    c.start(Mode::HandsFree);
    feed(&mut c, 450, 0.2);
    let mut short = capture(16_000);
    short.start(Mode::HandsFree);
    feed(&mut short, 300, 0.2);
    let mut g = Gestures::new();
    let t0 = Instant::now();

    let press = g.down(t0, true);
    let cut = cut_utterance(&mut c);
    let release = g.up(t0 + Duration::from_millis(80), 0, 16_000);

    assert_eq!(press, Gesture::HandsFreeOff);
    assert!(cut.is_some_and(|u| u.len() >= 16_000 * 400 / 1000), "450 ms of speech is pasted");
    assert_eq!(cut_utterance(&mut short), None, "300 ms is under the 400 ms floor");
    assert_eq!(release, Gesture::Ignore, "that press's release does nothing");
    assert!(c.mode == Mode::Idle);
}

// ------------------------------------------------------------ S6 logs (AC 39, 80)

#[test]
fn the_utterance_log_holds_counts_and_timings_but_no_text() {
    let text = "o contrato fecha sexta com a Acme";

    let line = utterance_log("ptt", 2.14, 0.351, text, 1_762_530_000_000, 1_762_530_000_420);

    assert!(line.starts_with("ptt 2.1 s -> 0.35 s, 33 chars"), "{line}");
    assert!(!line.contains("contrato") && !line.contains(text), "{line}");
}

#[test]
fn the_utterance_log_carries_release_and_paste_timestamps() {
    let line = utterance_log("hf", 1.0, 0.2, "x", 1_762_530_000_000, 1_762_530_000_420);

    assert!(line.contains("release_ms=1762530000000") && line.contains("paste_ms=1762530000420"), "{line}");
}

// ------------------------------------------------------------ S6 paste (AC 40-42, 90)

/// A clipboard that records what it was given; `fail` makes every call fail.
struct FakeClip {
    text: Option<String>,
    sets: Vec<String>,
    fail: bool,
}

impl Clip for FakeClip {
    fn get(&mut self) -> Option<String> {
        self.text.clone()
    }
    fn set(&mut self, text: &str) -> Result<(), String> {
        if self.fail {
            return Err("clipboard busy".into());
        }
        self.sets.push(text.to_owned());
        self.text = Some(text.to_owned());
        Ok(())
    }
}

fn clip(previous: &str) -> FakeClip {
    FakeClip { text: Some(previous.to_owned()), sets: Vec::new(), fail: false }
}

#[test]
fn a_paste_puts_the_text_sends_ctrl_v_and_restores_the_previous_text() {
    let mut cb = clip("what the user had copied");
    let mut injected = 0;

    let log = paste_with(Ok(&mut cb), "ditado ", &mut || {
        injected += 1;
        4
    }, false, Duration::from_millis(1));

    assert_eq!(injected, 1);
    assert_eq!(cb.sets, ["ditado ", "what the user had copied"]);
    assert!(log.is_empty(), "{log:?}");
}

#[test]
fn a_partial_injection_leaves_the_text_on_the_clipboard() {
    let mut cb = clip("previous");

    let log = paste_with(Ok(&mut cb), "ditado ", &mut || 2, false, Duration::from_millis(1));

    assert_eq!(cb.text.as_deref(), Some("ditado "), "not restored: the user can still paste it");
    assert_eq!(log, ["paste failed: injected 2 of 4"]);
}

#[test]
fn a_clipboard_failure_is_logged_without_the_text() {
    let mut busy = FakeClip { text: None, sets: Vec::new(), fail: true };
    let mut injected = false;

    let unopened = paste_with(Err("OpenClipboard failed".into()), "segredo ", &mut || 4, false, Duration::from_millis(1));
    let unset = paste_with(Ok(&mut busy), "segredo ", &mut || {
        injected = true;
        4
    }, false, Duration::from_millis(1));

    assert_eq!(unopened, ["paste failed: clipboard OpenClipboard failed"]);
    assert_eq!(unset, ["paste failed: clipboard clipboard busy"]);
    assert!(!injected, "nothing is typed when the clipboard does not hold the text");
}

#[test]
fn an_elevated_target_skips_the_paste_and_keeps_the_text() {
    let mut cb = clip("previous");
    let mut injected = false;

    let log = paste_with(Ok(&mut cb), "ditado ", &mut || {
        injected = true;
        4
    }, true, Duration::from_millis(1));

    assert!(!injected, "no Ctrl+V into an elevated window");
    assert_eq!(cb.text.as_deref(), Some("ditado "), "left on the clipboard, not restored");
    assert_eq!(log, ["paste skipped: elevated target"]);
}

// ------------------------------------------------------------ S6 failures (AC 43-45, 47)

#[test]
fn an_orb_that_cannot_be_created_leaves_dictation_running() {
    let mut log = Vec::new();

    let none = orb_or_log::<(), _>(Err("WebView2 Runtime not found"), &mut |l| log.push(l.to_owned()));
    let some = orb_or_log::<u8, String>(Ok(7), &mut |l| log.push(l.to_owned()));

    assert_eq!(none, None);
    assert_eq!(some, Some(7));
    assert_eq!(log, ["overlay unavailable: WebView2 Runtime not found"]);
}

#[test]
fn a_lost_microphone_logs_and_exits_with_code_3() {
    let (line, code) = stream_lost("The requested device is no longer available.");

    assert_eq!(line, "microfone desconectado: The requested device is no longer available.");
    assert_eq!(code, Some(3));
    assert_eq!(EXIT_DEVICE_LOST, 3);
}

#[test]
fn an_unsupported_sample_format_or_model_failure_is_logged_with_exit_1() {
    let u8_format = std::panic::catch_unwind(|| sample_format_ok(cpal::SampleFormat::U8));
    let model = std::panic::catch_unwind(|| load_model::<(), _>(std::path::Path::new(r"C:\m\ggml-large-v3-turbo.bin"), || Err("invalid model data")));

    assert_eq!(u8_format.expect("no panic"), Err("formato de áudio não suportado: u8".to_owned()));
    assert_eq!(model.expect("no panic"), Err(r"modelo não carregou: C:\m\ggml-large-v3-turbo.bin: invalid model data".to_owned()));
    assert_eq!(sample_format_ok(cpal::SampleFormat::F32), Ok(()));
    assert_eq!(sample_format_ok(cpal::SampleFormat::I16), Ok(()));
}

#[test]
fn a_cpu_fallback_is_logged_as_sem_gpu() {
    assert_eq!(gpu_note(&[Device::Cpu]), Some("sem GPU: usando CPU"));
    assert_eq!(gpu_note(&[]), Some("sem GPU: usando CPU"));
    assert_eq!(gpu_note(&[Device::Cpu, Device::Gpu]), None);
}

#[test]
fn dictation_recognizes_portuguese_only() {
    assert_eq!(crate::LANGUAGE, "pt");
}

// ------------------------------------------------------------ review round 1: paste outcomes, startup (P5-2, P5-4, P5-21, AC 45)

/// A clock that returns `start`, then `start + step`, and so on.
fn clock(start: u128, step: u128) -> impl FnMut() -> u128 {
    let mut next = start;
    move || {
        let t = next;
        next += step;
        t
    }
}

#[test]
fn a_completed_paste_reports_when_ctrl_v_was_sent_and_when_the_clipboard_came_back() {
    let mut cb = clip("previous");

    let outcome = paste_outcome_with(Ok(&mut cb), "ditado ", &mut || 4, false, Duration::from_millis(1), &mut clock(1_000, 260));

    assert_eq!(outcome, PasteOutcome::Pasted { injected_ms: 1_000, restore: Restore::Done(1_260) });
    assert_eq!(cb.sets, ["ditado ", "previous"]);
}

#[test]
fn a_paste_that_did_not_happen_has_no_completion_timestamp() {
    let mut partial = clip("previous");
    let mut elevated = clip("previous");
    let mut busy = FakeClip { text: None, sets: Vec::new(), fail: true };
    let wait = Duration::from_millis(1);

    let injected_2 = paste_outcome_with(Ok(&mut partial), "x ", &mut || 2, false, wait, &mut clock(1, 1));
    let skipped = paste_outcome_with(Ok(&mut elevated), "x ", &mut || 4, true, wait, &mut clock(1, 1));
    let unopened = paste_outcome_with(Err("OpenClipboard failed".into()), "x ", &mut || 4, false, wait, &mut clock(1, 1));
    let unset = paste_outcome_with(Ok(&mut busy), "x ", &mut || 4, false, wait, &mut clock(1, 1));

    assert_eq!(injected_2, PasteOutcome::InjectFailed(2));
    assert_eq!(skipped, PasteOutcome::SkippedElevated);
    assert_eq!(unopened, PasteOutcome::ClipboardFailed("OpenClipboard failed".into()));
    assert_eq!(unset, PasteOutcome::ClipboardFailed("clipboard busy".into()));
    for (outcome, text) in [(&injected_2, "paste failed: injected 2 of 4"), (&skipped, "paste skipped: elevated target"),
                            (&unopened, "paste failed: clipboard OpenClipboard failed")] {
        assert_eq!(outcome.log_lines(), [text]);
    }
}

#[test]
fn the_utterance_line_names_paste_and_restore_times_only_for_a_paste_that_happened() {
    let pasted = utterance_line("ptt", 1.0, 0.2, "x", 5_000, &PasteOutcome::Pasted { injected_ms: 5_400, restore: Restore::Done(5_650) });
    let failed = utterance_line("ptt", 1.0, 0.2, "x", 5_000, &PasteOutcome::ClipboardFailed("busy".into()));
    let empty = utterance_line("hf", 1.0, 0.2, "", 5_000, &PasteOutcome::Empty);
    let partial = utterance_line("hf", 1.0, 0.2, "x", 5_000, &PasteOutcome::InjectFailed(1));
    let elevated = utterance_line("hf", 1.0, 0.2, "x", 5_000, &PasteOutcome::SkippedElevated);

    assert!(pasted.contains("release_ms=5000 paste_ms=5400") && pasted.ends_with("restore_ms=5650"), "{pasted}");
    for (line, why) in [(&failed, "clipboard_failed"), (&empty, "empty"), (&partial, "inject_failed"), (&elevated, "skipped_elevated")] {
        assert!(line.contains("release_ms=5000") && line.ends_with(&format!("paste={why}")), "{line}");
        assert!(!line.contains("paste_ms=") && !line.contains("restore_ms="), "{line}");
    }
    assert!(!failed.contains("busy"), "{failed}");
}

#[test]
fn a_clipboard_failure_hides_the_orb_in_both_modes() {
    let failed = PasteOutcome::ClipboardFailed("clipboard busy".into());
    let pasted = PasteOutcome::Pasted { injected_ms: 1, restore: Restore::Done(2) };

    assert!(hide_orb_after(&failed, true, false), "hands-free, still listening");
    assert!(hide_orb_after(&failed, false, true), "push-to-talk, idle");
    assert!(hide_orb_after(&failed, false, false), "push-to-talk, key held again");
    assert!(!hide_orb_after(&pasted, true, false), "hands-free keeps its orb after a good paste");
    assert!(hide_orb_after(&pasted, false, true));
    assert!(!hide_orb_after(&pasted, false, false));
    assert!(!hide_orb_after(&PasteOutcome::InjectFailed(2), true, false));
}

#[test]
fn a_dictate_that_starts_with_no_microphone_exits_3_like_a_lost_one() {
    let (line, code) = mic_start_failure("nenhum microfone disponível");

    assert_eq!(line, "microfone desconectado: nenhum microfone disponível");
    assert_eq!(code, EXIT_DEVICE_LOST);
    assert_eq!(code, 3);
}

#[test]
fn a_model_that_fails_to_make_its_state_or_to_warm_up_is_logged_not_a_panic() {
    let path = std::path::Path::new(r"C:\m\ggml-large-v3-turbo.bin");
    let ok_ctx = || Ok::<_, String>("ctx");

    let no_state = std::panic::catch_unwind(|| prepare_model(path, ok_ctx, |_| Err::<(), _>("out of memory"), |_: &mut ()| Ok::<(), String>(())));
    let no_warm = std::panic::catch_unwind(|| prepare_model(path, ok_ctx, |_| Ok::<_, String>(1u8), |_| Err::<(), _>("cuda error")));
    let no_ctx = std::panic::catch_unwind(|| prepare_model::<&str, (), _, String, String>(path, || Err("invalid model data"), |_| Ok(()), |_| Ok(())));
    let good = prepare_model(path, ok_ctx, |c| Ok::<_, String>(c.len()), |_| Ok::<(), String>(()));

    assert_eq!(no_state.expect("no panic").err(), Some(r"modelo não carregou: C:\m\ggml-large-v3-turbo.bin: out of memory".to_owned()));
    assert_eq!(no_warm.expect("no panic").err(), Some(r"modelo não carregou: C:\m\ggml-large-v3-turbo.bin: cuda error".to_owned()));
    assert_eq!(no_ctx.expect("no panic").err(), Some(r"modelo não carregou: C:\m\ggml-large-v3-turbo.bin: invalid model data".to_owned()));
    assert_eq!(good, Ok(("ctx", 3)));
}

// ------------------------------------------------------------ review round 1: the backend whisper.cpp reports (AC 46, 47)

#[test]
fn the_backend_whisper_cpp_names_is_read_from_its_own_log_line() {
    assert_eq!(observe_backend("whisper_backend_init_gpu: using CUDA0 backend\n"), Some(BackendResult::Named("CUDA0".into())));
    assert_eq!(observe_backend("whisper_backend_init_gpu: using Vulkan0 backend"), Some(BackendResult::Named("Vulkan0".into())));
    assert_eq!(observe_backend("whisper_init_with_params_no_state: using CPU backend\n"), Some(BackendResult::Named("CPU".into())));
    assert_eq!(observe_backend("whisper_backend_init_gpu: no GPU found\n"), Some(BackendResult::CpuFallback));
    assert_eq!(observe_backend("ggml_cuda_init: failed to initialize CUDA backend: out of memory"), Some(BackendResult::CpuFallback));
    assert_eq!(observe_backend("whisper_model_load: CPU total size = 1623.92 MB"), None);
    assert_eq!(observe_backend("using the default prompt"), None);
}

#[test]
fn a_gpu_that_whisper_cpp_did_not_use_is_logged_as_sem_gpu_whatever_ggml_registered() {
    let gpu_registered = [Device::Cpu, Device::Gpu];
    let cpu_only = [Device::Cpu];

    assert_eq!(backend_note(&gpu_registered, Some(&BackendResult::CpuFallback)), Some("sem GPU: usando CPU"));
    assert_eq!(backend_note(&gpu_registered, Some(&BackendResult::Named("CPU".into()))), Some("sem GPU: usando CPU"));
    assert_eq!(backend_note(&cpu_only, Some(&BackendResult::Named("CUDA0".into()))), None);
    assert_eq!(backend_note(&gpu_registered, Some(&BackendResult::Named("Vulkan0".into()))), None);
}

#[test]
fn without_a_backend_report_the_registered_devices_decide() {
    assert_eq!(backend_note(&[Device::Cpu], None), Some("sem GPU: usando CPU"));
    assert_eq!(backend_note(&[Device::Cpu, Device::Gpu], None), None);
}

#[test]
fn the_log_hook_is_wired_to_whisper_cpp_itself() {
    let _one_at_a_time = crate::test_lock();
    install_whisper_log();

    let loaded = WhisperContext::new_with_params("no-such-model-for-the-log-hook.bin", WhisperContextParameters::default());

    assert!(loaded.is_err(), "a missing model does not load");
    let tail: Vec<String> = LOG_TAIL.lock().unwrap().iter().cloned().collect();
    assert!(tail.iter().any(|l| l.contains("no-such-model-for-the-log-hook.bin")), "whisper.cpp's own lines reach the hook: {tail:?}");
}

// ------------------------------------------------------------ review round 2: the clipboard restore (P5-4, P5-21)

/// A clipboard whose first `set` works and whose later ones fail, like a clipboard another program grabs
/// while the paste runs. Holds the text the paste put there.
struct GrabbedAfterTheFirstSet {
    text: Option<String>,
    sets: usize,
}

impl Clip for GrabbedAfterTheFirstSet {
    fn get(&mut self) -> Option<String> {
        self.text.clone()
    }
    fn set(&mut self, text: &str) -> Result<(), String> {
        self.sets += 1;
        if self.sets > 1 {
            return Err("clipboard busy".into());
        }
        self.text = Some(text.to_owned());
        Ok(())
    }
}

#[test]
fn a_restore_that_fails_keeps_the_injection_time_and_reports_no_restore_time() {
    let mut cb = GrabbedAfterTheFirstSet { text: Some("what the user had copied".into()), sets: 0 };

    let outcome = paste_outcome_with(Ok(&mut cb), "segredo do ditado ", &mut || 4, false, Duration::from_millis(1), &mut clock(7_000, 260));

    assert_eq!(outcome, PasteOutcome::Pasted { injected_ms: 7_000, restore: Restore::Failed("clipboard busy".into()) });
    assert_eq!(cb.text.as_deref(), Some("segredo do ditado "), "the dictated text is still there for the user");
}

#[test]
fn a_restore_failure_is_logged_without_the_dictated_text_and_has_no_restore_timestamp() {
    let outcome = PasteOutcome::Pasted { injected_ms: 7_000, restore: Restore::Failed("clipboard busy".into()) };

    let log = outcome.log_lines();
    let line = utterance_line("hf", 1.0, 0.2, "segredo do ditado", 6_500, &outcome);

    assert_eq!(log, ["paste restore failed: clipboard clipboard busy"]);
    assert!(line.contains("release_ms=6500 paste_ms=7000") && line.ends_with("restore=failed"), "{line}");
    assert!(!line.contains("restore_ms=") && !line.contains("segredo"), "{line}");
}

#[test]
fn a_paste_with_no_previous_clipboard_text_reports_no_restore_time() {
    let mut cb = FakeClip { text: None, sets: Vec::new(), fail: false };

    let outcome = paste_outcome_with(Ok(&mut cb), "ditado ", &mut || 4, false, Duration::from_millis(1), &mut clock(7_000, 260));

    assert_eq!(outcome, PasteOutcome::Pasted { injected_ms: 7_000, restore: Restore::NoPrevious });
    assert_eq!(cb.sets, ["ditado "], "nothing was put back");
    let line = utterance_line("ptt", 1.0, 0.2, "x", 6_500, &outcome);
    assert!(line.contains("paste_ms=7000") && line.ends_with("restore=none") && !line.contains("restore_ms="), "{line}");
}

#[test]
fn a_restore_failure_hides_the_orb_in_both_modes() {
    let failed = PasteOutcome::Pasted { injected_ms: 1, restore: Restore::Failed("clipboard busy".into()) };
    let none = PasteOutcome::Pasted { injected_ms: 1, restore: Restore::NoPrevious };

    assert!(hide_orb_after(&failed, true, false), "hands-free, still listening");
    assert!(hide_orb_after(&failed, false, true), "push-to-talk, idle");
    assert!(hide_orb_after(&failed, false, false), "push-to-talk, key held again");
    assert!(!hide_orb_after(&none, true, false), "no previous text is not a failure");
}
