//! Meeting minutes: transcript -> Claude (structured JSON; model from config `minutes_model`) -> Markdown
//! -> Google Doc in the "VOXIS Atas" Drive folder.
//!
//! Secrets live in %APPDATA%\voxis (the OS config dir elsewhere):
//!   secrets.toml               anthropic_api_key = "..." (ANTHROPIC_API_KEY wins)
//!   google-oauth-client.json   OAuth desktop client from the voxis GCP project
//!   google-token.json          {"refresh_token": "..."} from the first authorization
//!
//! Every provider call has a finite deadline (`DEADLINES`); a call past it
//! fails naming the provider and the deadline, and `ata.md` is on disk before
//! the first Drive call.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

/// Sonnet 5.5 at low effort framed a real meeting right where Haiku 5.5 got
/// the speakers' roles backwards (A/B on 2026-11-06); `minutes_model` in
/// config.toml overrides it.
const DEFAULT_MODEL: &str = "claude-sonnet-5-5";

/// The model named by `minutes_model` in the config.toml text, or the default.
fn minutes_model(config: Option<&str>) -> String {
    config
        .and_then(|raw| crate::config_value_in(raw, "minutes_model"))
        .unwrap_or_else(|| DEFAULT_MODEL.to_owned())
}

pub const FOLDER: &str = "VOXIS Atas";

/// The words every deadline error carries; the pipeline maps them to
/// `provider_timeout`.
pub const DEADLINE_MARK: &str = "(deadline)";

/// How long each provider call may take, end to end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Deadlines {
    pub connect: Duration,
    pub anthropic: Duration,
    pub oauth: Duration,
    /// Drive folder list and create, and the lookup of an earlier upload
    pub drive_list: Duration,
    pub drive_upload: Duration,
}

pub const DEADLINES: Deadlines = Deadlines {
    connect: Duration::from_secs(10),
    anthropic: Duration::from_secs(300),
    oauth: Duration::from_secs(30),
    drive_list: Duration::from_secs(30),
    drive_upload: Duration::from_secs(120),
};

/// Where the provider calls go, and their deadlines; tests point it at
/// loopback fakes.
pub struct Ctx {
    pub anthropic: String,
    pub oauth_default: String,
    pub drive_files: String,
    pub drive_upload: String,
    pub deadlines: Deadlines,
}

impl Ctx {
    pub fn real() -> Self {
        Self {
            anthropic: "https://api.anthropic.com/v1/messages".into(),
            oauth_default: "https://oauth2.googleapis.com/token".into(),
            drive_files: "https://www.googleapis.com/drive/v3/files".into(),
            drive_upload: "https://www.googleapis.com/upload/drive/v3/files".into(),
            deadlines: DEADLINES,
        }
    }
}

fn config_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| ".".into());
    base.join("voxis")
}

/// "300 s", or "500 ms" under a second.
fn secs(d: Duration) -> String {
    if d.as_millis() % 1000 == 0 {
        format!("{} s", d.as_secs())
    } else {
        format!("{} ms", d.as_millis())
    }
}

/// An agent for one call that must end within `deadline`, body included.
fn agent(connect: Duration, deadline: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        // read 4xx/5xx bodies ourselves: the API error text is the useful part
        .http_status_as_error(false)
        .timeout_connect(Some(connect))
        .timeout_global(Some(deadline))
        .build()
        .into()
}

/// A failed call, named; a timeout says which deadline it hit.
fn call_error(provider: &str, deadline: Duration, e: ureq::Error) -> String {
    match e {
        ureq::Error::Timeout(_) => format!("{provider}: no answer within {} {DEADLINE_MARK}", secs(deadline)),
        other => format!("{provider}: {other}"),
    }
}

/// A Drive upload that timed out may still have landed; the retry finds it
/// through its `voxis_session` property instead of uploading again.
fn upload_error(msg: String) -> String {
    if msg.contains(DEADLINE_MARK) {
        format!("{msg}; the upload may already have completed: check the Drive folder {FOLDER} (a retry reuses it)")
    } else {
        msg
    }
}

fn check(mut resp: ureq::http::Response<ureq::Body>, what: &str, deadline: Duration) -> Result<Value, String> {
    let status = resp.status();
    let body: Value = resp.body_mut().read_json().map_err(|e| call_error(what, deadline, e))?;
    if status.is_success() {
        Ok(body)
    } else {
        Err(format!("{what}: HTTP {status}: {body}"))
    }
}

fn api_key() -> Result<String, String> {
    if let Ok(k) = std::env::var("ANTHROPIC_API_KEY") {
        if !k.is_empty() {
            return Ok(k);
        }
    }
    let raw = std::fs::read_to_string(config_dir().join("secrets.toml"))
        .map_err(|e| format!("no ANTHROPIC_API_KEY and no secrets.toml: {e}"))?;
    raw.lines()
        .filter_map(|l| l.trim().strip_prefix("anthropic_api_key"))
        .filter_map(|rest| rest.trim_start().strip_prefix('='))
        .map(|v| v.trim().trim_matches('"').to_string())
        .find(|v| !v.is_empty())
        .ok_or_else(|| "anthropic_api_key is empty in secrets.toml".into())
}

const SYSTEM: &str = "Você escreve atas de reunião em português do Brasil a partir de uma \
transcrição automática. A transcrição tem erros de reconhecimento de fala: corrija nomes e termos \
técnicos quando o contexto deixar claro, e nunca invente fatos, nomes ou prazos que não estejam nela. \
\"Você\" é o dono da ata (microfone local); \"Outros\" são os demais participantes. \
Seja direto: o resumo cabe em um parágrafo curto, decisões e tarefas em itens objetivos.";

fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["titulo", "resumo", "decisoes", "action_items"],
        "properties": {
            "titulo": {"type": "string", "description": "título curto da reunião, sem data"},
            "resumo": {"type": "string"},
            "decisoes": {"type": "array", "items": {"type": "string"}},
            "action_items": {"type": "array", "items": {
                "type": "object",
                "additionalProperties": false,
                "required": ["tarefa", "responsavel"],
                "properties": {
                    "tarefa": {"type": "string"},
                    "responsavel": {"type": "string", "description": "quem faz, ou \"a definir\""}
                }
            }}
        }
    })
}

/// Checks the minutes Claude returned as JSON matching `schema()`.
/// A title and a summary, at least: structured output once came back with
/// every field empty on a 15-minute meeting, and that was saved as a note.
fn usable(m: &Value) -> bool {
    let filled = |k: &str| m[k].as_str().is_some_and(|s| !s.trim().is_empty());
    filled("titulo") && filled("resumo")
}

/// The session dir is named %Y%m%d-%H%M%S when the recording started; the
/// WAV's mtime is when it stopped.
fn recording_date(dir: &Path) -> String {
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if let Ok(t) = chrono::NaiveDateTime::parse_from_str(&name, "%Y%m%d-%H%M%S") {
        return t.format("%Y-%m-%d %H:%M").to_string();
    }
    std::fs::metadata(dir.join("mic.wav"))
        .or_else(|_| std::fs::metadata(dir.join("transcript.md")))
        .and_then(|m| m.modified())
        .map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

/// Low effort first; an empty answer gets one retry at medium.
pub fn summarize(ctx: &Ctx, key: &str, model: &str, transcript: &str, raw_log: &Path) -> Result<(Value, Value), String> {
    let mut last = Value::Null;
    for effort in ["low", "medium"] {
        let (m, usage) = summarize_once(ctx, key, model, transcript, effort, raw_log)?;
        if usable(&m) {
            return Ok((m, usage));
        }
        last = m;
    }
    Err(format!("{model} devolveu a ata vazia duas vezes: {last}"))
}

fn request_body(model: &str, effort: &str, transcript: &str) -> Value {
    json!({
        "model": model,
        "max_tokens": 16000,
        "output_config": {"effort": effort, "format": {"type": "json_schema", "schema": schema()}},
        "system": SYSTEM,
        "messages": [{"role": "user", "content": format!("Transcrição:\n\n{transcript}")}],
    })
}

fn summarize_once(ctx: &Ctx, key: &str, model: &str, transcript: &str, effort: &str, raw_log: &Path) -> Result<(Value, Value), String> {
    let body = request_body(model, effort, transcript);
    let deadline = ctx.deadlines.anthropic;
    let resp = agent(ctx.deadlines.connect, deadline)
        .post(&ctx.anthropic)
        .header("x-api-key", key)
        .header("anthropic-version", "2026-10-21")
        .send_json(&body)
        .map_err(|e| call_error("anthropic", deadline, e))?;
    // an unknown model comes back as a 404 naming it; point at where it is set
    let msg = check(resp, "anthropic", deadline)
        .map_err(|e| format!("{e} (modelo {model}; mude em minutes_model no %APPDATA%\\voxis\\config.toml)"))?;
    // the raw answer, for when the minutes come out wrong
    let _ = std::fs::write(raw_log, serde_json::to_string_pretty(&msg).unwrap_or_default());
    match msg["stop_reason"].as_str() {
        Some("end_turn") => {}
        other => return Err(format!("anthropic stopped with {other:?}: {msg}")),
    }
    let text: String = msg["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect();
    let minutes: Value = serde_json::from_str(&text).map_err(|e| format!("minutes json: {e}: {text}"))?;
    Ok((minutes, msg["usage"].clone()))
}

pub fn render(m: &Value, date: &str, transcript: &str) -> String {
    let mut md = format!("# {}\n\n**Data:** {date}\n\n## Resumo\n\n{}\n\n", m["titulo"].as_str().unwrap_or("Reunião"),
                         m["resumo"].as_str().unwrap_or(""));
    md += "## Decisões\n\n";
    let decisions: Vec<&str> = m["decisoes"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if decisions.is_empty() {
        md += "Nenhuma decisão registrada.\n\n";
    }
    for d in decisions {
        md += &format!("- {d}\n");
    }
    md += "\n## Action items\n\n";
    for a in m["action_items"].as_array().into_iter().flatten() {
        md += &format!("- [ ] {} ({})\n", a["tarefa"].as_str().unwrap_or(""), a["responsavel"].as_str().unwrap_or("a definir"));
    }
    // the transcript already carries its own "# Transcrição" heading
    md += "\n---\n\n";
    md += &transcript.replacen("# Transcrição", "## Transcrição", 1);
    md
}

fn google_token(ctx: &Ctx, cfg: &Path) -> Result<String, String> {
    let client: Value = serde_json::from_str(
        &std::fs::read_to_string(cfg.join("google-oauth-client.json")).map_err(|e| format!("oauth client: {e}"))?,
    )
    .map_err(|e| e.to_string())?;
    let c = &client["installed"];
    let tok: Value = serde_json::from_str(
        &std::fs::read_to_string(cfg.join("google-token.json"))
            .map_err(|e| format!("not authorized with Google yet (google-token.json): {e}"))?,
    )
    .map_err(|e| e.to_string())?;
    let deadline = ctx.deadlines.oauth;
    let resp = agent(ctx.deadlines.connect, deadline)
        .post(c["token_uri"].as_str().unwrap_or(&ctx.oauth_default))
        .send_form([
            ("client_id", c["client_id"].as_str().unwrap_or("")),
            ("client_secret", c["client_secret"].as_str().unwrap_or("")),
            ("refresh_token", tok["refresh_token"].as_str().unwrap_or("")),
            ("grant_type", "refresh_token"),
        ])
        .map_err(|e| call_error("google token", deadline, e))?;
    let v = check(resp, "google token", deadline)?;
    v["access_token"].as_str().map(str::to_owned).ok_or_else(|| format!("google token: {v}"))
}

fn ensure_folder(ctx: &Ctx, token: &str, name: &str) -> Result<String, String> {
    let q = format!("name='{name}' and mimeType='application/vnd.google-apps.folder' and trashed=false");
    let deadline = ctx.deadlines.drive_list;
    let resp = agent(ctx.deadlines.connect, deadline)
        .get(&ctx.drive_files)
        .query("q", &q)
        .query("fields", "files(id)")
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| call_error("drive list", deadline, e))?;
    let found = check(resp, "drive list", deadline)?;
    if let Some(id) = found["files"][0]["id"].as_str() {
        return Ok(id.to_owned());
    }
    let resp = agent(ctx.deadlines.connect, deadline)
        .post(&format!("{}?fields=id", ctx.drive_files))
        .header("Authorization", &format!("Bearer {token}"))
        .send_json(&json!({"name": name, "mimeType": "application/vnd.google-apps.folder"}))
        .map_err(|e| call_error("drive mkdir", deadline, e))?;
    let v = check(resp, "drive mkdir", deadline)?;
    v["id"].as_str().map(str::to_owned).ok_or_else(|| format!("drive mkdir: {v}"))
}

/// The Doc an earlier run of this session already uploaded, if any: a file
/// in the folder whose `voxis_session` property is this session (door 8).
fn find_session_doc(ctx: &Ctx, token: &str, folder: &str, session: &str) -> Result<Option<Value>, String> {
    let q = format!("'{folder}' in parents and appProperties has {{ key='voxis_session' and value='{session}' }} and trashed=false");
    let deadline = ctx.deadlines.drive_list;
    let resp = agent(ctx.deadlines.connect, deadline)
        .get(&ctx.drive_files)
        .query("q", &q)
        .query("fields", "files(id,name,webViewLink)")
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| call_error("drive list", deadline, e))?;
    let found = check(resp, "drive list", deadline)?;
    Ok(found["files"].as_array().and_then(|f| f.first()).cloned())
}

/// Uploads Markdown; Drive converts it into a Google Doc (headings, bold,
/// checklists).
pub fn upload_doc(ctx: &Ctx, cfg: &Path, title: &str, markdown: &str, session: &str) -> Result<Value, String> {
    let token = google_token(ctx, cfg)?;
    let folder = ensure_folder(ctx, &token, FOLDER)?;
    if let Some(doc) = find_session_doc(ctx, &token, &folder, session)? {
        return Ok(doc);
    }
    let meta = json!({
        "name": title, "parents": [folder], "mimeType": "application/vnd.google-apps.document",
        "appProperties": {"voxis_session": session},
    });
    let boundary = format!("voxis{:x}", std::process::id() as u64 ^ 0x9e37_79b9_7f4a_7c15);
    let body = format!(
        "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n\
         --{boundary}\r\nContent-Type: text/markdown; charset=UTF-8\r\n\r\n{markdown}\r\n--{boundary}--\r\n"
    );
    let deadline = ctx.deadlines.drive_upload;
    let resp = agent(ctx.deadlines.connect, deadline)
        .post(&format!("{}?uploadType=multipart&fields=id,name,webViewLink", ctx.drive_upload))
        .header("Authorization", &format!("Bearer {token}"))
        .header("Content-Type", &format!("multipart/related; boundary={boundary}"))
        .send(body.as_bytes())
        .map_err(|e| upload_error(call_error("drive upload", deadline, e)))?;
    check(resp, "drive upload", deadline).map_err(upload_error)
}

/// What the machine has set up for the minutes: an Anthropic key, and both
/// Google files (OAuth client and token).
pub struct Credentials {
    pub anthropic_key: Option<String>,
    pub google: bool,
}

impl Credentials {
    pub fn here() -> Self {
        Self::in_dir(&config_dir(), api_key().ok())
    }

    pub fn in_dir(dir: &Path, anthropic_key: Option<String>) -> Self {
        Self {
            anthropic_key,
            google: dir.join("google-oauth-client.json").is_file() && dir.join("google-token.json").is_file(),
        }
    }
}

/// Where the minutes of a meeting ended up.
#[derive(Debug, PartialEq)]
pub enum Ata {
    /// a Google Doc: (title, link)
    Doc(String, String),
    /// ata.md in the session folder, for a machine without Google set up
    Local(PathBuf),
    /// no Anthropic key: only the transcript exists
    NotWritten,
}

/// summarize -> render -> ata.md next to the recording -> Google Doc.
pub fn run(dir: &Path) -> Result<Ata, String> {
    let model = minutes_model(crate::config_text().as_deref());
    let raw_log = dir.join("minutes.json");
    let ctx = Ctx::real();
    let creds = Credentials::here();
    let key = creds.anthropic_key.clone().unwrap_or_default();
    let summarize_with_model = |transcript: &str| -> Result<Value, String> {
        let t = std::time::Instant::now();
        let (m, usage) = summarize(&ctx, &key, &model, transcript, &raw_log)?;
        println!("{model}: {:.1} s, {} in / {} out tokens", t.elapsed().as_secs_f64(), usage["input_tokens"], usage["output_tokens"]);
        Ok(m)
    };
    let session = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let upload = |title: &str, md: &str| upload_doc(&ctx, &config_dir(), title, md, &session);
    run_with_drive(dir, &creds, &summarize_with_model, &upload)
}

/// `run` with the setup and the summarizer passed in, and no Drive. Without
/// a key nothing is asked for; without Google the minutes stay in ata.md.
pub fn run_with(dir: &Path, creds: &Credentials, summarize: &dyn Fn(&str) -> Result<Value, String>) -> Result<Ata, String> {
    run_with_drive(dir, creds, summarize, &|_, _| Err("no Drive in this run".into()))
}

/// `ata.md` stands for this transcript: present, not empty, newer than it.
fn ata_fresh(dir: &Path) -> bool {
    let meta = |f: &str| std::fs::metadata(dir.join(f)).ok();
    match (meta("ata.md"), meta("transcript.md")) {
        (Some(a), Some(t)) => a.len() > 0 && matches!((a.modified(), t.modified()), (Ok(a), Ok(t)) if a > t),
        _ => false,
    }
}

/// `run_with` and the Drive upload passed in.
pub fn run_with_drive(
    dir: &Path,
    creds: &Credentials,
    summarize: &dyn Fn(&str) -> Result<Value, String>,
    upload: &dyn Fn(&str, &str) -> Result<Value, String>,
) -> Result<Ata, String> {
    if creds.anthropic_key.is_none() {
        println!("no anthropic_api_key: minutes not written");
        return Ok(Ata::NotWritten);
    }
    let transcript = std::fs::read_to_string(dir.join("transcript.md"))
        .map_err(|e| format!("{}: {e} (run transcribe first)", dir.join("transcript.md").display()))?;
    let date = recording_date(dir);
    let ata = dir.join("ata.md");
    // a retry after a failure past the summary reuses it: no second paid call
    let (md, titulo) = if ata_fresh(dir) {
        let md = std::fs::read_to_string(&ata).map_err(|e| e.to_string())?;
        let titulo = md.lines().next().and_then(|l| l.strip_prefix("# ")).unwrap_or("Reunião").to_owned();
        println!("ata.md is newer than the transcript: reused");
        (md, titulo)
    } else {
        let m = summarize(&transcript)?;
        let md = render(&m, &date, &transcript);
        // on disk before the first Drive call
        std::fs::write(&ata, &md).map_err(|e| e.to_string())?;
        (md, m["titulo"].as_str().unwrap_or("Reunião").to_owned())
    };
    let title = format!("{date} - {titulo}");
    if !creds.google {
        println!("no Google credentials: minutes kept in {}", ata.display());
        return Ok(Ata::Local(ata));
    }
    let doc = upload(&title, &md)?;
    let link = doc["webViewLink"].as_str().unwrap_or("").to_owned();
    println!("ata: {link}");
    Ok(Ata::Doc(title, link))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_counts_as_set_up_only_with_both_its_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("google-oauth-client.json"), "{}").unwrap();

        let only_client = Credentials::in_dir(tmp.path(), None);
        std::fs::write(tmp.path().join("google-token.json"), "{}").unwrap();
        let both = Credentials::in_dir(tmp.path(), None);

        assert!(!only_client.google);
        assert!(both.google);
    }

    #[test]
    fn minutes_without_a_title_or_summary_are_not_accepted() {
        let empty = json!({"titulo": "", "resumo": " ", "decisoes": [], "action_items": []});

        assert!(!usable(&empty));
    }

    #[test]
    fn minutes_with_a_title_and_summary_are_accepted() {
        let ok = json!({"titulo": "Contratação", "resumo": "Entrevista positiva.", "decisoes": [], "action_items": []});

        assert!(usable(&ok));
    }

    #[test]
    fn without_a_config_file_the_minutes_are_written_by_sonnet() {
        let body = request_body(&minutes_model(None), "low", "**[00:01] Você:** oi");

        assert_eq!(body["model"], "claude-sonnet-5-5");
    }

    #[test]
    fn a_config_without_minutes_model_keeps_sonnet() {
        let config = "mic = \"Microfone (2- fifine Microphone)\"\nspeaker = \"\"\n";

        let body = request_body(&minutes_model(Some(config)), "low", "**[00:01] Você:** oi");

        assert_eq!(body["model"], "claude-sonnet-5-5");
    }

    #[test]
    fn the_minutes_model_set_in_the_config_writes_the_minutes() {
        let config = "mic = \"fifine\"\nminutes_model = \"claude-haiku-5-5\"\n";

        let body = request_body(&minutes_model(Some(config)), "low", "**[00:01] Você:** oi");

        assert_eq!(body["model"], "claude-haiku-5-5");
    }

    #[test]
    fn the_minutes_are_dated_when_the_recording_started() {
        let dir = Path::new(r"C:\Users\x\.miris\sessions\20261105-222346");

        assert_eq!(recording_date(dir), "2026-11-05 22:23");
    }

    // ------------------------------------------------------------ S5 deadlines and Drive (F21, AC 29-34, 88, 89)

    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::{Arc, Mutex};

    enum Reply {
        Json(u16, String),
        /// accept, read the request, then answer nothing for 3 s
        Stall,
    }

    type Handler = Arc<dyn Fn(&str, &str, &str) -> Reply + Send + Sync>;

    /// A loopback HTTP server: `handler(method, target, body)` decides each answer.
    fn fake(handler: impl Fn(&str, &str, &str) -> Reply + Send + Sync + 'static) -> String {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let handler: Handler = Arc::new(handler);
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let h = handler.clone();
                std::thread::spawn(move || {
                    let mut s = s;
                    let mut r = BufReader::new(s.try_clone().unwrap());
                    let mut first = String::new();
                    r.read_line(&mut first).unwrap_or(0);
                    let mut length = 0usize;
                    loop {
                        let mut line = String::new();
                        if r.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                            break;
                        }
                        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                            length = v.trim().parse().unwrap_or(0);
                        }
                    }
                    let mut body = vec![0u8; length];
                    let _ = r.read_exact(&mut body);
                    let mut parts = first.split_whitespace();
                    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
                    match h(method, target, &String::from_utf8_lossy(&body)) {
                        Reply::Json(code, body) => {
                            let _ = write!(s, "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                        }
                        Reply::Stall => std::thread::sleep(Duration::from_secs(3)),
                    }
                });
            }
        });
        base
    }

    fn ctx_at(base: &str, deadline: Duration) -> Ctx {
        Ctx {
            anthropic: format!("{base}/v1/messages"),
            oauth_default: format!("{base}/token"),
            drive_files: format!("{base}/drive/v3/files"),
            drive_upload: format!("{base}/upload/drive/v3/files"),
            deadlines: Deadlines { connect: deadline, anthropic: deadline, oauth: deadline, drive_list: deadline, drive_upload: deadline },
        }
    }

    /// %APPDATA%\voxis with both Google files, the token endpoint at `base`.
    fn google_dir(base: &str) -> tempfile::TempDir {
        let cfg = tempfile::tempdir().unwrap();
        std::fs::write(cfg.path().join("google-oauth-client.json"), json!({"installed": {"client_id": "c", "client_secret": "s", "token_uri": format!("{base}/token")}}).to_string()).unwrap();
        std::fs::write(cfg.path().join("google-token.json"), r#"{"refresh_token": "r"}"#).unwrap();
        cfg
    }

    /// A session folder with its transcript, named like a real session.
    fn session_with_transcript() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("20261107-143200");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("transcript.md"), "# Transcrição\n\n**[00:01] Você:** fechar sexta\n").unwrap();
        (tmp, dir)
    }

    fn minutes_json(_: &str) -> Result<Value, String> {
        Ok(json!({"titulo": "Alinhamento", "resumo": "Fechar sexta.", "decisoes": [], "action_items": []}))
    }

    fn google() -> Credentials {
        Credentials { anthropic_key: Some("key".into()), google: true }
    }

    /// A Drive that answers the token and folder calls and lets `upload` decide the rest.
    fn drive(upload: impl Fn(&str, &str) -> Reply + Send + Sync + 'static) -> String {
        fake(move |method, target, body| match (method, target) {
            ("POST", t) if t.starts_with("/token") => Reply::Json(200, r#"{"access_token": "a"}"#.into()),
            ("GET", t) if t.starts_with("/drive/v3/files") && !t.contains("appProperties") => Reply::Json(200, r#"{"files": [{"id": "F"}]}"#.into()),
            _ => upload(target, body),
        })
    }

    #[test]
    fn every_provider_call_carries_its_deadline() {
        assert_eq!(
            DEADLINES,
            Deadlines {
                connect: Duration::from_secs(10),
                anthropic: Duration::from_secs(300),
                oauth: Duration::from_secs(30),
                drive_list: Duration::from_secs(30),
                drive_upload: Duration::from_secs(120),
            }
        );
        let _one_at_a_time = crate::test_lock();
        let base = fake(|_, _, _| Reply::Stall);
        let ctx = ctx_at(&base, Duration::from_millis(300));
        let cfg = google_dir(&base);
        let raw = cfg.path().join("minutes.json");
        // token and folder answer; only the upload itself stalls
        let up = drive(|t, _| if t.contains("appProperties") { Reply::Json(200, r#"{"files": []}"#.into()) } else { Reply::Stall });
        let up_ctx = ctx_at(&up, Duration::from_millis(300));
        let up_cfg = google_dir(&up);

        let calls: Vec<(&str, Box<dyn Fn() -> String>)> = vec![
            ("anthropic", Box::new(|| summarize_once(&ctx, "k", "m", "t", "low", &raw).unwrap_err())),
            ("google token", Box::new(|| google_token(&ctx, cfg.path()).unwrap_err())),
            ("drive list", Box::new(|| ensure_folder(&ctx, "tok", FOLDER).unwrap_err())),
            ("drive upload", Box::new(|| upload_doc(&up_ctx, up_cfg.path(), "t", "md", "20261107-143200").unwrap_err())),
        ];
        for (provider, call) in calls {
            let t = std::time::Instant::now();
            let e = call();
            assert!(e.starts_with(provider) && e.contains(DEADLINE_MARK), "{provider}: {e}");
            assert!(t.elapsed() < Duration::from_millis(2500), "{provider} took {:?}", t.elapsed());
        }
    }

    #[test]
    fn a_stalled_anthropic_call_ends_at_its_deadline() {
        let _one_at_a_time = crate::test_lock();
        let base = fake(|_, _, _| Reply::Stall);
        let ctx = ctx_at(&base, Duration::from_millis(500));
        let raw = tempfile::tempdir().unwrap();

        let t = std::time::Instant::now();
        let e = summarize(&ctx, "k", "m", "t", &raw.path().join("minutes.json")).unwrap_err();

        assert!(t.elapsed() >= Duration::from_millis(400) && t.elapsed() < Duration::from_millis(2500), "{:?}", t.elapsed());
        assert!(e.starts_with("anthropic: no answer within 500 ms (deadline)"), "{e}");
    }

    #[test]
    fn a_timeout_names_the_provider_and_the_deadline() {
        let e = call_error("anthropic", DEADLINES.anthropic, ureq::Error::Timeout(ureq::Timeout::Global));

        assert_eq!(e, "anthropic: no answer within 300 s (deadline)");
    }

    #[test]
    fn a_drive_upload_timeout_says_it_may_have_completed_and_keeps_ata_md() {
        let _one_at_a_time = crate::test_lock();
        let base = drive(|t, _| if t.contains("appProperties") { Reply::Json(200, r#"{"files": []}"#.into()) } else { Reply::Stall });
        let ctx = ctx_at(&base, Duration::from_millis(300));
        let cfg = google_dir(&base);
        let (_tmp, dir) = session_with_transcript();

        let e = run_with_drive(&dir, &google(), &minutes_json, &|t, md| upload_doc(&ctx, cfg.path(), t, md, "20261107-143200")).unwrap_err();

        assert!(e.contains("may already have completed") && e.contains("VOXIS Atas") && e.contains(DEADLINE_MARK), "{e}");
        assert!(std::fs::read_to_string(dir.join("ata.md")).unwrap().contains("Fechar sexta."));
    }

    #[test]
    fn ata_md_is_written_before_the_first_drive_call() {
        let _one_at_a_time = crate::test_lock();
        let (_tmp, dir) = session_with_transcript();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (seen2, ata) = (seen.clone(), dir.join("ata.md"));
        let base = fake(move |_, target, _| {
            seen2.lock().unwrap().push(ata.exists());
            match target {
                t if t.starts_with("/token") => Reply::Json(200, r#"{"access_token": "a"}"#.into()),
                t if t.contains("appProperties") => Reply::Json(200, r#"{"files": []}"#.into()),
                t if t.starts_with("/drive") => Reply::Json(200, r#"{"files": [{"id": "F"}]}"#.into()),
                _ => Reply::Json(200, r#"{"id": "D", "webViewLink": "https://docs.google.com/document/d/D"}"#.into()),
            }
        });
        let ctx = ctx_at(&base, Duration::from_secs(5));
        let cfg = google_dir(&base);

        run_with_drive(&dir, &google(), &minutes_json, &|t, md| upload_doc(&ctx, cfg.path(), t, md, "20261107-143200")).unwrap();

        let seen = seen.lock().unwrap();
        assert!(!seen.is_empty() && seen.iter().all(|&x| x), "ata.md before every Drive call: {seen:?}");
    }

    #[test]
    fn a_retry_after_an_upload_that_completed_returns_the_existing_doc() {
        let _one_at_a_time = crate::test_lock();
        let docs: Arc<Mutex<Vec<Value>>> = Arc::default();
        let store = docs.clone();
        let base = drive(move |target, body| {
            if target.contains("appProperties") {
                let id = "20261107-143200";
                let found: Vec<Value> = store.lock().unwrap().iter().filter(|d| d["appProperties"]["voxis_session"] == id && target.contains(id)).cloned().collect();
                return Reply::Json(200, json!({ "files": found }).to_string());
            }
            // the upload lands in Drive, then the answer never comes back
            let meta: Value = body.lines().find(|l| l.starts_with('{')).and_then(|l| serde_json::from_str(l).ok()).unwrap_or_default();
            let n = store.lock().unwrap().len();
            store.lock().unwrap().push(json!({"id": n, "webViewLink": format!("https://docs.google.com/document/d/{n}"), "appProperties": meta["appProperties"]}));
            Reply::Stall
        });
        let ctx = ctx_at(&base, Duration::from_millis(300));
        let cfg = google_dir(&base);
        let (_tmp, dir) = session_with_transcript();
        let up = |t: &str, md: &str| upload_doc(&ctx, cfg.path(), t, md, "20261107-143200");

        let first = run_with_drive(&dir, &google(), &minutes_json, &up);
        let retry = run_with_drive(&dir, &google(), &minutes_json, &up);

        assert!(first.is_err());
        assert_eq!(retry.map(|a| matches!(a, Ata::Doc(_, link) if link == "https://docs.google.com/document/d/0")), Ok(true));
        assert_eq!(docs.lock().unwrap().len(), 1, "no second Doc");
    }

    #[test]
    fn a_new_upload_carries_the_session_app_property() {
        let _one_at_a_time = crate::test_lock();
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let keep = bodies.clone();
        let base = drive(move |target, body| {
            if target.contains("appProperties") {
                return Reply::Json(200, r#"{"files": []}"#.into());
            }
            keep.lock().unwrap().push(body.to_owned());
            Reply::Json(200, r#"{"id": "D", "webViewLink": "https://docs.google.com/document/d/D"}"#.into())
        });
        let ctx = ctx_at(&base, Duration::from_secs(5));
        let cfg = google_dir(&base);

        upload_doc(&ctx, cfg.path(), "2026-11-07 14:32 - Alinhamento", "# Alinhamento", "20261107-143200").unwrap();

        let bodies = bodies.lock().unwrap();
        let meta: Value = bodies[0].lines().find(|l| l.starts_with('{')).map(|l| serde_json::from_str(l).unwrap()).unwrap();
        assert_eq!(meta["appProperties"], json!({"voxis_session": "20261107-143200"}));
    }

    #[test]
    fn a_fresh_ata_md_is_reused_without_asking_anthropic() {
        let (_tmp, dir) = session_with_transcript();
        let old = std::time::SystemTime::now() - Duration::from_secs(600);
        std::fs::File::options().write(true).open(dir.join("transcript.md")).unwrap().set_modified(old).unwrap();
        std::fs::write(dir.join("ata.md"), "# Alinhamento\n\n**Data:** 2026-11-07 14:32\n").unwrap();
        let creds = Credentials { anthropic_key: Some("key".into()), google: false };
        let never = |_: &str| -> Result<Value, String> { panic!("a fresh ata.md must not be summarized again") };

        let ata = run_with(&dir, &creds, &never).unwrap();

        assert_eq!(ata, Ata::Local(dir.join("ata.md")));
        assert!(std::fs::read_to_string(dir.join("ata.md")).unwrap().starts_with("# Alinhamento"));
    }
}
