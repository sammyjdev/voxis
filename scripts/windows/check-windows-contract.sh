#!/usr/bin/env bash
# Static checks of the Windows port contract (plan windows-port, checks.md
# `Run contract`): documents, preserved decisions, lock and engine inputs.
# Read-only; run from Git Bash at the repo root. Prints PASS/FAIL per check
# and exits 1 when any check fails.
set -uo pipefail
cd "$(dirname "$0")/../.."

BASE=889d48c9a2cca7f61a0648b832c3c3ac20953c78   # PR #4 head the plan froze
H=77a4ba37d7ee72c99f3831c737fce1c34904bc89      # layer 1 harness commit (criteria 84, 87)
failed=0
check() { # name, then a command that succeeds when the check holds
    local name=$1
    shift
    if "$@"; then echo "PASS: $name"; else echo "FAIL: $name"; failed=1; fi
}
# text compared with line breaks and runs of spaces folded: Markdown reflows
flat() { tr '\r\n\t' '   ' < "$1" | tr -s ' '; }
has() { flat "$1" | grep -qF -- "$2" || { echo "  $1 lacks: $2" >&2; return 1; }; }
lacks() { ! flat "$1" | grep -qF -- "$2" || { echo "  $1 still has: $2" >&2; return 1; }; }
py() { python3 - "$@"; }

no_automatic_retention() {
    # outside tests, the native binary removes only its own stop file and .part downloads
    local bad
    bad=$(for f in spikes/windows-rust/meetrec/src/*.rs; do
        [[ $f == *_tests.rs ]] && continue
        sed '/^#\[cfg(test)\]/,$d' "$f" | grep -nE 'remove_file|remove_dir|zip|compress' | sed "s|^|$f:|"
    done | grep -vE 'stop_file|\.part|&part')
    [[ -z $bad ]] || { echo "$bad" >&2; return 1; }
}

verified_windows_configuration() {
    local f
    for f in README.md docs/INSTALL.md; do
        has "$f" "Windows 11 x64" && has "$f" "NVIDIA GeForce RTX 4070 Ti" && has "$f" "CUDA 13.3.1" &&
            has "$f" "Not tested: Windows 10, ARM64, AMD and Intel GPUs, other NVIDIA generations, and other audio devices" || return 1
    done
}

no_windows_fork_statement() {
    # dated history paragraphs (BACKLOG.md's 2026-07-28 split note) are the exemption
    local bad
    bad=$(git ls-files '*.md' | grep -v '^\.specs/' | while read -r f; do
        awk -v f="$f" 'BEGIN { RS = "" } /Allyhgf|voxis-windows|own fork/ && !/20[0-9][0-9]-[0-9][0-9]-[0-9][0-9]/ { print f ": " $0 }' "$f"
    done)
    [[ -z $bad ]] || { echo "$bad" >&2; return 1; }
    has docs/INSTALL.md "[Windows](#windows)"
}

readme_download_statement_names_the_draft_tag() {
    has README.md "releases/tag/v0.2.0-rc3" && has README.md "release candidate, not published" && lacks README.md "releases/latest"
}

installer_sizes_and_labels() {
    has README.md "523,385,745 bytes" && has README.md "9,641,882 bytes" && has README.md "as NVIDIA states them" &&
        has README.md "not measured yet" && lacks README.md "Fastest"
}

windows_dictation_and_notes_facts() {
    has README.md '`f13` by default' && has README.md '`f5` to `f15`' && has README.md "ggml-large-v3-turbo" &&
        has README.md "Silero VAD" && has README.md "not to an Obsidian vault" && has README.md "energy gate, not Silero"
}

windows_privacy_statement() {
    has README.md "the meeting transcript goes to the Anthropic API" && has README.md "the minutes go to Google Drive" &&
        has README.md "Nothing else leaves a Windows machine" &&
        # 0600 is claimed only for macOS
        [[ $(flat README.md | grep -o "0600" | wc -l) == $(flat README.md | grep -oF 'On macOS, session recordings and the auth token are written owner-only (`0600`)' | wc -l) ]]
}

windows_data_locations() {
    local f
    for f in README.md docs/INSTALL.md; do
        has "$f" '%USERPROFILE%\.miris\sessions' && has "$f" '%USERPROFILE%\.voxis\meetings' &&
            has "$f" '%USERPROFILE%\.voxis\logs' && has "$f" "Nothing deletes them automatically" || return 1
    done
}

install_folder_equals_data_folder() {
    has docs/INSTALL.md '%LOCALAPPDATA%\VOXIS' && has docs/INSTALL.md "are the same folder" && has docs/INSTALL.md "uninstall keeps the models"
}

msys_argument_rewrite_note() {
    has spikes/windows-rust/README.md "MSYS2_ARG_CONV_EXCL='*'" && has spikes/windows-rust/README.md "//S"
}

clipboard_and_elevated_window_limits() {
    has README.md "replaced and not restored" && has README.md "running as administrator" && has README.md "leaves the text on the clipboard"
}

audit_scope_names_the_native_lock() {
    # the README names what the workflow scans; while the native lock is not
    # in the workflow, it says the native binary is not covered (AC 60)
    has README.md "apps/tray/src-tauri/Cargo.lock" && has README.md "apps/tray/package-lock.json" || return 1
    if grep -q "working-directory: spikes/windows-rust" .github/workflows/security-audit.yml; then
        has README.md "spikes/windows-rust/Cargo.lock"
    else
        has README.md "The Windows \`meetrec\` binary is not covered: \`spikes/windows-rust/Cargo.lock\` is not scanned yet"
    fi
}

portuguese_only_statement() {
    has README.md "Portuguese (pt-BR) only"
}

pins_cuda_layout_and_menu_wiring_unchanged() {
    git diff --quiet "$BASE" -- .github/workflows/tray-build.yml apps/tray/src-tauri/tauri.conf.json \
        scripts/windows/build-installer.ps1 || { echo "  installer workflow, NSIS config or build script changed" >&2; return 1; }
    py "$BASE" <<'PY'
import re, subprocess, sys
base = sys.argv[1]
def at(rev, path):
    return subprocess.run(["git", "show", f"{rev}:{path}"], capture_output=True, text=True, encoding="utf-8").stdout
def block(text, start, end):
    i = text.index(start)
    return text[i:text.index(end, i)]
ok = True
pins = "spikes/windows-rust/meetrec/src/models.rs"
if block(at(base, pins), "pub const PINS", "];") != block(open(pins, encoding="utf-8").read(), "pub const PINS", "];"):
    print("  model pins changed", file=sys.stderr); ok = False
lib = "apps/tray/src-tauri/src/lib.rs"
old = block(at(base, lib), "fn handle_menu", "\n}\n").splitlines()
new = iter(block(open(lib, encoding="utf-8").read(), "fn handle_menu", "\n}\n").splitlines())
# every owner-accepted wiring line is still there, in order; new arms may be added
if not all(any(line == n for n in new) for line in old):
    print("  a menu wiring line changed", file=sys.stderr); ok = False
if ".on_menu_event(|app, event| handle_menu(app, event.id().as_ref()))" not in open(lib, encoding="utf-8").read():
    print("  menu event hookup changed", file=sys.stderr); ok = False
sys.exit(0 if ok else 1)
PY
}

no_packages_or_macos_linux_build_changes() {
    git diff --quiet "$BASE" -- packages/ .github/workflows/linux.yml .github/workflows/tray-build.yml \
        apps/tray/src-tauri/tauri.conf.json apps/tray/src-tauri/build-mic-helper.sh apps/tray/src-tauri/Cargo.toml \
        apps/tray/src-tauri/Cargo.lock apps/tray/package.json apps/tray/package-lock.json ||
        { git diff --stat "$BASE" -- packages/ .github/workflows/linux.yml apps/tray/src-tauri/Cargo.toml >&2; return 1; }
}

lock_adds_only_toml_edit_sysinfo_and_their_dependencies() {
    py "$BASE" <<'PY'
import re, subprocess, sys
def pk(text):
    out = {}
    for b in text.split("[[package]]")[1:]:
        n = re.search(r'name = "([^"]+)"', b).group(1)
        v = re.search(r'version = "([^"]+)"', b).group(1)
        out.setdefault(n, set()).add(v)
    return out
lock = "spikes/windows-rust/Cargo.lock"
a = pk(subprocess.run(["git", "show", f"{sys.argv[1]}:{lock}"], capture_output=True, text=True).stdout)
b = pk(open(lock, encoding="utf-8").read())
removed = {n: a[n] - b.get(n, set()) for n in a if a[n] - b.get(n, set())}
added = {n: b[n] - a.get(n, set()) for n in b if b[n] - a.get(n, set())}
# sysinfo 0.37 and what it pulls in; toml_edit reuses the 0.25 entry already locked
allowed = {"sysinfo", "ntapi", "objc2-io-kit", "toml_edit", "toml_writer", "toml_parser", "toml_datetime", "winnow"}
ok = not removed and set(added) <= allowed
for n in ("whisper-rs", "whisper-rs-sys"):
    ok = ok and a.get(n) == b.get(n)
if not ok:
    print(f"  removed or changed: {removed}; added: {added}", file=sys.stderr)
sys.exit(0 if ok else 1)
PY
}

engine_inputs_unchanged_since_h() {
    git diff --quiet "$H" -- spikes/windows-rust/stt-bench spikes/windows-rust/Cargo.toml spikes/windows-rust/build-stt-bench.cmd \
        .github/workflows/tray-build.yml || { echo "  stt-bench, workspace or CUDA build inputs changed since H" >&2; return 1; }
    py "$H" <<'PY'
import re, subprocess, sys
h = sys.argv[1]
def at(rev, path):
    return subprocess.run(["git", "show", f"{rev}:{path}"], capture_output=True, text=True, encoding="utf-8").stdout
def entries(text):
    return [b for b in text.split("[[package]]") if re.search(r'name = "(whisper-rs|whisper-rs-sys)"', b)]
def features(text):
    m = re.search(r"\[features\](.*?)\n\[", text, re.S)
    return (m.group(1) if m else "") + "".join(re.findall(r"whisper-rs = .*", text))
def prompt(text):
    return re.search(r'const DEFAULT_PROMPT: &str = "((?:[^"\\]|\\.)*)";', text, re.S).group(1)
def pins(text):
    i = text.index("pub const PINS")
    return text[i:text.index("];", i)]
lock, toml = "spikes/windows-rust/Cargo.lock", "spikes/windows-rust/meetrec/Cargo.toml"
dic, mod = "spikes/windows-rust/meetrec/src/dictate.rs", "spikes/windows-rust/meetrec/src/models.rs"
read = lambda p: open(p, encoding="utf-8").read()
checks = {
    "whisper lock entries": entries(at(h, lock)) == entries(read(lock)),
    "meetrec features": features(at(h, toml)) == features(read(toml)),
    "DEFAULT_PROMPT": prompt(at(h, dic)) == prompt(read(dic)),
    "model pins": pins(at(h, mod)) == pins(read(mod)),
}
bad = [k for k, v in checks.items() if not v]
if bad:
    print(f"  changed since H: {bad}", file=sys.stderr)
sys.exit(1 if bad else 0)
PY
}

harness_commit_changes_exactly_the_listed_files() {
    local want got
    want=$(printf '%s\n' scripts/stt_bench/bench.py scripts/stt_bench/perf_driver.py scripts/stt_bench/voice.py \
        scripts/stt_bench/voice_score.py spikes/windows-rust/stt-bench/src/main.rs)
    got=$(git diff --name-only "$BASE" "$H" | sort)
    [[ $(git rev-parse "$H^") == "$BASE" && $got == "$want" ]] || { echo "  H parent or files differ: $got" >&2; return 1; }
}

check no_automatic_retention no_automatic_retention
check verified_windows_configuration verified_windows_configuration
check no_windows_fork_statement no_windows_fork_statement
check readme_download_statement_names_the_draft_tag readme_download_statement_names_the_draft_tag
check installer_sizes_and_labels installer_sizes_and_labels
check windows_dictation_and_notes_facts windows_dictation_and_notes_facts
check windows_privacy_statement windows_privacy_statement
check windows_data_locations windows_data_locations
check install_folder_equals_data_folder install_folder_equals_data_folder
check msys_argument_rewrite_note msys_argument_rewrite_note
check clipboard_and_elevated_window_limits clipboard_and_elevated_window_limits
check audit_scope_names_the_native_lock audit_scope_names_the_native_lock
check portuguese_only_statement portuguese_only_statement
check pins_cuda_layout_and_menu_wiring_unchanged pins_cuda_layout_and_menu_wiring_unchanged
check no_packages_or_macos_linux_build_changes no_packages_or_macos_linux_build_changes
check lock_adds_only_toml_edit_sysinfo_and_their_dependencies lock_adds_only_toml_edit_sysinfo_and_their_dependencies
check engine_inputs_unchanged_since_h engine_inputs_unchanged_since_h
check harness_commit_changes_exactly_the_listed_files harness_commit_changes_exactly_the_listed_files
exit "$failed"
