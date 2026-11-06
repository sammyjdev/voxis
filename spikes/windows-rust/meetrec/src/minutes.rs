//! Meeting minutes: transcript -> Claude (structured JSON; model from config `minutes_model`) -> Markdown
//! -> Google Doc in the "VOXIS Atas" Drive folder.
//!
//! Secrets live in %APPDATA%\voxis (the OS config dir elsewhere):
//!   secrets.toml               anthropic_api_key = "..." (ANTHROPIC_API_KEY wins)
//!   google-oauth-client.json   OAuth desktop client from the voxis GCP project
//!   google-token.json          {"refresh_token": "..."} from the first authorization

use std::path::{Path, PathBuf};

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

fn config_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| ".".into());
    base.join("voxis")
}

fn agent() -> ureq::Agent {
    // read 4xx/5xx bodies ourselves: the API error text is the useful part
    ureq::Agent::config_builder().http_status_as_error(false).build().into()
}

fn check(mut resp: ureq::http::Response<ureq::Body>, what: &str) -> Result<Value, String> {
    let status = resp.status();
    let body: Value = resp.body_mut().read_json().map_err(|e| format!("{what}: {e}"))?;
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
pub fn summarize(model: &str, transcript: &str, raw_log: &Path) -> Result<(Value, Value), String> {
    let mut last = Value::Null;
    for effort in ["low", "medium"] {
        let (m, usage) = summarize_once(model, transcript, effort, raw_log)?;
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

fn summarize_once(model: &str, transcript: &str, effort: &str, raw_log: &Path) -> Result<(Value, Value), String> {
    let body = request_body(model, effort, transcript);
    let resp = agent()
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key()?)
        .header("anthropic-version", "2026-10-21")
        .send_json(&body)
        .map_err(|e| format!("anthropic: {e}"))?;
    // an unknown model comes back as a 404 naming it; point at where it is set
    let msg = check(resp, "anthropic")
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

fn google_token() -> Result<String, String> {
    let client: Value = serde_json::from_str(
        &std::fs::read_to_string(config_dir().join("google-oauth-client.json")).map_err(|e| format!("oauth client: {e}"))?,
    )
    .map_err(|e| e.to_string())?;
    let c = &client["installed"];
    let tok: Value = serde_json::from_str(
        &std::fs::read_to_string(config_dir().join("google-token.json"))
            .map_err(|e| format!("not authorized with Google yet (google-token.json): {e}"))?,
    )
    .map_err(|e| e.to_string())?;
    let resp = agent()
        .post(c["token_uri"].as_str().unwrap_or("https://oauth2.googleapis.com/token"))
        .send_form([
            ("client_id", c["client_id"].as_str().unwrap_or("")),
            ("client_secret", c["client_secret"].as_str().unwrap_or("")),
            ("refresh_token", tok["refresh_token"].as_str().unwrap_or("")),
            ("grant_type", "refresh_token"),
        ])
        .map_err(|e| format!("google token: {e}"))?;
    let v = check(resp, "google token")?;
    v["access_token"].as_str().map(str::to_owned).ok_or_else(|| format!("google token: {v}"))
}

fn ensure_folder(token: &str, name: &str) -> Result<String, String> {
    let q = format!("name='{name}' and mimeType='application/vnd.google-apps.folder' and trashed=false");
    let resp = agent()
        .get("https://www.googleapis.com/drive/v3/files")
        .query("q", &q)
        .query("fields", "files(id)")
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| format!("drive list: {e}"))?;
    let found = check(resp, "drive list")?;
    if let Some(id) = found["files"][0]["id"].as_str() {
        return Ok(id.to_owned());
    }
    let resp = agent()
        .post("https://www.googleapis.com/drive/v3/files?fields=id")
        .header("Authorization", &format!("Bearer {token}"))
        .send_json(&json!({"name": name, "mimeType": "application/vnd.google-apps.folder"}))
        .map_err(|e| format!("drive mkdir: {e}"))?;
    let v = check(resp, "drive mkdir")?;
    v["id"].as_str().map(str::to_owned).ok_or_else(|| format!("drive mkdir: {v}"))
}

/// Uploads Markdown; Drive converts it into a Google Doc (headings, bold, checklists).
pub fn upload_doc(title: &str, markdown: &str) -> Result<Value, String> {
    let token = google_token()?;
    let folder = ensure_folder(&token, FOLDER)?;
    let meta = json!({"name": title, "parents": [folder], "mimeType": "application/vnd.google-apps.document"});
    let boundary = format!("voxis{:x}", std::process::id() as u64 ^ 0x9e37_79b9_7f4a_7c15);
    let body = format!(
        "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n\
         --{boundary}\r\nContent-Type: text/markdown; charset=UTF-8\r\n\r\n{markdown}\r\n--{boundary}--\r\n"
    );
    let resp = agent()
        .post("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=id,name,webViewLink")
        .header("Authorization", &format!("Bearer {token}"))
        .header("Content-Type", &format!("multipart/related; boundary={boundary}"))
        .send(body.as_bytes())
        .map_err(|e| format!("drive upload: {e}"))?;
    check(resp, "drive upload")
}

/// summarize -> render -> ata.md next to the recording -> Google Doc.
/// Returns (doc title, Google Doc link).
pub fn run(dir: &Path) -> Result<(String, String), String> {
    let transcript = std::fs::read_to_string(dir.join("transcript.md"))
        .map_err(|e| format!("{}: {e} (run transcribe first)", dir.join("transcript.md").display()))?;
    let date = recording_date(dir);
    let t = std::time::Instant::now();
    let model = minutes_model(crate::config_text().as_deref());
    let (m, usage) = summarize(&model, &transcript, &dir.join("minutes.json"))?;
    println!("{model}: {:.1} s, {} in / {} out tokens", t.elapsed().as_secs_f64(), usage["input_tokens"], usage["output_tokens"]);
    let md = render(&m, &date, &transcript);
    std::fs::write(dir.join("ata.md"), &md).map_err(|e| e.to_string())?;
    let title = format!("{} - {}", date, m["titulo"].as_str().unwrap_or("Reunião"));
    let doc = upload_doc(&title, &md)?;
    let link = doc["webViewLink"].as_str().unwrap_or("").to_owned();
    println!("ata: {link}");
    Ok((title, link))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
