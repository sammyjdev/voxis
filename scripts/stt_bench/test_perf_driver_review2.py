"""Certificates round: which process the driver sampled (AC 73, AC 75; Vera's finding V1).

The driver samples the PID it spawns. A venv's python.exe is a launcher stub that starts the base
interpreter as a child, so a variant started through it reports the stub's near-zero CPU and a few MiB.
The rule: a Python variant names the interpreter that runs the model. The record says which process
was sampled and which children it had, so a stub cannot pass for the worker.
"""
from __future__ import annotations

import json
import os
import sys
import textwrap

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import perf_driver as pd  # noqa: E402

# the interpreter that does the work: this venv's python.exe is itself a launcher stub with a child
BASE = getattr(sys, "_base_executable", sys.executable)

LAUNCHER = textwrap.dedent('''\
    import json, subprocess, sys, time
    args = sys.argv[1:]
    out = args[args.index("--out") + 1]
    child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(2.0)"])
    print(f"READY {int(time.time() * 1000)}", flush=True)
    time.sleep(0.8)
    sweep = [{"start_s": 0.0, "audio_s": 1.0, "hyp": "x", "lang": "pt", "stt_s": 0.1}]
    json.dump({"schema": 2, "config": "fake", "backend": None, "results": sweep, "sweeps": [sweep]}, open(out, "w"))
    child.kill()
    ''')


def test_the_record_names_the_sampled_process_and_the_children_it_had(tmp_path):
    script = tmp_path / "launcher.py"
    script.write_text(LAUNCHER)
    variant = {"id": "V3", "kind": "python", "python": BASE, "cmd": [BASE, str(script)],
               "binary": BASE, "build": {}}

    rec = pd.run_variant(variant, str(tmp_path / "run"), sweeps=1, extra_env={}, host={}, source_sha="abc",
                         sample_interval_s=0.05)

    raw = json.load(open(tmp_path / "run" / "result.json"))
    sampled = raw["sampled_process"]
    assert sampled["pid"] == rec["spawn_pid"]
    assert os.path.normcase(os.path.realpath(sampled["exe"])) == os.path.normcase(os.path.realpath(BASE))
    assert len(sampled["children_seen"]) >= 1, sampled
    assert all(c["pid"] != sampled["pid"] for c in sampled["children_seen"])
    assert "launcher" in sampled["note"] and "interpreter that runs the model" in sampled["note"]


def test_a_variant_without_children_records_none(tmp_path):
    script = tmp_path / "plain.py"
    script.write_text(textwrap.dedent('''\
        import json, sys, time
        args = sys.argv[1:]
        out = args[args.index("--out") + 1]
        print(f"READY {int(time.time() * 1000)}", flush=True)
        time.sleep(0.4)
        sweep = [{"start_s": 0.0, "audio_s": 1.0, "hyp": "x", "lang": "pt", "stt_s": 0.1}]
        json.dump({"schema": 2, "config": "fake", "backend": None, "results": sweep, "sweeps": [sweep]}, open(out, "w"))
        '''))
    variant = {"id": "V3", "kind": "python", "python": BASE, "cmd": [BASE, str(script)],
               "binary": BASE, "build": {}}

    pd.run_variant(variant, str(tmp_path / "run"), sweeps=1, extra_env={}, host={}, source_sha="abc",
                   sample_interval_s=0.05)

    raw = json.load(open(tmp_path / "run" / "result.json"))
    assert raw["sampled_process"]["children_seen"] == []
