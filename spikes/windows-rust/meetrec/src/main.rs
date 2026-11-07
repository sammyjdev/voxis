//! Spike for MIRIS on Windows: records a meeting as two tracks, the microphone
//! (you) and the default output device through WASAPI loopback (everyone
//! else), then transcribes both with whisper.cpp and writes one transcript.
//!
//! usage:
//!   meetrec devices
//!   meetrec record <dir> [mic-name-substring] [secs] Enter stops, or after secs
//!   meetrec transcribe <dir> <model.bin> <vad.bin> [lang=pt]
//!   meetrec summarize <dir>                         Claude minutes -> Google Doc
//!   meetrec meeting [mic]                           record (Enter stops) + transcribe + summarize
//!     into %USERPROFILE%\.voxis\meetings\<date>
//!   meetrec models fetch                            download the pinned models (see models.rs)
//! Models come from VOXIS_MODELS, %LOCALAPPDATA%\voxis\models, or the bench
//! cache %USERPROFILE%\.cache\voxis-bench\models, first match wins.

#[cfg(windows)]
mod dictate;
mod minutes;
mod models;
mod session;

use std::io::BufRead;
use std::path::Path;
use std::sync::mpsc;
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperVadParams};

const RATE: u32 = 16_000;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("devices") => devices(a.get(2).map(String::as_str) == Some("--json")),
        Some("record") if a.len() >= 3 => {
            let stop = a.get(4).map(|s| Stop::After(s.parse().expect("secs"))).unwrap_or(Stop::Enter);
            record(Path::new(&a[2]), a.get(3).map(String::as_str).filter(|s| !s.is_empty()), stop, || {});
        }
        Some("transcribe") if a.len() >= 5 => {
            transcribe(Path::new(&a[2]), &a[3], &a[4], a.get(5).map(String::as_str).unwrap_or("pt"))
        }
        Some("meeting") => meeting(a.get(2).map(String::as_str)),
        Some("models") if a.get(2).map(String::as_str) == Some("fetch") => std::process::exit(models_fetch()),
        #[cfg(windows)]
        Some("dictate") => dictate::run(),
        // MIRIS-compatible surface the tray drives (see session.rs)
        Some(cmd @ ("start" | "stop" | "process" | "recover" | "miris.recorder" | "miris.pipeline")) => {
            std::process::exit(session::cli(cmd, &a[2..]))
        }
        Some("summarize") if a.len() >= 3 => {
            if let Err(e) = minutes::run(Path::new(&a[2])) {
                eprintln!("summarize failed: {e}");
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!("usage: meetrec devices | record <dir> [mic] [secs] | transcribe <dir> <model.bin> <vad.bin> [lang] | summarize <dir> | models fetch");
            std::process::exit(2);
        }
    }
}

pub fn home() -> std::path::PathBuf {
    std::env::var_os("USERPROFILE").map(std::path::PathBuf::from).unwrap_or_else(|| ".".into())
}

/// Where this machine keeps the models, in lookup order.
fn model_places() -> Vec<std::path::PathBuf> {
    models::places(
        std::env::var_os("VOXIS_MODELS").map(Into::into),
        std::env::var_os("LOCALAPPDATA").map(Into::into),
        &home(),
    )
}

/// (whisper model, silero vad) from the first place that holds both.
pub fn models() -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    let dir = models::find(&model_places())?;
    Ok((dir.join(models::MODEL), dir.join(models::VAD)))
}

/// `meetrec models fetch`: into VOXIS_MODELS when set, else the installed
/// app's %LOCALAPPDATA%\voxis\models.
fn models_fetch() -> i32 {
    let places = model_places();
    let target = models::fetch_target_from_env(&|k| std::env::var_os(k).map(Into::into), &home());
    let (mut out, mut err) = (std::io::stdout(), std::io::stderr());
    models::fetch(&places, &target, &models::PINS, &mut out, &mut err)
}

fn meeting(mic: Option<&str>) {
    let dir = home().join(".voxis").join("meetings").join(chrono::Local::now().format("%Y-%m-%d_%H%M").to_string());
    let (model, vad) = models().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    println!("sessão: {}", dir.display());
    record(&dir, mic, Stop::Enter, || {});
    transcribe(&dir, &model.to_string_lossy(), &vad.to_string_lossy(), "pt");
    if let Err(e) = minutes::run(&dir) {
        // the transcript is already on disk; summarize can be retried alone
        eprintln!("ata falhou: {e}
rode depois: meetrec summarize {}", dir.display());
        std::process::exit(1);
    }
}

/// `devices --json` is what the tray's Áudio settings list.
fn devices(as_json: bool) {
    let host = cpal::default_host();
    let inputs: Vec<String> = host.input_devices().map(|ds| ds.filter_map(|d| d.name().ok()).collect()).unwrap_or_default();
    let outputs: Vec<String> = host.output_devices().map(|ds| ds.filter_map(|d| d.name().ok()).collect()).unwrap_or_default();
    let default_in = host.default_input_device().and_then(|d| d.name().ok()).unwrap_or_default();
    let default_out = host.default_output_device().and_then(|d| d.name().ok()).unwrap_or_default();
    if as_json {
        println!("{}", serde_json::json!({
            "inputs": inputs, "outputs": outputs,
            "default_input": default_in, "default_output": default_out,
        }));
        return;
    }
    for n in &inputs {
        println!("in : {n}{}", if *n == default_in { "  (padrão)" } else { "" });
    }
    for n in &outputs {
        println!("out: {n}{}", if *n == default_out { "  (padrão)" } else { "" });
    }
}

/// A key of %APPDATA%\voxis\config.toml (the tray's Áudio settings write
/// `mic` and `speaker`); empty or missing is None.
pub fn config_value(key: &str) -> Option<String> {
    config_value_in(&config_text()?, key)
}

/// The text of %APPDATA%\voxis\config.toml, if there is one.
pub fn config_text() -> Option<String> {
    let base = std::env::var_os("APPDATA").map(std::path::PathBuf::from)?;
    std::fs::read_to_string(base.join("voxis").join("config.toml")).ok()
}

/// `key` in a config.toml text; empty or missing is None.
pub fn config_value_in(raw: &str, key: &str) -> Option<String> {
    raw.lines()
        .filter_map(|l| l.split_once('='))
        .filter(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim().trim_matches('"').to_owned())
        .find(|v| !v.is_empty())
}

/// The device named `want` (exact name first, then a substring such as
/// "fifine"), or the system default when nothing is chosen. A chosen device
/// that is unplugged falls back to the default with a warning instead of
/// failing the meeting.
fn pick(all: Vec<cpal::Device>, default: Option<cpal::Device>, want: Option<&str>, what: &str) -> Result<cpal::Device, String> {
    if let Some(w) = want {
        let name = |d: &cpal::Device| d.name().unwrap_or_default();
        let lower = w.to_lowercase();
        if let Some(d) = all.iter().position(|d| name(d) == w).or_else(|| all.iter().position(|d| name(d).to_lowercase().contains(&lower))) {
            return Ok(all.into_iter().nth(d).unwrap());
        }
        eprintln!("{what} '{w}' não encontrado, usando o padrão do sistema");
    }
    default.ok_or_else(|| format!("nenhum {what} disponível"))
}

pub fn pick_input(want: Option<&str>) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    pick(host.input_devices().map(|d| d.collect()).unwrap_or_default(), host.default_input_device(), want, "microfone")
}

pub fn pick_output(want: Option<&str>) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    pick(host.output_devices().map(|d| d.collect()).unwrap_or_default(), host.default_output_device(), want, "dispositivo de saída")
}

/// Mono samples tagged with the moment the callback ran, so the writer can
/// pad gaps: WASAPI loopback delivers nothing while nothing plays, and the two
/// tracks must stay on one clock for the transcript to interleave.
struct Chunk {
    at: f64,
    mono: Vec<f32>,
}

fn open_track(
    device: &cpal::Device,
    config: cpal::SupportedStreamConfig,
    t0: Instant,
    tx: mpsc::Sender<Chunk>,
) -> cpal::Stream {
    let ch = config.channels() as usize;
    let err = |e| eprintln!("stream error: {e}");
    let cfg: cpal::StreamConfig = config.clone().into();
    macro_rules! build {
        ($t:ty, $conv:expr) => {
            device.build_input_stream(
                &cfg,
                move |data: &[$t], _: &_| {
                    let mono = data.chunks(ch).map(|f| f.iter().map(|&s| $conv(s)).sum::<f32>() / ch as f32).collect();
                    let _ = tx.send(Chunk { at: t0.elapsed().as_secs_f64(), mono });
                },
                err,
                None,
            )
        };
    }
    match config.sample_format() {
        cpal::SampleFormat::F32 => build!(f32, |s: f32| s),
        cpal::SampleFormat::I16 => build!(i16, |s: i16| s as f32 / 32768.0),
        cpal::SampleFormat::I32 => build!(i32, |s: i32| s as f32 / 2147483648.0),
        f => panic!("unsupported sample format {f:?}"),
    }
    .expect("build stream")
}

/// Writes a track at the device rate as f32 mono, padding silence whenever
/// the clock says samples are missing. Returns the peak level for a sanity print.
fn writer(path: std::path::PathBuf, rate: u32, rx: mpsc::Receiver<Chunk>) -> std::thread::JoinHandle<(u64, f32)> {
    std::thread::spawn(move || {
        let spec = hound::WavSpec { channels: 1, sample_rate: rate, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut w = hound::WavWriter::create(&path, spec).expect("create wav");
        let (mut written, mut peak) = (0u64, 0f32);
        for c in rx {
            // where this chunk should start on the shared clock
            let due = ((c.at * rate as f64) as u64).saturating_sub(c.mono.len() as u64);
            if due > written + rate as u64 / 10 {
                for _ in written..due {
                    w.write_sample(0.0f32).unwrap();
                }
            }
            for s in c.mono {
                peak = peak.max(s.abs());
                w.write_sample(s).unwrap();
            }
            written = w.len() as u64;
        }
        w.finalize().unwrap();
        (written, peak)
    })
}

pub enum Stop {
    Enter,
    After(f64),
    /// the tray's `meetrec stop` creates this file; polled every 200 ms
    File(std::path::PathBuf),
}

pub struct Track {
    pub frames: u64,
    pub rate: u32,
}

pub fn record(dir: &Path, mic_want: Option<&str>, stop: Stop, on_started: impl FnOnce()) -> (Track, Track) {
    std::fs::create_dir_all(dir).unwrap();
    let mic = pick_input(mic_want).unwrap_or_else(|e| panic!("{e}"));
    let out = pick_output(config_value("speaker").as_deref()).unwrap_or_else(|e| panic!("{e}"));
    let mic_cfg = mic.default_input_config().unwrap();
    // an input stream on an output device is WASAPI loopback
    let out_cfg = out.default_output_config().unwrap();
    println!("mic    : {} ({} Hz, {} ch)", mic.name().unwrap(), mic_cfg.sample_rate().0, mic_cfg.channels());
    println!("system : {} ({} Hz, {} ch, loopback)", out.name().unwrap(), out_cfg.sample_rate().0, out_cfg.channels());

    let t0 = Instant::now();
    let (mtx, mrx) = mpsc::channel();
    let (stx, srx) = mpsc::channel();
    let mw = writer(dir.join("mic.wav"), mic_cfg.sample_rate().0, mrx);
    let sw = writer(dir.join("system.wav"), out_cfg.sample_rate().0, srx);
    let ms = open_track(&mic, mic_cfg.clone(), t0, mtx.clone());
    let ss = open_track(&out, out_cfg.clone(), t0, stx.clone());
    ms.play().unwrap();
    ss.play().unwrap();
    on_started();
    match stop {
        Stop::After(s) => {
            println!("gravando por {s} s...");
            std::thread::sleep(std::time::Duration::from_secs_f64(s));
        }
        Stop::Enter => {
            println!("gravando... Enter para parar");
            let _ = std::io::stdin().lock().lines().next();
        }
        Stop::File(f) => {
            while !f.exists() {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    }
    let secs = t0.elapsed().as_secs_f64();
    drop(ms);
    drop(ss);
    // a final empty chunk pads both tracks to the same length
    let _ = mtx.send(Chunk { at: secs, mono: vec![] });
    let _ = stx.send(Chunk { at: secs, mono: vec![] });
    drop(mtx);
    drop(stx);
    let (mn, mp) = mw.join().unwrap();
    let (sn, sp) = sw.join().unwrap();
    println!("{secs:.1} s  mic {:.1} s peak {mp:.3}  system {:.1} s peak {sp:.3}",
             mn as f64 / mic_cfg.sample_rate().0 as f64, sn as f64 / out_cfg.sample_rate().0 as f64);
    (Track { frames: mn, rate: mic_cfg.sample_rate().0 }, Track { frames: sn, rate: out_cfg.sample_rate().0 })
}

/// Mono f32 at RATE. Box-filter average then linear interpolation: good
/// enough for speech in a spike, a real resampler comes with the port.
fn load_16k(path: &Path) -> Vec<f32> {
    let mut r = hound::WavReader::open(path).expect("open wav");
    let spec = r.spec();
    let ch = spec.channels as usize;
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().map(Result::unwrap).collect(),
        hound::SampleFormat::Int => r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect(),
    };
    let mono: Vec<f32> = raw.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect();
    if mono.is_empty() {
        return mono;
    }
    let ratio = spec.sample_rate as f64 / RATE as f64;
    if (ratio - 1.0).abs() < 1e-9 {
        return mono;
    }
    let k = ratio.floor().max(1.0) as usize;
    let smooth: Vec<f32> = (0..mono.len()).map(|i| {
        let lo = i.saturating_sub(k / 2);
        let hi = (lo + k).min(mono.len());
        mono[lo..hi].iter().sum::<f32>() / (hi - lo) as f32
    }).collect();
    let n = (mono.len() as f64 / ratio) as usize;
    (0..n).map(|i| {
        let x = i as f64 * ratio;
        let j = x as usize;
        let f = (x - j as f64) as f32;
        let b = smooth.get(j + 1).copied().unwrap_or(smooth[j]);
        smooth[j] * (1.0 - f) + b * f
    }).collect()
}

struct Line {
    t0: i64,
    who: &'static str,
    text: String,
}

pub fn transcribe(dir: &Path, model: &str, vad: &str, lang: &str) {
    let mut cp = WhisperContextParameters::default();
    cp.use_gpu(true);
    let t = Instant::now();
    let ctx = WhisperContext::new_with_params(model, cp).expect("load model");
    let mut state = ctx.create_state().unwrap();
    println!("model loaded in {:.1} s", t.elapsed().as_secs_f64());

    let mut lines = Vec::new();
    for (file, who) in [("mic.wav", "Você"), ("system.wav", "Outros")] {
        let path = dir.join(file);
        if !path.exists() {
            continue;
        }
        let audio = load_16k(&path);
        if audio.len() < RATE as usize {
            println!("{file}: under 1 s of audio, skipped");
            continue;
        }
        let mut p = FullParams::new(SamplingStrategy::BeamSearch { beam_size: 5, patience: -1.0 });
        p.set_language(Some(lang));
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_special(false);
        p.set_print_timestamps(false);
        // silence between turns is where Whisper invents "Obrigado."
        p.set_vad_model_path(Some(vad));
        p.enable_vad(true);
        p.set_vad_params(WhisperVadParams::new());
        let t = Instant::now();
        state.full(p, &audio).expect("transcribe");
        let secs = audio.len() as f64 / RATE as f64;
        let el = t.elapsed().as_secs_f64();
        let mut n = 0;
        for seg in state.as_iter() {
            let text = seg.to_str_lossy().unwrap().trim().to_string();
            if !text.is_empty() {
                lines.push(Line { t0: seg.start_timestamp(), who, text });
                n += 1;
            }
        }
        println!("{file}: {secs:.0} s of audio in {el:.1} s ({:.0}x real time), {n} segments", secs / el.max(1e-9));
    }
    let total = lines.len();
    let (md, dropped) = transcript_md(lines);
    std::fs::write(dir.join("transcript.md"), &md).unwrap();
    println!("{} lines ({dropped} repeats dropped) -> {}", total - dropped, dir.join("transcript.md").display());
}

/// Whisper can loop on one phrase for minutes (205 copies in a 15-minute
/// meeting), so a line equal to the same speaker's previous line, ignoring
/// case and punctuation, is dropped. Only lines of three words or more: in
/// that meeting 249 of the 260 repeats were that long, while a short answer
/// ("Sim.") given to two questions in a row is real speech. Returns the
/// markdown and how many lines were dropped.
fn transcript_md(mut lines: Vec<Line>) -> (String, usize) {
    let key = |s: &str| -> String {
        s.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect::<String>()
            .to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
    };
    lines.sort_by_key(|l| l.t0);
    let mut last: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
    let mut md = String::from("# Transcrição\n\n");
    let mut dropped = 0;
    for l in &lines {
        let k = key(&l.text);
        if k.split(' ').count() >= 3 && last.get(l.who) == Some(&k) {
            dropped += 1;
            continue;
        }
        last.insert(l.who, k);
        let s = l.t0 / 100;
        md += &format!("**[{:02}:{:02}] {}:** {}\n\n", s / 60, s % 60, l.who, l.text);
    }
    (md, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(t0: i64, who: &'static str, text: &str) -> Line {
        Line { t0, who, text: text.into() }
    }

    #[test]
    fn a_line_whisper_repeats_for_one_speaker_is_kept_once_at_its_first_time() {
        let lines = vec![
            line(100, "Você", "Eu vou fazer com essa, né?"),
            line(150, "Outros", "Bora."),
            line(200, "Você", "eu vou fazer com essa né"),
            line(300, "Você", "Eu vou fazer com essa,né?"),
        ];

        let (md, dropped) = transcript_md(lines);

        assert_eq!(md, "# Transcrição\n\n\
            **[00:01] Você:** Eu vou fazer com essa, né?\n\n\
            **[00:01] Outros:** Bora.\n\n");
        assert_eq!(dropped, 2);
    }

    #[test]
    fn a_speaker_saying_the_same_thing_again_later_is_kept() {
        let lines = vec![
            line(100, "Você", "Vamos fechar o escopo."),
            line(200, "Você", "Vamos marcar sexta."),
            line(300, "Você", "Vamos fechar o escopo."),
        ];

        let (md, _) = transcript_md(lines);

        assert_eq!(md.matches("Vamos fechar o escopo.").count(), 2);
    }

    #[test]
    fn a_short_answer_given_twice_in_a_row_is_kept() {
        let lines = vec![
            line(100, "Você", "Sim."),
            line(200, "Outros", "E o deploy fica pra sexta?"),
            line(300, "Você", "Sim."),
        ];

        let (md, _) = transcript_md(lines);

        assert_eq!(md.matches("Você:** Sim.").count(), 2);
    }

    #[test]
    fn a_two_word_line_repeated_in_a_row_is_kept() {
        let lines = vec![
            line(100, "Você", "Tá bom."),
            line(200, "Você", "Tá bom."),
        ];

        let (md, dropped) = transcript_md(lines);

        assert_eq!(md.matches("Tá bom.").count(), 2);
        assert_eq!(dropped, 0);
    }

    #[test]
    fn a_three_word_line_repeated_in_a_row_is_kept_once() {
        let lines = vec![
            line(100, "Você", "Tá me ouvindo?"),
            line(200, "Você", "Tá me ouvindo?"),
        ];

        let (md, dropped) = transcript_md(lines);

        assert_eq!(md.matches("Tá me ouvindo?").count(), 1);
        assert_eq!(dropped, 1);
    }

    #[test]
    fn another_speaker_repeating_the_same_words_is_kept() {
        let lines = vec![
            line(100, "Você", "vamos marcar sexta então"),
            line(200, "Outros", "vamos marcar sexta então"),
        ];

        let (md, _) = transcript_md(lines);

        assert_eq!(md.matches("vamos marcar sexta então").count(), 2);
    }
}
