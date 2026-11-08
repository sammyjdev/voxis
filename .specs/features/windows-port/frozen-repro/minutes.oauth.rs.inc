
// ---- appended by scripts/windows/frozen-repro.sh to minutes.rs of the frozen SHA 889d48c (certificates round) ----
// Criterion 29, OAuth: the frozen `google_token` against a loopback endpoint named by `token_uri` in a
// temporary %APPDATA%\voxis holding made-up OAuth files. The endpoint accepts, reads the request and
// holds it for 40 s without answering. The criterion's OAuth deadline is 30 s: the call has to be over
// before 32 s. No real host, account or secret is involved (the only URL the call can reach is the
// loopback one the files name; Drive URLs are not touched because the token call never succeeds).
// The frozen client has no deadline, so the call ends only when the server drops it, after 40 s.
#[cfg(test)]
mod frozen_repro_oauth {
    use super::*;
    use std::io::{BufRead, BufReader};

    #[test]
    fn the_oauth_call_ends_at_its_thirty_second_deadline_when_the_endpoint_stalls_for_forty() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut r = BufReader::new(s.try_clone().unwrap());
                    let mut line = String::new();
                    while r.read_line(&mut line).unwrap_or(0) > 2 {
                        line.clear();
                    }
                    std::thread::sleep(std::time::Duration::from_secs(40));
                    drop(s);
                });
            }
        });
        let appdata = tempfile::tempdir().unwrap();
        let cfg = appdata.path().join("voxis");
        std::fs::create_dir_all(&cfg).unwrap();
        std::fs::write(
            cfg.join("google-oauth-client.json"),
            serde_json::json!({"installed": {"client_id": "c", "client_secret": "s", "token_uri": format!("{base}/token")}}).to_string(),
        )
        .unwrap();
        std::fs::write(cfg.join("google-token.json"), r#"{"refresh_token": "r"}"#).unwrap();
        std::env::set_var("APPDATA", appdata.path());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let t = std::time::Instant::now();
            let r = google_token();
            let _ = tx.send((t.elapsed(), r));
        });

        let (elapsed, result) = rx.recv_timeout(std::time::Duration::from_secs(80)).expect("the call never returned");

        assert!(
            elapsed < std::time::Duration::from_secs(32),
            "the call outlived the 30 s OAuth deadline: it ended after {elapsed:?} with {result:?}"
        );
    }
}
