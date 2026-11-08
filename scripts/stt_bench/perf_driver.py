"""Layer 1 driver of the Windows performance evaluation (plan windows-port, S10).

Runs stt-bench (native CUDA and Vulkan builds) and voice.py (Python
reference) on the scripted corpus in counterbalanced blocks, one process per
variant per block, and keeps everything raw: per-run JSON in result schema 2,
stdout, stderr, exit code, hashes, the build record, the host record and the
resource samples. It measures engine stacks without VAD; it is not product
performance and it states no target.

  perf_driver.py prepare --dictate-rs <path> --out <evidence>
      prompt.txt (the shipped DEFAULT_PROMPT) with its SHA-256, the generated
      silence and noise control clips with their hashes, host.json
  perf_driver.py run --protocol <perf-protocol.md> --variants <variants.json>
                     --wav <voice.wav> --spans <voice.spans.json> --out <evidence>
                     --table prompt|noprompt [--blocks 6] [--sweeps 10]
  perf_driver.py controls --variants <variants.json> --out <evidence>
  perf_driver.py report --out <evidence>

Sampled process (criteria 73 and 75): the counters are those of the process the driver spawns. For a
Python variant, "python" must be the interpreter that runs the model: a venv's python.exe is a launcher
stub whose child does the work, and its counters would be a false near-zero. Name the base interpreter and
put the venv's site-packages on PYTHONPATH. Every run record says which pid and image were sampled and
which children it had (`sampled_process`), so a stub shows up as a sampled image with a child.

variants.json: a list of {"id": "V1", "kind": "native", "exe": ..., "model": ...,
"build": {...}} or {"id": "V3", "kind": "python", "python": ..., "script":
"voice.py"}. "build" carries what only the builder knows (CUDA toolkit,
CUDAARCHS, cargo profile and features); the driver adds rustc, the whisper-rs
lock entries and the binary hash. A value nobody can read is null with a
`<field>_unavailable` reason, never 0.
"""
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
import os
import platform
import random
import re
import shutil
import statistics
import struct
import subprocess
import sys
import threading
import time
import wave

HERE = os.path.dirname(os.path.abspath(__file__))
AUDIO_SHA256 = "879813cb7d985c288be6f2df6d7b0502495054aeca4e2c42a9fe9eda2ce4dd7c"
SPANS_SHA256 = "26e7cd855321dfe36520ea60a9ee9049b50bb42a9389027899dfb996a67fa525"
SAMPLE_INTERVAL_S = 0.2
RATE = 16000


class CorpusMismatch(Exception):
    pass


def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def check_corpus(wav: str, spans: str, audio_sha256: str = AUDIO_SHA256, spans_sha256: str = SPANS_SHA256) -> dict:
    """Stops before any run when the corpus is not the pinned one (criterion 70)."""
    got = {"audio_sha256": sha256_file(wav), "spans_sha256": sha256_file(spans)}
    for (key, want), path in zip((("audio_sha256", audio_sha256), ("spans_sha256", spans_sha256)), (wav, spans)):
        if got[key] != want:
            raise CorpusMismatch(f"corpus mismatch: {os.path.basename(path)} sha256 {got[key]}, expected {want}")
    return got


def block_orders(variants: list[str]) -> list[list[str]]:
    """Every ordering once: with 3 variants, 6 blocks and each variant twice in each position."""
    return [list(p) for p in itertools.permutations(variants)]


def parse_backend(stderr: str) -> tuple[str | None, str | None]:
    """The backend whisper.cpp chose, from its own log line."""
    m = re.search(r"using (\S+) backend", stderr)
    if m:
        return m.group(1), None
    if re.search(r"no GPU found|failed to initialize .*backend", stderr, re.I):
        return "CPU", None
    return None, "no whisper.cpp backend line ('using <name> backend') in stderr"


def extract_default_prompt(rust_source: str) -> str:
    """The DEFAULT_PROMPT string literal of dictate.rs, with Rust's `\\`
    line continuation (newline and the next line's indentation skipped)."""
    m = re.search(r'const DEFAULT_PROMPT: &str = "((?:[^"\\]|\\.)*)";', rust_source, re.S)
    if not m:
        raise ValueError("DEFAULT_PROMPT not found")
    text = re.sub(r"\\\r?\n\s*", "", m.group(1))
    return text.replace('\\"', '"').replace("\\\\", "\\")


def parse_compute_apps(csv: str, pid: int) -> tuple[int | None, str | None]:
    """GPU memory of one process from `nvidia-smi --query-compute-apps=pid,used_memory`."""
    for line in csv.splitlines():
        parts = [x.strip() for x in line.split(",")]
        if len(parts) == 2 and parts[0] == str(pid):
            if parts[1].isdigit():
                return int(parts[1]), None
            return None, "nvidia-smi reports [N/A] per process (WDDM)"
    return None, f"pid {pid} not listed by nvidia-smi --query-compute-apps"


def summarize_samples(samples: list[dict], peak_wset_mib: float | None) -> dict:
    """The last CPU time, the peak working set and the peak GPU memory; a
    counter with no sample stays null with the reason the sampler saw."""
    out: dict = {}
    cpu = [s["cpu_s"] for s in samples if s.get("cpu_s") is not None]
    out["cpu_s"] = max(cpu) if cpu else None
    if out["cpu_s"] is None:
        out["cpu_s_unavailable"] = "no CPU time sample"
    rss = [s["rss_mib"] for s in samples if s.get("rss_mib") is not None]
    peaks = [x for x in rss + ([peak_wset_mib] if peak_wset_mib is not None else [])]
    out["peak_rss_mib"] = max(peaks) if peaks else None
    if out["peak_rss_mib"] is None:
        out["peak_rss_mib_unavailable"] = "no working-set sample"
    vram = [s["vram_mib"] for s in samples if s.get("vram_mib") is not None]
    out["vram_peak_mib"] = max(vram) if vram else None
    if out["vram_peak_mib"] is None:
        reasons = [s.get("vram_reason") for s in samples if s.get("vram_reason")]
        out["vram_peak_mib_unavailable"] = reasons[-1] if reasons else "no GPU memory sample"
    return out


def quantile(xs: list[float], p: float) -> float:
    """Linear interpolation between closest ranks (numpy's default)."""
    xs = sorted(xs)
    if len(xs) == 1:
        return xs[0]
    k = (len(xs) - 1) * p
    lo = int(k)
    hi = min(lo + 1, len(xs) - 1)
    return xs[lo] + (xs[hi] - xs[lo]) * (k - lo)


def process_stats(run: dict, spawn_ms: int | None, ready_ms: int | None) -> dict:
    """Criterion 74 for one process: warm per-span p50/p95 over all sweeps,
    RTF per sweep (processing time / audio time), cold start to READY."""
    sweeps = run["sweeps"]
    spans = [x["stt_s"] for sweep in sweeps for x in sweep]
    rtf = [sum(x["stt_s"] for x in sweep) / max(sum(x["audio_s"] for x in sweep), 1e-9) for sweep in sweeps]
    return {
        "warm_p50": quantile(spans, 0.5),
        "warm_p95": quantile(spans, 0.95),
        "rtf_per_sweep": rtf,
        "rtf": statistics.median(rtf),
        "cold_s": (ready_ms - spawn_ms) / 1000 if ready_ms is not None and spawn_ms is not None else None,
        "sweeps": len(sweeps),
    }


def across_runs(values: list[float | None]) -> dict:
    v = [x for x in values if x is not None]
    if not v:
        return {"n": 0, "min": None, "median": None, "max": None}
    return {"n": len(v), "min": min(v), "median": statistics.median(v), "max": max(v)}


PROTOCOL_BLOCKS = 6


def claim(first: list[float | None], second: list[float | None], blocks: list[int] | None = None) -> str:
    """Criterion 77: a difference only when the per-block sign is the same in
    all 6 distinct protocol blocks; a tie, a missing block or any other count
    of blocks is no consistent difference."""
    if len(first) != PROTOCOL_BLOCKS or len(second) != PROTOCOL_BLOCKS:
        return "no consistent difference"
    if blocks is not None and len(set(blocks)) != PROTOCOL_BLOCKS:
        return "no consistent difference"
    if any(x is None for x in first + second):
        return "no consistent difference"
    diffs = [a - b for a, b in zip(first, second)]
    if all(d < 0 for d in diffs):
        return f"difference: first lower in all {len(diffs)} blocks"
    if all(d > 0 for d in diffs):
        return f"difference: first higher in all {len(diffs)} blocks"
    return "no consistent difference"


def _write_wav(path: str, samples: list[int]) -> None:
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(struct.pack(f"<{len(samples)}h", *samples))


def write_control_clips(out_dir: str) -> dict:
    """Criterion 79: 10 s of digital silence and 10 s of seeded white noise
    (amplitude 0.01 of full scale), 16 kHz mono 16-bit."""
    os.makedirs(out_dir, exist_ok=True)
    rng = random.Random(79)
    clips = {"silence": [0] * (10 * RATE),
             "noise": [int(rng.gauss(0, 0.01) * 32767) for _ in range(10 * RATE)]}
    out = {}
    for name, samples in clips.items():
        path = os.path.join(out_dir, f"{name}.wav")
        _write_wav(path, samples)
        out[name] = {"path": path, "sha256": sha256_file(path)}
    return out


def non_empty_controls(runs: dict) -> list[tuple[str, str, str]]:
    return [(v, clip, text) for v, by_clip in runs.items() for clip, text in by_clip.items() if text.strip()]


# ------------------------------------------------------------------ records

def _run_text(cmd: list[str]) -> tuple[str | None, str | None]:
    try:
        out = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        if out.returncode != 0:
            return None, f"{cmd[0]} exited {out.returncode}: {out.stderr.strip()[:200]}"
        return out.stdout.strip(), None
    except (OSError, subprocess.TimeoutExpired) as e:
        return None, f"{cmd[0]}: {e}"


def _field(record: dict, key: str, value, reason: str | None) -> None:
    record[key] = value
    if value is None:
        record[f"{key}_unavailable"] = reason or "not readable"


def host_record() -> dict:
    """CPU, RAM, GPU, driver, Windows build and power plan of this host."""
    h: dict = {}
    cpu, reason = None, "not Windows"
    if sys.platform == "win32":
        try:
            import winreg
            with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r"HARDWARE\DESCRIPTION\System\CentralProcessor\0") as k:
                cpu, reason = winreg.QueryValueEx(k, "ProcessorNameString")[0].strip(), None
        except OSError as e:
            reason = f"registry: {e}"
    _field(h, "cpu", cpu, reason)
    try:
        import psutil
        _field(h, "ram_mib", psutil.virtual_memory().total // (1 << 20), None)
    except ImportError as e:
        _field(h, "ram_mib", None, f"psutil: {e}")
    gpu, reason = _run_text(["nvidia-smi", "--query-gpu=name,driver_version", "--format=csv,noheader"])
    name, driver = (gpu.split(",", 1) + [None])[:2] if gpu else (None, None)
    _field(h, "gpu", name.strip() if name else None, reason)
    _field(h, "driver", driver.strip() if driver else None, reason)
    _field(h, "os", f"{platform.system()} {platform.release()} build {platform.version()}", None)
    plan, reason = _run_text(["powercfg", "/getactivescheme"]) if sys.platform == "win32" else (None, "not Windows")
    _field(h, "power_plan", plan, reason)
    return h


def lock_entries(lock_path: str, names=("whisper-rs", "whisper-rs-sys", "ggml")) -> dict:
    """The lock entries of the engine crates (criterion 84 compares them)."""
    out: dict = {}
    text = open(lock_path, encoding="utf-8").read() if os.path.isfile(lock_path) else ""
    for block in text.split("[[package]]"):
        m = re.search(r'name = "([^"]+)"', block)
        if m and m.group(1) in names:
            out[m.group(1)] = {k: v for k, v in re.findall(r'(version|source|checksum) = "([^"]+)"', block)}
    for name in names:
        # ggml is vendored inside whisper-rs-sys, so it has no entry of its own
        out.setdefault(name, {"absent": "no [[package]] entry with this name in the lock"})
    return out


def build_record(variant: dict, repo: str | None) -> dict:
    """The builder's record plus what the driver can read itself."""
    b = dict(variant.get("build", {}))
    if variant.get("kind") == "native":
        rustc, reason = _run_text(["rustc", "--version"])
        _field(b, "rustc", b.get("rustc", rustc), reason)
        lock = os.path.join(repo, "spikes", "windows-rust", "Cargo.lock") if repo else ""
        b["lock"] = lock_entries(lock)
        # the lock pins whisper-rs-sys by version and checksum; the whisper.cpp
        # commit it vendors is not in the lock
        b.setdefault("whisper_cpp_commit", None)
        if b["whisper_cpp_commit"] is None:
            b["whisper_cpp_commit_unavailable"] = "Cargo.lock records whisper-rs-sys version and checksum only"
        for k in ("cuda_toolkit", "cudaarchs", "cargo_profile", "features"):
            if b.get(k) is None:
                b[k] = None
                b[f"{k}_unavailable"] = "not supplied in variants.json"
    return b


def variant_command(variant: dict, wav: str, spans: str, out_json: str, sweeps: int, table: str) -> tuple[list[str], dict]:
    """The process and environment of one variant run."""
    out_dir = os.path.dirname(out_json)
    if "cmd" in variant:  # a prepared command line (tests, ad hoc variants)
        return list(variant["cmd"]) + ["--sweeps", str(sweeps), "--out", out_json], {}
    if variant["kind"] == "native":
        config = f"{variant['id']}-{table}"
        return ([variant["exe"], config, variant["model"], wav, spans, out_dir, "pt", "5",
                 "--sweeps", str(sweeps), "--out", out_json], {})
    config = "turbo-pt-b5-promptfile" if table == "prompt" else "turbo-pt-b5"
    script = os.path.join(HERE, variant.get("script", "voice.py"))
    return [variant["python"], script, config, wav, spans, out_dir, "--sweeps", str(sweeps), "--out", out_json], {}


def run_variant(variant: dict, run_dir: str, sweeps: int, extra_env: dict, host: dict, source_sha: str | None,
                sample_interval_s: float = SAMPLE_INTERVAL_S, wav: str = "", spans: str = "", table: str = "prompt",
                repo: str | None = None, source_sha_reason: str | None = None) -> dict:
    """One process: spawn, sample until exit, keep stdout/stderr/exit code,
    then complete the engine's schema 2 JSON with the driver's records."""
    os.makedirs(run_dir, exist_ok=True)
    out_json = os.path.join(run_dir, "result.json")
    cmd, env_add = variant_command(variant, wav, spans, out_json, sweeps, table)
    env = dict(os.environ, **extra_env, **env_add)
    samples: list[dict] = []
    ready = {"ms": None}
    stdout_lines: list[str] = []
    spawn_ms = int(time.time() * 1000)
    stderr_path = os.path.join(run_dir, "stderr.log")
    try:
        with open(stderr_path, "wb") as stderr_file:
            proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=stderr_file,
                                    env=env, text=True, encoding="utf-8", errors="replace")
    except OSError as e:  # missing executable, access denied: the variant is unavailable, the run goes on
        rec = {"variant": variant["id"], "cmd": cmd, "exit_code": None, "spawn_ms": spawn_ms, "ready_ms": None,
               "samples": [], "unavailable": f"unavailable: {e} ({cmd[0]})"}
        with open(stderr_path, "w", encoding="utf-8") as f:
            f.write(f"launch failed: {e}\n")
        with open(os.path.join(run_dir, "stdout.log"), "w", encoding="utf-8"):
            pass
        with open(os.path.join(run_dir, "run.json"), "w") as f:
            json.dump(rec, f, indent=1)
        return rec

    def read_stdout() -> None:
        for line in proc.stdout:
            stdout_lines.append(line)
            m = re.match(r"READY (\d+)", line)
            if m and ready["ms"] is None:
                ready["ms"] = int(m.group(1))

    reader = threading.Thread(target=read_stdout, daemon=True)
    reader.start()
    peak_wset = None
    try:
        import psutil
        ps = psutil.Process(proc.pid)
        sampled_exe = ps.exe()
    except Exception:  # psutil missing or process already gone
        ps = None
        sampled_exe = None
    children_seen: dict[int, str | None] = {}
    while proc.poll() is None:
        s: dict = {"t": time.time()}
        if ps is not None:
            try:
                for c in ps.children(recursive=True):
                    if c.pid not in children_seen:
                        try:
                            children_seen[c.pid] = c.exe()
                        except Exception:
                            children_seen[c.pid] = None
            except Exception:
                pass
            try:
                t = ps.cpu_times()
                s["cpu_s"] = t.user + t.system
                mem = ps.memory_info()
                s["rss_mib"] = mem.rss / (1 << 20)
                if getattr(mem, "peak_wset", None):
                    peak_wset = max(peak_wset or 0, mem.peak_wset / (1 << 20))
            except Exception:
                pass
        csv, reason = _run_text(["nvidia-smi", "--query-compute-apps=pid,used_memory",
                                 "--format=csv,noheader,nounits"]) if shutil.which("nvidia-smi") else (None, "nvidia-smi not on PATH")
        s["vram_mib"], s["vram_reason"] = parse_compute_apps(csv, proc.pid) if csv is not None else (None, reason)
        samples.append(s)
        time.sleep(sample_interval_s)
    reader.join(timeout=5)
    code = proc.returncode
    with open(os.path.join(run_dir, "stdout.log"), "w", encoding="utf-8") as f:
        f.writelines(stdout_lines)
    stderr = open(os.path.join(run_dir, "stderr.log"), encoding="utf-8", errors="replace").read()
    rec = {"variant": variant["id"], "cmd": cmd, "exit_code": code, "spawn_ms": spawn_ms, "ready_ms": ready["ms"],
           "spawn_pid": proc.pid, "samples": samples}
    with open(os.path.join(run_dir, "samples.json"), "w") as f:
        json.dump(samples, f)
    if code != 0 or not os.path.isfile(out_json):
        last = [line for line in stderr.splitlines() if line.strip()]
        rec["unavailable"] = f"unavailable: {last[-1] if last else f'exit {code}'}"
        with open(os.path.join(run_dir, "run.json"), "w") as f:
            json.dump(rec, f, indent=1)
        return rec
    raw = json.load(open(out_json, encoding="utf-8"))
    backend, reason = parse_backend(stderr)
    if variant.get("kind") == "python":
        backend, reason = raw.get("backend"), raw.get("backend_unavailable")
    _field(raw, "backend", backend, reason)
    raw["host"] = host
    _field(raw, "source_sha", source_sha, source_sha_reason or "git did not report a commit")
    binary = variant.get("binary") or variant.get("exe") or variant.get("python")
    _field(raw, "binary_sha256", sha256_file(binary) if binary and os.path.isfile(binary) else None,
           f"{binary} not found")
    prompt_file = env.get("STT_PROMPT_FILE")
    _field(raw, "prompt_sha256", sha256_file(prompt_file) if prompt_file and os.path.isfile(prompt_file) else None,
           "no prompt file for this table" if not prompt_file else f"{prompt_file} not found")
    if variant.get("kind") == "native":
        model = variant.get("model")
        raw["model"] = {"path": model, "sha256": sha256_file(model) if model and os.path.isfile(model) else None}
        if raw["model"]["sha256"] is None:
            raw["model"]["sha256_unavailable"] = f"{model} not found"
    elif variant.get("kind") == "python":
        py = variant.get("python")
        version, reason = _run_text([py, "-c", "import sys; print(sys.version)"]) if py else (None, "no python")
        raw["python"] = {"executable": py, "version": version}
        if version is None:
            raw["python"]["version_unavailable"] = reason
        if not raw.get("engine_package"):
            raw["engine_package"] = None
            raw["engine_package_unavailable"] = "the engine script did not report its package versions"
        if not raw.get("model"):
            raw["model"] = None
            raw["model_unavailable"] = "the engine script did not report the model identity"
    raw["build"] = build_record(variant, repo)
    raw["exit_code"] = code
    raw["sampled_process"] = {
        "pid": proc.pid, "exe": sampled_exe,
        "children_seen": [{"pid": pid, "exe": exe} for pid, exe in sorted(children_seen.items())],
        "note": "CPU, working set and GPU memory are those of this pid only. A launcher (a venv's python.exe starts "
                "the base interpreter as a child) shows near-zero CPU and a few MiB: a Python variant names the "
                "interpreter that runs the model, with its packages on PYTHONPATH",
    }
    if sampled_exe is None:
        raw["sampled_process"]["exe_unavailable"] = "psutil could not read the process image"
    raw["cold_s"] = process_stats(raw, spawn_ms, ready["ms"])["cold_s"]
    for k in [k for k in list(raw) if k.endswith("_unavailable") and k[:-12] in ("cpu_s", "peak_rss_mib", "vram_peak_mib")]:
        del raw[k]
    raw.update(summarize_samples(samples, peak_wset))
    raw["sample_interval_s"] = sample_interval_s
    with open(out_json, "w", encoding="utf-8") as f:
        json.dump(raw, f, ensure_ascii=False, indent=1)
    rec["stats"] = process_stats(raw, spawn_ms, ready["ms"])
    with open(os.path.join(run_dir, "run.json"), "w") as f:
        json.dump(rec, f, indent=1)
    return rec


# ------------------------------------------------------------------ commands

def git_head_with_reason(repo: str) -> tuple[str | None, str | None]:
    return _run_text(["git", "-C", repo, "rev-parse", "HEAD"])


def git_head(repo: str) -> str | None:
    return git_head_with_reason(repo)[0]


def cmd_prepare(a) -> None:
    os.makedirs(a.out, exist_ok=True)
    prompt = extract_default_prompt(open(a.dictate_rs, encoding="utf-8").read())
    path = os.path.join(a.out, "prompt.txt")
    with open(path, "w", encoding="utf-8", newline="") as f:
        f.write(prompt)
    clips = write_control_clips(os.path.join(a.out, "controls"))
    meta = {"prompt": {"path": path, "sha256": sha256_file(path)}, "controls": clips, "host": host_record()}
    with open(os.path.join(a.out, "prepare.json"), "w") as f:
        json.dump(meta, f, indent=1)
    print(json.dumps(meta, indent=1))


def _variants(path: str) -> list[dict]:
    return json.load(open(path, encoding="utf-8"))


def _prompt_env(out: str, table: str) -> dict:
    if table != "prompt":
        return {"STT_PROMPT": ""}
    path = os.path.join(out, "prompt.txt")
    return {"STT_PROMPT": open(path, encoding="utf-8").read(), "STT_PROMPT_FILE": path}


def cmd_run(a) -> None:
    try:
        corpus = check_corpus(a.wav, a.spans)
    except CorpusMismatch as e:
        print(e, file=sys.stderr)
        sys.exit(1)
    if not os.path.isfile(a.protocol):
        print(f"no protocol at {a.protocol}: write perf-protocol.md first", file=sys.stderr)
        sys.exit(1)
    variants = _variants(a.variants)
    by_id = {v["id"]: v for v in variants}
    orders = block_orders([v["id"] for v in variants])[: a.blocks]
    host = host_record()
    source_sha, source_reason = git_head_with_reason(a.repo)
    manifest = {"protocol_sha256": sha256_file(a.protocol), "corpus": corpus, "table": a.table, "orders": orders,
                "sweeps": a.sweeps, "source_sha": source_sha, "host": host, "started": time.time(), "runs": [],
                "blocks_run": len(orders), "protocol_blocks": PROTOCOL_BLOCKS,
                "claims_allowed": len(orders) == PROTOCOL_BLOCKS}
    if source_sha is None:
        manifest["source_sha_unavailable"] = source_reason or "git did not report a commit"
    if not manifest["claims_allowed"]:
        print(f"smoke run: {len(orders)} of {PROTOCOL_BLOCKS} blocks, no protocol claim", file=sys.stderr)
    env = _prompt_env(a.out, a.table)
    for b, order in enumerate(orders, 1):
        for v in order:
            run_dir = os.path.join(a.out, a.table, f"block{b}", v)
            rec = run_variant(by_id[v], run_dir, a.sweeps, env, host, manifest["source_sha"], wav=a.wav,
                              spans=a.spans, table=a.table, repo=a.repo, source_sha_reason=source_reason)
            manifest["runs"].append({"block": b, "variant": v, "dir": run_dir, "exit_code": rec["exit_code"],
                                     "unavailable": rec.get("unavailable")})
            print(f"block {b} {v}: exit {rec['exit_code']} {rec.get('unavailable', '')}", flush=True)
    manifest["finished"] = time.time()
    with open(os.path.join(a.out, f"manifest-{a.table}.json"), "w") as f:
        json.dump(manifest, f, indent=1)


def cmd_controls(a) -> None:
    clips = json.load(open(os.path.join(a.out, "prepare.json")))["controls"]
    results: dict = {}
    for v in _variants(a.variants):
        results[v["id"]] = {}
        for name, clip in clips.items():
            spans = os.path.join(a.out, "controls", f"{name}.spans.json")
            with open(spans, "w") as f:
                json.dump([[0, 10 * RATE]], f)
            run_dir = os.path.join(a.out, "controls", name, v["id"])
            sha, sha_reason = git_head_with_reason(a.repo)
            rec = run_variant(v, run_dir, 1, _prompt_env(a.out, "prompt"), {}, sha, wav=clip["path"],
                              spans=spans, table="prompt", repo=a.repo, source_sha_reason=sha_reason)
            text = ""
            if rec["exit_code"] == 0:
                text = " ".join(x["hyp"] for x in json.load(open(os.path.join(run_dir, "result.json")))["sweeps"][0])
            results[v["id"]][name] = text if rec["exit_code"] == 0 else rec.get("unavailable", "")
    report = {"clips": clips, "outputs": results, "non_empty": non_empty_controls(results)}
    with open(os.path.join(a.out, "controls.json"), "w", encoding="utf-8") as f:
        json.dump(report, f, ensure_ascii=False, indent=1)
    print(json.dumps(report["non_empty"], ensure_ascii=False))


def cmd_report(a) -> None:
    import voice_score
    phrases = voice_score.load_phrases()
    lines = ["# Layer 1 engine microbenchmark (no VAD; not product performance)", ""]
    for table in ("prompt", "noprompt"):
        mpath = os.path.join(a.out, f"manifest-{table}.json")
        if not os.path.isfile(mpath):
            continue
        m = json.load(open(mpath))
        blocks_run = len(m["orders"])
        if blocks_run != PROTOCOL_BLOCKS:
            lines += [f"## Table: {table}", "",
                      f"**smoke run: {blocks_run} of {PROTOCOL_BLOCKS} protocol blocks. No protocol claim is made from it.**",
                      ""]
        else:
            lines += [f"## Table: {table}", ""]
        lines += [f"protocol sha256 `{m['protocol_sha256']}`, source `{m['source_sha']}`, "
                  f"{len(m['orders'])} blocks, {m['sweeps']} sweeps per process", ""]
        per: dict = {}
        for r in m["runs"]:
            run = json.load(open(os.path.join(r["dir"], "run.json")))
            per.setdefault(r["variant"], {})[r["block"]] = run
        lines += ["| variant | runs ok | warm p50 s (min/median/max) | warm p95 s | RTF | cold s | backend |",
                  "|---|---|---|---|---|---|---|"]
        for v, blocks in sorted(per.items()):
            st = [b.get("stats") for b in blocks.values()]
            ok = [s for s in st if s]
            fmt = lambda k: "{min:.3f} / {median:.3f} / {max:.3f}".format(**across_runs([s[k] for s in ok])) if ok else "-"
            backend = "-"
            first_ok = next((b for b in blocks.values() if b.get("stats")), None)
            if first_ok:
                raw = json.load(open(os.path.join(os.path.dirname(first_ok["cmd"][-1]), "result.json")))
                backend = raw.get("backend") or f"null ({raw.get('backend_unavailable')})"
            unavailable = {b.get("unavailable") for b in blocks.values() if b.get("unavailable")}
            lines.append(f"| {v} | {len(ok)}/{len(blocks)} | {fmt('warm_p50')} | {fmt('warm_p95')} | {fmt('rtf')} | "
                         f"{fmt('cold_s') if all(s['cold_s'] is not None for s in ok) else '-'} | {backend} |")
            for u in unavailable:
                lines.append(f"| {v} | {u} | | | | | |")
        lines += ["", "Claim rule (protocol, criterion 77): per-block sign of the process statistic, all blocks.", ""]
        ids = sorted(per)
        for x, y in itertools.combinations(ids, 2):
            blocks = sorted(set(per[x]) & set(per[y]))
            for k in ("warm_p50", "rtf", "cold_s"):
                a_ = [(per[x][b].get("stats") or {}).get(k) for b in blocks]
                b_ = [(per[y][b].get("stats") or {}).get(k) for b in blocks]
                lines.append(f"- {x} vs {y}, {k}: {claim(a_, b_, blocks)}")
        lines += ["", "| variant | WER | CER | terms right | empty | repeated 3-grams | omitted words |",
                  "|---|---|---|---|---|---|---|"]
        for v, blocks in sorted(per.items()):
            first_ok = next((b for b in blocks.values() if b.get("stats")), None)
            if not first_ok:
                continue
            s = voice_score.score_file(os.path.join(os.path.dirname(first_ok["cmd"][-1]), "result.json"), phrases)
            lines.append(f"| {v} | {s['wer']:.1f} | {s['cer']:.1f} | {s['term_recall']:.0f}% | {s['empty_outputs']} | "
                         f"{s['repeated_ngrams']} | {len(s['omitted_words'])} |")
            if s["note"]:
                lines.append(f"\n{v}: {s['note']}\n")
        lines.append("")
    cpath = os.path.join(a.out, "controls.json")
    if os.path.isfile(cpath):
        c = json.load(open(cpath, encoding="utf-8"))
        lines += ["## Silence and noise controls", ""]
        lines += [f"- {v} on {clip}: `{text}`" for v, clip, text in c["non_empty"]] or ["- every output empty"]
    out = os.path.join(a.out, "report-layer1.md")
    with open(out, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    print(out)


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("prepare")
    s.add_argument("--dictate-rs", required=True)
    s.add_argument("--out", required=True)
    s = sub.add_parser("run")
    for k in ("--protocol", "--variants", "--wav", "--spans", "--out"):
        s.add_argument(k, required=True)
    s.add_argument("--table", choices=("prompt", "noprompt"), required=True)
    s.add_argument("--blocks", type=int, default=6)
    s.add_argument("--sweeps", type=int, default=10)
    s.add_argument("--repo", default=os.path.normpath(os.path.join(HERE, "..", "..")))
    s = sub.add_parser("controls")
    s.add_argument("--variants", required=True)
    s.add_argument("--out", required=True)
    s.add_argument("--repo", default=os.path.normpath(os.path.join(HERE, "..", "..")))
    s = sub.add_parser("report")
    s.add_argument("--out", required=True)
    a = p.parse_args()
    {"prepare": cmd_prepare, "run": cmd_run, "controls": cmd_controls, "report": cmd_report}[a.cmd](a)


if __name__ == "__main__":
    main()
