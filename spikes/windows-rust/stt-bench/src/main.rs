//! whisper.cpp twin of scripts/stt_bench/voice.py: transcribes the owner's
//! recording span by span and writes the same JSON, so voice_score.py scores
//! both engines the same way.
//!
//! usage: stt-bench <config-name> <model.bin> <wav> <spans.json> <out-dir> [lang] [beam]
//!                  [--sweeps N] [--out <file.json>]
//! STT_PROMPT, when set, is the initial prompt (the driver passes prompt.txt,
//! the text of dictate.rs DEFAULT_PROMPT).
//!
//! Result schema 2 (additive over schema 1): `results` is the first sweep,
//! `sweeps` holds every sweep; `load_s` is the model load, `warmup_s` the
//! warm-up pass; after the warm-up a `READY <unix_ms>` line goes to stdout so
//! the driver can time the cold start. Backend, host and resource fields are
//! null with a reason: scripts/stt_bench/perf_driver.py fills them from the
//! captured stderr and its own sampler. No VAD: a stack measurement.

use std::io::Write;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

fn read_wav(path: &str) -> Vec<f32> {
    let mut r = hound::WavReader::open(path).expect("open wav");
    let spec = r.spec();
    assert_eq!(spec.sample_rate, 16_000, "wav must be 16 kHz");
    let ch = spec.channels as usize;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect(),
        hound::SampleFormat::Float => r.samples::<f32>().map(|s| s.unwrap()).collect(),
    };
    // mono, like voice.py's audio.mean(axis=1)
    samples.chunks(ch).map(|c| c.iter().sum::<f32>() / ch as f32).collect()
}

/// `--sweeps N` and `--out <file>` taken out of the arguments, positionals kept in order.
fn split_flags(args: &[String]) -> (Vec<String>, usize, Option<String>) {
    let (mut pos, mut sweeps, mut out) = (Vec::new(), 1usize, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--sweeps" => sweeps = it.next().and_then(|n| n.parse().ok()).expect("--sweeps N"),
            "--out" => out = it.next().cloned(),
            _ => pos.push(a.clone()),
        }
    }
    (pos, sweeps.max(1), out)
}

fn unix_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

fn main() {
    let (a, sweeps, out_file) = split_flags(&std::env::args().collect::<Vec<_>>());
    if a.len() < 6 {
        eprintln!("usage: stt-bench <config> <model.bin> <wav> <spans.json> <out-dir> [lang=pt] [beam=5] [--sweeps N] [--out file]");
        std::process::exit(2);
    }
    let (config, model, wav, spans_path, out_dir) = (&a[1], &a[2], &a[3], &a[4], &a[5]);
    let lang = a.get(6).map(String::as_str).unwrap_or("pt");
    let beam: i32 = a.get(7).map(|b| b.parse().unwrap()).unwrap_or(5);
    let prompt = std::env::var("STT_PROMPT").ok().filter(|s| !s.is_empty());

    let audio = read_wav(wav);
    let spans: Vec<(usize, usize)> = serde_json::from_str(&std::fs::read_to_string(spans_path).unwrap()).unwrap();

    let t_load = Instant::now();
    let mut cparams = WhisperContextParameters::default();
    cparams.use_gpu(true);
    let ctx = WhisperContext::new_with_params(model, cparams).expect("load model");
    let mut state = ctx.create_state().expect("state");
    let load_s = t_load.elapsed().as_secs_f64();

    let params = || {
        let mut p = FullParams::new(SamplingStrategy::BeamSearch { beam_size: beam, patience: -1.0 });
        p.set_language(if lang == "auto" { None } else { Some(lang) });
        p.set_translate(false);
        p.set_no_context(true); // condition_on_previous_text=False
        p.set_no_timestamps(true);
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_special(false);
        p.set_print_timestamps(false);
        if let Some(pr) = &prompt {
            p.set_initial_prompt(pr);
        }
        p
    };
    // warm, like voice.py's run(np.zeros(8000))
    let t_warm = Instant::now();
    state.full(params(), &vec![0.0f32; 8000]).expect("warm");
    let warmup_s = t_warm.elapsed().as_secs_f64();
    println!("READY {}", unix_ms());
    let _ = std::io::stdout().flush();

    let mut all = Vec::new();
    for _ in 0..sweeps {
        let mut results = Vec::new();
        for &(start, end) in &spans {
            let t = Instant::now();
            state.full(params(), &audio[start..end.min(audio.len())]).expect("transcribe");
            let hyp: String = state
                .as_iter()
                .map(|seg| seg.to_string())
                .collect::<String>()
                .trim()
                .to_string();
            let stt_s = t.elapsed().as_secs_f64();
            println!("{:>6.2}s audio  {:>5.3}s stt  {hyp}", (end - start) as f64 / 16000.0, stt_s);
            results.push(json!({
                "start_s": start as f64 / 16000.0,
                "audio_s": (end - start) as f64 / 16000.0,
                "hyp": hyp, "lang": lang, "stt_s": stt_s,
            }));
        }
        all.push(Value::Array(results));
    }
    let by_driver = "sampled by scripts/stt_bench/perf_driver.py";
    let out: Value = json!({
        "schema": 2, "config": config,
        "backend": Value::Null, "backend_unavailable": "read from stderr by scripts/stt_bench/perf_driver.py",
        "host": Value::Null, "host_unavailable": "recorded by scripts/stt_bench/perf_driver.py",
        "vram_mib": Value::Null, "vram_mib_unavailable": by_driver,
        "cpu_s": Value::Null, "cpu_s_unavailable": by_driver,
        "peak_rss_mib": Value::Null, "peak_rss_mib_unavailable": by_driver,
        "vram_peak_mib": Value::Null, "vram_peak_mib_unavailable": by_driver,
        "load_s": load_s, "warmup_s": warmup_s,
        "results": all[0], "sweeps": all,
    });
    let path = out_file.unwrap_or_else(|| format!("{out_dir}/{config}.json"));
    std::fs::write(path, serde_json::to_string(&out).unwrap()).unwrap();
}
