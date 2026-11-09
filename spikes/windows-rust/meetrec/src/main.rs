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
//!   meetrec models fetch [--verify]                 download the pinned models (see models.rs);
//!                                                   --verify re-hashes the files already there
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
/// The one recognition language on Windows (dictation and meetings).
pub const LANGUAGE: &str = "pt";

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("devices") => devices(a.get(2).map(String::as_str) == Some("--json")),
        Some("record") if a.len() >= 3 => {
            let stop = a.get(4).map(|s| Stop::After(s.parse().expect("secs"))).unwrap_or(Stop::Enter);
            if let Err(e) = record(Path::new(&a[2]), a.get(3).map(String::as_str).filter(|s| !s.is_empty()), stop, || {}) {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        Some("transcribe") if a.len() >= 5 => {
            transcribe(Path::new(&a[2]), &a[3], &a[4], a.get(5).map(String::as_str).unwrap_or("pt"))
        }
        Some("meeting") => meeting(a.get(2).map(String::as_str)),
        Some("models") if a.get(2).map(String::as_str) == Some("fetch") => {
            std::process::exit(models_fetch(a.get(3).map(String::as_str) == Some("--verify")))
        }
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

/// `meetrec models fetch [--verify]`: into VOXIS_MODELS when set, else the
/// installed app's %LOCALAPPDATA%\voxis\models. `--verify` hashes the files
/// already there against their pins.
fn models_fetch(verify: bool) -> i32 {
    let places = model_places();
    let target = models::fetch_target_from_env(&|k| std::env::var_os(k).map(Into::into), &home());
    let (mut out, mut err) = (std::io::stdout(), std::io::stderr());
    if verify {
        models::fetch_verify(&places, &target, &models::PINS, &mut out, &mut err)
    } else {
        models::fetch(&places, &target, &models::PINS, &mut out, &mut err)
    }
}

/// The folder of a `meetrec meeting` run: `meetings/<YYYY-MM-DD_HHMM>`, or
/// `_2` to `_99` when a run in the same minute holds it. Created with a call
/// that fails on an existing path, so a second run never writes into the
/// first one's folder.
fn reserve_meeting_dir(base: &Path, stamp: &str) -> Result<std::path::PathBuf, String> {
    std::fs::create_dir_all(base).map_err(|e| format!("{}: {e}", base.display()))?;
    for n in 1..=99 {
        let dir = if n == 1 { base.join(stamp) } else { base.join(format!("{stamp}_{n}")) };
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("{}: {e}", dir.display())),
        }
    }
    Err(format!("no free session folder for {} in {}", stamp, base.display()))
}

fn meeting(mic: Option<&str>) {
    let base = home().join(".voxis").join("meetings");
    let (model, vad) = models().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H%M").to_string();
    let dir = reserve_meeting_dir(&base, &stamp).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    println!("sessão: {}", dir.display());
    if let Err(e) = record(&dir, mic, Stop::Enter, || {}) {
        eprintln!("{e}");
        std::process::exit(1);
    }
    transcribe(&dir, &model.to_string_lossy(), &vad.to_string_lossy(), LANGUAGE);
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
    config_value_logged(raw, key, &mut std::io::stderr())
}

/// `config_value_in`: the file parsed as TOML, as the tray's reader parses
/// it (a top-level string; comments, quoting and escapes per the spec). A
/// file that is not TOML is reported to `err` and every key is unset, so
/// the defaults apply and nothing exits.
fn config_value_logged(raw: &str, key: &str, err: &mut dyn std::io::Write) -> Option<String> {
    match raw.parse::<toml_edit::DocumentMut>() {
        Ok(doc) => doc.get(key).and_then(|v| v.as_str()).filter(|v| !v.is_empty()).map(str::to_owned),
        Err(e) => {
            let msg = e.to_string();
            let _ = writeln!(err, "config.toml invalid: {}", msg.split_whitespace().collect::<Vec<_>>().join(" "));
            None
        }
    }
}

/// The device named `want` (exact name first, then a substring such as
/// "fifine"), or the system default when nothing is chosen. A chosen device
/// that is unplugged falls back to the default with a warning instead of
/// failing the meeting.
fn pick(all: Vec<cpal::Device>, default: Option<cpal::Device>, want: Option<&str>, what: &str, warn: &mut dyn FnMut(&str)) -> Result<cpal::Device, String> {
    let names: Vec<String> = all.iter().map(|d| d.name().unwrap_or_default()).collect();
    let (index, warning) = choose_device(&names, want, what);
    if let Some(w) = warning {
        warn(&w);
    }
    match index {
        Some(i) => Ok(all.into_iter().nth(i).unwrap()),
        None => default.ok_or_else(|| format!("nenhum {what} disponível")),
    }
}

/// Which of `names` the user chose (exact name first, then a substring such
/// as "fifine"); None is the system default, with the warning to log when a
/// chosen device is absent.
fn choose_device(names: &[String], want: Option<&str>, what: &str) -> (Option<usize>, Option<String>) {
    let Some(w) = want else { return (None, None) };
    let lower = w.to_lowercase();
    match names.iter().position(|n| n == w).or_else(|| names.iter().position(|n| n.to_lowercase().contains(&lower))) {
        Some(i) => (Some(i), None),
        None => (None, Some(format!("{what} '{w}' não encontrado, usando o padrão do sistema"))),
    }
}

/// The input device; a chosen one that is absent is warned on stderr (the
/// recorder's recorder.log).
pub fn pick_input(want: Option<&str>) -> Result<cpal::Device, String> {
    pick_input_with(want, &mut |w| eprintln!("{w}"))
}

/// `pick_input` with the warning going to `warn` (dictation's own log).
pub fn pick_input_with(want: Option<&str>, warn: &mut dyn FnMut(&str)) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    pick(host.input_devices().map(|d| d.collect()).unwrap_or_default(), host.default_input_device(), want, "microfone", warn)
}

pub fn pick_output(want: Option<&str>) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    pick(host.output_devices().map(|d| d.collect()).unwrap_or_default(), host.default_output_device(), want, "dispositivo de saída", &mut |w| eprintln!("{w}"))
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
fn writer(
    path: std::path::PathBuf,
    rate: u32,
    rx: mpsc::Receiver<Chunk>,
    failed: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::thread::JoinHandle<Result<(u64, f32), String>> {
    std::thread::spawn(move || {
        let result = std::fs::File::create(&path)
            .map_err(|e| format!("disk_write_error: {}: {e}", path.display()))
            .and_then(|file| {
                let reopen = || std::fs::OpenOptions::new().read(true).write(true).open(&path);
                write_wav(file, rate, rx, reopen)
            });
        if result.is_err() {
            // ends the recording loop: there is no point waiting for a stop
            failed.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        result
    })
}

/// The track on `inner` through the buffered writer production uses
/// (`hound::WavWriter::create` is this over a `File`). A failed flush stops
/// the writer before it can update the header, so a failure is followed by a
/// repair of the sizes from the bytes on disk, through `reopen`, which needs
/// no free space.
fn write_wav<W, R>(mut inner: W, rate: u32, rx: mpsc::Receiver<Chunk>, reopen: impl FnOnce() -> std::io::Result<R>) -> Result<(u64, f32), String>
where
    W: std::io::Write + std::io::Seek,
    R: std::io::Read + std::io::Write + std::io::Seek,
{
    // the header reaches the file before the first chunk, so a recorder killed before any sample leaves a 68-byte
    // track; it is written straight to the file and the buffered writer starts at 0 as before, so its writes keep
    // their alignment and a failure loses no more than before
    let result = match write_header_through(&mut inner, rate) {
        Err(e) => Err(format!("disk_write_error: {e}")),
        Ok(()) => {
            let w = hound::WavWriter::new(std::io::BufWriter::new(inner), track_spec(rate)).map_err(|e| format!("disk_write_error: {e}"))?;
            write_track(w, rate, rx)
        }
    };
    if result.is_err() {
        if let Err(repair) = reopen().map_err(|e| e.to_string()).and_then(|mut f| session::repair_wav_in(&mut f)) {
            return result.map_err(|e| format!("{e}; header repair failed: {repair}"));
        }
    }
    result
}

/// The header hound writes for a track, written to `inner` and flushed, then `inner` back at 0.
fn write_header_through<W: std::io::Write + std::io::Seek>(inner: &mut W, rate: u32) -> std::io::Result<()> {
    let mut head = std::io::Cursor::new(Vec::new());
    drop(hound::WavWriter::new(&mut head, track_spec(rate)).map_err(std::io::Error::other)?);
    inner.write_all(head.get_ref())?;
    inner.flush()?;
    inner.seek(std::io::SeekFrom::Start(0)).map(|_| ())
}

fn track_spec(rate: u32) -> hound::WavSpec {
    hound::WavSpec { channels: 1, sample_rate: rate, bits_per_sample: 32, sample_format: hound::SampleFormat::Float }
}

/// The track's samples into `w`, padded on the shared clock. A failed
/// write (a full disk) stops the track: the header is finalized over the
/// samples already on disk, so the file stays playable and recoverable.
fn write_track<W: std::io::Write + std::io::Seek>(mut w: hound::WavWriter<W>, rate: u32, rx: mpsc::Receiver<Chunk>) -> Result<(u64, f32), String> {
    let (mut written, mut peak) = (0u64, 0f32);
    let mut failure = None;
    'chunks: for c in rx {
        // where this chunk should start on the shared clock
        let due = ((c.at * rate as f64) as u64).saturating_sub(c.mono.len() as u64);
        let pad = if due > written + rate as u64 / 10 { due - written } else { 0 };
        for s in std::iter::repeat_n(0.0f32, pad as usize).chain(c.mono) {
            peak = peak.max(s.abs());
            if let Err(e) = w.write_sample(s) {
                failure = Some(e);
                break 'chunks;
            }
        }
        written = w.len() as u64;
    }
    let finalized = w.finalize();
    match (failure, finalized) {
        (Some(e), _) => Err(format!("disk_write_error: {e}")),
        (None, Err(e)) => Err(format!("disk_write_error: {e}")),
        (None, Ok(())) => Ok((written, peak)),
    }
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

/// The folder a recording writes into: created when missing, refused when
/// it already holds a track, before any writer opens (a writer would
/// truncate it).
fn prepare_record_dir(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for track in ["mic.wav", "system.wav"] {
        if dir.join(track).exists() {
            return Err(format!("{} already holds {track}; refusing to overwrite a recording", dir.display()));
        }
    }
    Ok(())
}

pub fn record(dir: &Path, mic_want: Option<&str>, stop: Stop, on_started: impl FnOnce()) -> Result<(Track, Track), String> {
    prepare_record_dir(dir)?;
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
    let failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mw = writer(dir.join("mic.wav"), mic_cfg.sample_rate().0, mrx, failed.clone());
    let sw = writer(dir.join("system.wav"), out_cfg.sample_rate().0, srx, failed.clone());
    let failing = || failed.load(std::sync::atomic::Ordering::SeqCst);
    let ms = open_track(&mic, mic_cfg.clone(), t0, mtx.clone());
    let ss = open_track(&out, out_cfg.clone(), t0, stx.clone());
    ms.play().unwrap();
    ss.play().unwrap();
    on_started();
    match stop {
        Stop::After(s) => {
            println!("gravando por {s} s...");
            while t0.elapsed().as_secs_f64() < s && !failing() {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
        Stop::Enter => {
            println!("gravando... Enter para parar");
            let _ = std::io::stdin().lock().lines().next();
        }
        Stop::File(f) => {
            while !f.exists() && !failing() {
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
    // both writers finish (and finalize) before either error is returned
    let (mic_done, sys_done) = (mw.join().unwrap(), sw.join().unwrap());
    let (mn, mp) = mic_done?;
    let (sn, sp) = sys_done?;
    println!("{secs:.1} s  mic {:.1} s peak {mp:.3}  system {:.1} s peak {sp:.3}",
             mn as f64 / mic_cfg.sample_rate().0 as f64, sn as f64 / out_cfg.sample_rate().0 as f64);
    Ok((Track { frames: mn, rate: mic_cfg.sample_rate().0 }, Track { frames: sn, rate: out_cfg.sample_rate().0 }))
}

/// Mono f32 at RATE. Box-filter average then linear interpolation: good
/// enough for speech in a spike, a real resampler comes with the port.
/// A file that cannot be read gives an error naming it, never a panic.
fn load_16k(path: &Path) -> Result<Vec<f32>, String> {
    let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    let mut r = hound::WavReader::open(path).map_err(|e| format!("{name}: {e}"))?;
    let spec = r.spec();
    let ch = spec.channels as usize;
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().collect::<Result<_, _>>(),
        hound::SampleFormat::Int => r.samples::<i16>().map(|s| s.map(|s| s as f32 / 32768.0)).collect::<Result<_, _>>(),
    }
    .map_err(|e| format!("{name}: {e}"))?;
    let mono: Vec<f32> = raw.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect();
    if mono.is_empty() {
        return Ok(mono);
    }
    let ratio = spec.sample_rate as f64 / RATE as f64;
    if (ratio - 1.0).abs() < 1e-9 {
        return Ok(mono);
    }
    let k = ratio.floor().max(1.0) as usize;
    let smooth: Vec<f32> = (0..mono.len()).map(|i| {
        let lo = i.saturating_sub(k / 2);
        let hi = (lo + k).min(mono.len());
        mono[lo..hi].iter().sum::<f32>() / (hi - lo) as f32
    }).collect();
    let n = (mono.len() as f64 / ratio) as usize;
    Ok((0..n).map(|i| {
        let x = i as f64 * ratio;
        let j = x as usize;
        let f = (x - j as f64) as f32;
        let b = smooth.get(j + 1).copied().unwrap_or(smooth[j]);
        smooth[j] * (1.0 - f) + b * f
    }).collect())
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
        // a track that cannot be read still stops the transcription here, as before (AC 14 skip and AC 15 settle are later checks)
        let audio = load_16k(&path).unwrap_or_else(|e| panic!("open wav: {e}"));
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

/// Tests that open loopback sockets or start processes run one at a time:
/// a process started while another test's socket is open resets that
/// connection when it exits.
#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
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

    fn sha(path: &Path) -> Vec<u8> {
        use sha2::Digest;
        sha2::Sha256::digest(std::fs::read(path).unwrap()).to_vec()
    }

    // ------------------------------------------------------------ S4 config and devices (F20)

    #[test]
    fn config_toml_values_are_read_as_toml_like_the_tray() {
        let cases = [
            ("mic = \"USB Microphone\" # desk\n", Some("USB Microphone")),
            ("mic = 'USB Mic'\n", Some("USB Mic")),
            ("mic = \"A \\\"B\\\"\"\n", Some("A \"B\"")),
            ("[audio]\nmic = \"nested\"\n", None),
            ("mic = \"\"\n", None),
        ];

        for (raw, want) in cases {
            assert_eq!(config_value_logged(raw, "mic", &mut Vec::new()).as_deref(), want, "{raw}");
        }
        let all = "mic = \"fifine\" # mesa\nspeaker = 'Alto-falantes'\nminutes_model = \"claude-haiku-5-5\" # mais barato\n";
        assert_eq!(config_value_logged(all, "speaker", &mut Vec::new()).as_deref(), Some("Alto-falantes"));
        assert_eq!(config_value_logged(all, "minutes_model", &mut Vec::new()).as_deref(), Some("claude-haiku-5-5"));
    }

    #[test]
    fn an_invalid_config_toml_is_reported_and_the_defaults_are_used() {
        let mut err = Vec::new();

        let mic = config_value_logged("mic = \"USB\nspeaker = 3 =\n", "mic", &mut err);

        let err = String::from_utf8(err).unwrap();
        assert_eq!(mic, None, "the default device");
        assert!(err.starts_with("config.toml invalid: "), "{err}");
    }

    #[test]
    fn a_chosen_device_is_used_and_an_absent_one_falls_back_with_a_warning() {
        let names: Vec<String> = ["Microfone (Realtek)", "Microfone (2- fifine Microphone)"].iter().map(|s| s.to_string()).collect();

        assert_eq!(choose_device(&names, Some("Microfone (Realtek)"), "microfone"), (Some(0), None));
        assert_eq!(choose_device(&names, Some("fifine"), "microfone"), (Some(1), None));
        assert_eq!(choose_device(&names, None, "microfone"), (None, None));
        assert_eq!(
            choose_device(&names, Some("Anker PowerConf"), "microfone"),
            (None, Some("microfone 'Anker PowerConf' não encontrado, usando o padrão do sistema".to_owned()))
        );
    }

    // ------------------------------------------------------------ S2 meeting folders (F18)

    #[test]
    fn the_meeting_folder_is_reserved_with_a_create_new_call() {
        let base = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(base.path().join("2026-11-07_1432")).unwrap();

        let dir = reserve_meeting_dir(base.path(), "2026-11-07_1432").unwrap();

        assert_ne!(dir, base.path().join("2026-11-07_1432"), "an existing folder must never be reused");
        assert!(dir.is_dir());
    }

    #[test]
    fn a_second_meeting_in_the_same_minute_gets_its_own_folder_and_keeps_the_first() {
        let base = tempfile::tempdir().unwrap();
        let first = reserve_meeting_dir(base.path(), "2026-11-07_1432").unwrap();
        std::fs::write(first.join("mic.wav"), b"first meeting").unwrap();
        let before = sha(&first.join("mic.wav"));

        let second = reserve_meeting_dir(base.path(), "2026-11-07_1432").unwrap();

        assert_eq!(first, base.path().join("2026-11-07_1432"));
        assert_eq!(second, base.path().join("2026-11-07_1432_2"));
        assert_eq!(sha(&first.join("mic.wav")), before);
    }

    #[test]
    fn after_99_folders_in_one_minute_the_meeting_refuses_with_no_free_session_folder() {
        let base = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(base.path().join("2026-11-07_1432")).unwrap();
        for n in 2..=99 {
            std::fs::create_dir_all(base.path().join(format!("2026-11-07_1432_{n}"))).unwrap();
        }

        let e = reserve_meeting_dir(base.path(), "2026-11-07_1432").unwrap_err();

        assert!(e.contains("no free session folder"), "{e}");
        assert!(!base.path().join("2026-11-07_1432_100").exists());
    }

    #[test]
    fn recording_into_a_folder_that_already_holds_a_track_is_refused_and_leaves_it_unchanged() {
        for track in ["mic.wav", "system.wav"] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join(track), b"an earlier recording").unwrap();

            let refused = prepare_record_dir(dir.path());

            assert!(refused.is_err(), "{track} must be refused");
            assert_eq!(std::fs::read(dir.path().join(track)).unwrap(), b"an earlier recording");
        }
    }

    #[test]
    fn recording_into_a_missing_or_empty_folder_is_allowed() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("new");
        let empty = tmp.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();

        assert_eq!(prepare_record_dir(&missing), Ok(()));
        assert_eq!(prepare_record_dir(&empty), Ok(()));
        assert!(missing.is_dir());
    }

    /// A file that accepts `cap` bytes, then fails like a full disk; bytes
    /// already written can still be rewritten (the WAV header). The bytes
    /// are shared so the test reads what reached the "disk".
    #[derive(Clone)]
    struct FullDisk {
        data: std::rc::Rc<std::cell::RefCell<std::io::Cursor<Vec<u8>>>>,
        cap: u64,
    }

    impl std::io::Write for FullDisk {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let mut d = self.data.borrow_mut();
            if d.position() + buf.len() as u64 > self.cap {
                return Err(std::io::Error::other("disk full"));
            }
            std::io::Write::write(&mut *d, buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl std::io::Seek for FullDisk {
        fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
            std::io::Seek::seek(&mut *self.data.borrow_mut(), pos)
        }
    }

    #[test]
    fn a_failed_wav_write_finalizes_the_header_of_the_samples_written() {
        let mut header = std::io::Cursor::new(Vec::new());
        hound::WavWriter::new(&mut header, track_spec(16_000)).unwrap().finalize().unwrap();
        let header_len = header.get_ref().len() as u64;
        let disk = FullDisk { data: Default::default(), cap: header_len + 100 * 4 };
        let w = hound::WavWriter::new(disk.clone(), track_spec(16_000)).unwrap();
        let (tx, rx) = mpsc::channel();
        tx.send(Chunk { at: 0.0, mono: vec![0.5; 400] }).unwrap();
        drop(tx);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| write_track(w, 16_000, rx)));

        let result = result.expect("a write failure must not panic");
        let e = result.unwrap_err();
        assert!(e.starts_with("disk_write_error"), "{e}");
        let bytes = disk.data.borrow().get_ref().clone();
        let r = hound::WavReader::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(r.len(), 100, "the header counts the 100 samples that reached the disk");
    }

    /// Like a full disk: takes bytes up to `cap` (a write that crosses it is
    /// partial), then fails; bytes already on it can be read and rewritten.
    #[derive(Clone)]
    struct PartialDisk {
        data: std::rc::Rc<std::cell::RefCell<std::io::Cursor<Vec<u8>>>>,
        cap: u64,
    }

    impl std::io::Write for PartialDisk {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let mut d = self.data.borrow_mut();
            let room = self.cap.saturating_sub(d.position()) as usize;
            if room == 0 {
                return Err(std::io::Error::other("disk full"));
            }
            std::io::Write::write(&mut *d, &buf[..buf.len().min(room)])
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl std::io::Seek for PartialDisk {
        fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
            std::io::Seek::seek(&mut *self.data.borrow_mut(), pos)
        }
    }

    impl std::io::Read for PartialDisk {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            std::io::Read::read(&mut *self.data.borrow_mut(), buf)
        }
    }

    fn recorded_samples_after_a_full_disk(cap_after_header: u64) -> (String, usize) {
        let mut header = std::io::Cursor::new(Vec::new());
        hound::WavWriter::new(&mut header, track_spec(16_000)).unwrap().finalize().unwrap();
        let disk = PartialDisk { data: Default::default(), cap: header.get_ref().len() as u64 + cap_after_header };
        let (tx, rx) = mpsc::channel();
        tx.send(Chunk { at: 0.0, mono: vec![0.5; 400] }).unwrap();
        drop(tx);

        let again = disk.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| write_wav(disk.clone(), 16_000, rx, || Ok::<_, std::io::Error>(again))));

        let e = result.expect("a write failure must not panic").unwrap_err();
        let bytes = disk.data.borrow().get_ref().clone();
        let samples = hound::WavReader::new(std::io::Cursor::new(bytes)).expect("a header that can be read").len() as usize;
        (e, samples)
    }

    #[test]
    fn a_full_disk_under_the_buffered_writer_leaves_a_wav_that_counts_the_samples_on_disk() {
        let (e, samples) = recorded_samples_after_a_full_disk(100 * 4);

        assert!(e.starts_with("disk_write_error"), "{e}");
        assert!(!e.contains("header repair failed"), "{e}");
        assert_eq!(samples, 100, "the header counts the 100 whole samples that reached the disk");
    }

    #[test]
    fn a_track_that_cannot_be_created_ends_in_an_error_not_a_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("no-such-folder").join("mic.wav");
        let (tx, rx) = mpsc::channel();
        tx.send(Chunk { at: 0.0, mono: vec![0.25; 160] }).unwrap();
        drop(tx);
        let failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let ended = writer(path, 16_000, rx, failed.clone()).join();

        let result = ended.expect("the writer thread must not panic");
        assert!(result.unwrap_err().starts_with("disk_write_error"));
        assert!(failed.load(std::sync::atomic::Ordering::SeqCst), "the recording loop is told to end");
    }

    // A failure during writing on a real `File`, with nothing filling a disk: the track file is locked
    // from 1 MiB on through a second handle (`LockFileEx`), so the first flush that reaches the locked
    // range fails by itself. The same recipe runs against the frozen `writer` in
    // .specs/features/windows-port/frozen-repro/main.midwrite.rs.inc.
    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        event: *mut std::ffi::c_void,
    }

    extern "system" {
        fn LockFileEx(file: *mut std::ffi::c_void, flags: u32, reserved: u32, low: u32, high: u32, overlapped: *mut Overlapped) -> i32;
    }

    const LOCK_FROM: u32 = 1 << 20;

    fn locked_track(dir: &std::path::Path) -> (std::path::PathBuf, std::fs::File) {
        use std::os::windows::io::AsRawHandle;
        let path = dir.join("mic.wav");
        let locker = std::fs::File::create(&path).unwrap();
        let mut o = Overlapped { internal: 0, internal_high: 0, offset: LOCK_FROM, offset_high: 0, event: std::ptr::null_mut() };
        let ok = unsafe { LockFileEx(locker.as_raw_handle() as *mut _, 3, 0, u32::MAX, 0, &mut o) };
        assert_ne!(ok, 0, "LockFileEx failed: {}", std::io::Error::last_os_error());
        (path, locker)
    }

    fn quiet_chunks(count: usize) -> mpsc::Receiver<Chunk> {
        let (tx, rx) = mpsc::channel();
        for i in 0..count {
            tx.send(Chunk { at: i as f64 * 0.25, mono: vec![0.25; 4096] }).unwrap();
        }
        rx
    }

    #[test]
    fn a_track_below_the_locked_range_is_written_and_finalized_on_a_real_file() {
        let tmp = tempfile::tempdir().unwrap();
        let (path, _locker) = locked_track(tmp.path());
        let failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let (written, _) = writer(path.clone(), 16_000, quiet_chunks(10), failed.clone()).join().unwrap().unwrap();

        assert_eq!(written, 10 * 4096);
        assert_eq!(hound::WavReader::open(&path).unwrap().len(), 10 * 4096);
        assert!(!failed.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn a_write_that_fails_in_the_middle_of_a_track_on_a_real_file_is_an_error_and_the_data_on_disk_is_recoverable() {
        let tmp = tempfile::tempdir().unwrap();
        let (path, _locker) = locked_track(tmp.path());
        let failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let ended = writer(path.clone(), 16_000, quiet_chunks(100), failed.clone()).join();

        let e = ended.expect("the writer thread must not panic").unwrap_err();
        assert!(e.starts_with("disk_write_error"), "{e}");
        assert!(!e.contains("header repair failed"), "the header was repaired from the bytes on disk: {e}");
        assert!(failed.load(std::sync::atomic::Ordering::SeqCst), "the recording loop is told to end");
        // exactly the file's length: a read at the end of file would touch the locked range
        let len = std::fs::metadata(&path).unwrap().len() as usize;
        let mut bytes = vec![0u8; len];
        std::io::Read::read_exact(&mut std::fs::File::open(&path).unwrap(), &mut bytes).unwrap();
        assert_eq!(bytes.len() as u32, LOCK_FROM, "everything before the locked range reached the disk, nothing after it");
        let data_at = bytes.windows(4).position(|w| w == b"data").unwrap() + 8;
        let reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.len() as usize, (bytes.len() - data_at) / 4, "the header counts every whole sample that reached the disk");
        assert!(reader.len() > 0);
    }

    #[test]
    fn a_half_written_sample_at_the_end_is_left_out_of_the_header() {
        let (_, samples) = recorded_samples_after_a_full_disk(100 * 4 + 2);

        assert_eq!(samples, 100);
    }

    #[test]
    fn a_disk_that_took_less_than_a_header_reports_the_repair_failure_with_the_write_error() {
        let disk = PartialDisk { data: Default::default(), cap: 10 };
        let (tx, rx) = mpsc::channel();
        tx.send(Chunk { at: 0.0, mono: vec![0.5; 4] }).unwrap();
        drop(tx);
        let again = disk.clone();

        let e = write_wav(disk, 16_000, rx, || Ok::<_, std::io::Error>(again)).unwrap_err();

        assert!(e.starts_with("disk_write_error") && e.contains("header repair failed"), "{e}");
    }
}

// M7 CP4 test-only module for main.rs (checks C26, C28 and C37 of .specs/features/m7-silent-track-recovery/checks.md).
// Appended unchanged to main.rs at c2d7b6c and at HEAD; tests only, no production line. Fixtures are synthetic
// bytes in a new folder under the run's TEMP, never removed by the test.
#[cfg(test)]
mod m7_tests {
    use super::*;
    use std::io::{Seek, Write};

    fn le32(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    /// The 68-byte product header (hound 3.5.1, 32-bit float mono) with R = 0 and D = 0; fixed fields checked.
    fn hk() -> Vec<u8> {
        let spec = hound::WavSpec { channels: 1, sample_rate: 48_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut c = std::io::Cursor::new(Vec::new());
        hound::WavWriter::new(&mut c, spec).unwrap().finalize().unwrap();
        let mut b = c.into_inner();
        assert_eq!((b.len(), &b[0..4], &b[8..12], &b[60..64], le32(&b, 16)), (68, &b"RIFF"[..], &b"WAVE"[..], &b"data"[..], 40), "fixture: product header");
        b[4..8].copy_from_slice(&0u32.to_le_bytes());
        b
    }

    /// V(n): a finalized product WAV of n samples.
    fn v(n: usize) -> Vec<u8> {
        let spec = hound::WavSpec { channels: 1, sample_rate: 48_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut c = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut c, spec).unwrap();
            for i in 0..n {
                w.write_sample(((i % 89) as f32 / 89.0) - 0.5).unwrap();
            }
            w.finalize().unwrap();
        }
        let b = c.into_inner();
        assert_eq!((b.len(), le32(&b, 64) as usize), (68 + 4 * n, 4 * n), "fixture: V({n}) L and D");
        b
    }

    fn run_folder(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir();
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = root.join(format!("{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        assert!(dir.starts_with(&root), "fixture: {} is outside {}", dir.display(), root.display());
        dir
    }

    /// `load_16k` on a file, with any panic caught: Ok(debug text of the return value) or Err(panic text).
    fn load_caught(path: &Path) -> Result<String, String> {
        let p = path.to_path_buf();
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let r = std::panic::catch_unwind(move || {
            let shown = format!("{:?}", load_16k(&p));
            shown.chars().take(120).collect::<String>()
        });
        std::panic::set_hook(prev);
        r.map_err(|e| e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default())
    }

    /// C26 (AC 14): `load_16k` on an empty or partial track returns without a panic; Hf is the control.
    #[test]
    fn m7_14_load_16k_does_not_panic_on_an_empty_or_partial_track() {
        let _one_at_a_time = crate::test_lock();
        let dir = run_folder("m7_14");
        let h = hk();
        let cases: [(&str, Vec<u8>); 4] = [("E", Vec::new()), ("F(1)", h[..1].to_vec()), ("F(67)", h[..67].to_vec()), ("Hf", {
            let mut f = h.clone();
            f[4..8].copy_from_slice(&60u32.to_le_bytes());
            f
        })];
        let mut panicked = Vec::new();
        for (name, bytes) in &cases {
            let p = dir.join(format!("{}.wav", name.replace(['(', ')'], "_")));
            std::fs::write(&p, bytes).unwrap();
            let r = load_caught(&p);
            eprintln!("m7_14 {name} observed: {r:?}");
            if r.is_err() {
                panicked.push(*name);
            }
        }
        assert!(panicked.is_empty(), "behavior: load_16k panicked on {panicked:?}");
    }

    /// C28 (AC 15): an unreadable track (X, or T(1000, 400) whose samples end early) gives an error naming the file, no panic.
    #[test]
    fn m7_15_an_unreadable_track_returns_an_error_naming_the_file() {
        let _one_at_a_time = crate::test_lock();
        let dir = run_folder("m7_15");
        let mut x = vec![b'x'; 40];
        x.extend_from_slice(b"data");
        x.extend_from_slice(&[b'x'; 56]);
        let mut t = v(400);
        t[64..68].copy_from_slice(&4000u32.to_le_bytes());
        t[4..8].copy_from_slice(&4060u32.to_le_bytes());
        assert_eq!((x.len(), t.len(), le32(&t, 64)), (100, 1668, 4000), "fixture: X length, T(1000, 400) L and D");
        for (name, bytes) in [("X", x), ("T(1000, 400)", t)] {
            let sub = dir.join(name.replace(['(', ')', ',', ' '], "_"));
            std::fs::create_dir(&sub).unwrap();
            let p = sub.join("system.wav");
            std::fs::write(&p, &bytes).unwrap();
            let r = load_caught(&p);
            eprintln!("m7_15 {name} observed: {r:?}");
            let shown = r.unwrap_or_else(|e| panic!("behavior: {name} panicked: {e}"));
            assert!(shown.starts_with("Err(") && shown.contains("system.wav"), "behavior: {name} must give an error naming system.wav, got {shown}");
        }
    }

    /// An in-memory file shared with the test, so the bytes that reached the underlying writer can be read.
    #[derive(Clone)]
    struct Shared(std::sync::Arc<std::sync::Mutex<std::io::Cursor<Vec<u8>>>>);

    impl std::io::Write for Shared {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().write(b)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl std::io::Seek for Shared {
        fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
            self.0.lock().unwrap().seek(to)
        }
    }

    /// C37 (AC 20): the 68-byte header reaches the underlying writer before the first chunk is received.
    #[test]
    fn m7_20_the_header_reaches_the_file_before_the_first_chunk() {
        let file = Shared(std::sync::Arc::new(std::sync::Mutex::new(std::io::Cursor::new(Vec::new()))));
        let (tx, rx) = mpsc::channel::<Chunk>();
        let inner = file.clone();
        let writer = std::thread::spawn(move || write_wav(inner, 48_000, rx, || Ok(std::io::Cursor::new(Vec::new()))));
        let started = std::time::Instant::now();
        let mut seen = Vec::new();
        while started.elapsed() < std::time::Duration::from_secs(2) {
            seen = file.0.lock().unwrap().get_ref().clone();
            if seen.len() >= 68 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        drop(tx);
        let _ = writer.join();
        eprintln!("m7_20 observed: {} bytes reached the writer within {:?}", seen.len(), started.elapsed());
        assert!(seen.len() >= 68, "behavior: the underlying writer holds {} bytes within 2 s, not the 68-byte header", seen.len());
        assert_eq!(&seen[60..64], b"data", "behavior: data tag at 60..64");
    }
}
