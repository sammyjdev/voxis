//! whisper.cpp twin of scripts/stt_bench/voice.py: transcribes the owner's
//! recording span by span and writes the same JSON, so voice_score.py scores
//! both engines the same way.
//!
//! usage: stt-bench <config-name> <model.bin> <wav> <spans.json> <out-dir> [lang] [beam]
//! STT_PROMPT, when set, is the initial prompt (bench.py VOCABULARY_PROMPT).

use std::time::Instant;

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

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 6 {
        eprintln!("usage: stt-bench <config> <model.bin> <wav> <spans.json> <out-dir> [lang=pt] [beam=5]");
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
    state.full(params(), &vec![0.0f32; 8000]).expect("warm");
    let load_s = t_load.elapsed().as_secs_f64();

    let mut results = Vec::new();
    for (start, end) in spans {
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
    let out: Value = json!({"config": config, "vram_mib": Value::Null, "load_s": load_s, "results": results});
    std::fs::write(format!("{out_dir}/{config}.json"), serde_json::to_string(&out).unwrap()).unwrap();
}
