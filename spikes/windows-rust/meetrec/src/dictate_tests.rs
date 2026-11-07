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
