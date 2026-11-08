#!/usr/bin/env bash
# Reproduces, against the frozen source, the failures that criterion 64 asks for.
#
#   scripts/windows/frozen-repro.sh <new worktree dir> <evidence dir>
#
# It checks out commit 889d48c in a new worktree, verifies the blobs of the six files it touches,
# appends the test-only modules in .specs/features/windows-port/frozen-repro/ (they add tests
# only, no production line), runs them, and keeps the sources' hashes, the test modules' hashes,
# the exact diff against the frozen tree and the raw output. The tests call the frozen functions
# themselves (serve_http, start, stop, cli, writer, agent, fetch_with and the tray reader).
#
#   control_*          pass on the frozen code: the harness reaches the code
#   characterization_* pass on the frozen code: behaviour that was already right, no red
#   every other test   must FAIL there
#
# Exit 0 only when that is so for every test of both runs (native meetrec, then the Windows tray).
# The tray run needs the tray's generated sidecar in apps/tray/src-tauri/binaries/ of this checkout
# (git-ignored; TRAY_SIDECAR names another file); without it the tray run is reported as NOT RUN
# and the exit is 3.
#
# This is a retrospective, independent reproduction on the frozen base. It does not claim a red was
# seen before the first implementation, and it does not stand in for the tests that live with each
# change. Needs the toolchain of scripts/windows/meetrec-test.cmd; CARGO_TARGET_DIR (native) and
# TRAY_TARGET_DIR (tray) must be set outside the worktree. Remove the worktree afterwards with
#   git worktree remove --force <worktree dir>
set -u
FROZEN=889d48c9a2cca7f61a0648b832c3c3ac20953c78
here=$(cd "$(dirname "$0")/../.." && pwd)
work=${1:?usage: frozen-repro.sh <new worktree dir> <evidence dir>}
ev=${2:?usage: frozen-repro.sh <new worktree dir> <evidence dir>}
inc="$here/.specs/features/windows-port/frozen-repro"
src=spikes/windows-rust/meetrec/src
tray=apps/tray/src-tauri
: "${CARGO_TARGET_DIR:?set CARGO_TARGET_DIR outside the worktree}"
: "${TRAY_TARGET_DIR:?set TRAY_TARGET_DIR outside the worktree}"
sidecar=${TRAY_SIDECAR:-"$here/$tray/binaries/meetrec-x86_64-pc-windows-msvc.exe"}
mkdir -p "$ev" || exit 2

git -C "$here" worktree add --detach "$work" "$FROZEN" > "$ev/worktree.log" 2>&1 || { cat "$ev/worktree.log" >&2; exit 2; }
[[ $(git -C "$work" rev-parse HEAD) == "$FROZEN" ]] || { echo "worktree is not at $FROZEN" >&2; exit 2; }

# file (relative to the repo) -> blob id at the frozen SHA
declare -A BLOB=(
    ["$src/dictate.rs"]=454881e185fb4f9a57f4c571cafc9c89e1e9612b
    ["$src/session.rs"]=428327c5ffba5b511f5bc608631566495e81870d
    ["$src/main.rs"]=772a5f51d7b32361950f6c1cb958deea8e44a93e
    ["$src/models.rs"]=$(git -C "$work" rev-parse "HEAD:$src/models.rs")
    ["$src/minutes.rs"]=$(git -C "$work" rev-parse "HEAD:$src/minutes.rs")
    ["$tray/src/miris.rs"]=$(git -C "$work" rev-parse "HEAD:$tray/src/miris.rs")
)
: > "$ev/frozen-sources.sha256"
for f in "${!BLOB[@]}"; do
    [[ $(git -C "$work" rev-parse "HEAD:$f") == "${BLOB[$f]}" ]] || { echo "blob of $f is not the frozen one" >&2; exit 2; }
    echo "${BLOB[$f]}  $f" >> "$ev/frozen-blobs.txt"
    (cd "$work" && sha256sum "$f") >> "$ev/frozen-sources.sha256"
done
(cd "$inc" && sha256sum ./*.rs.inc) > "$ev/test-modules.sha256"

cat "$inc/dictate.f17.rs.inc" >> "$work/$src/dictate.rs"
cat "$inc/session.owners.rs.inc" "$inc/session.owners2.rs.inc" >> "$work/$src/session.rs"
cat "$inc/main.writer.rs.inc" >> "$work/$src/main.rs"
cat "$inc/models.char.rs.inc" >> "$work/$src/models.rs"
cat "$inc/minutes.agent.rs.inc" >> "$work/$src/minutes.rs"
cat "$inc/tray.f23.rs.inc" >> "$work/$tray/src/miris.rs"
git -C "$work" diff > "$ev/frozen-test-patch.diff"
sha256sum "$ev/frozen-test-patch.diff" > "$ev/frozen-test-patch.sha256"
# the only change to the frozen tree is the appended text: no deleted line
if git -C "$work" diff --numstat | awk '$2 != 0 { bad = 1 } END { exit bad }'; then :; else
    echo "the patch removes lines of the frozen tree" >&2; exit 2
fi

MSYS2_ARG_CONV_EXCL='*' cmd /c "$(cygpath -w "$here/scripts/windows/frozen-repro.cmd")" "$(cygpath -w "$work")" > "$ev/frozen-run.log" 2>&1
echo "$?" > "$ev/frozen-run.exit"

tray_ran=0
if [[ -f $sidecar ]]; then
    mkdir -p "$work/$tray/binaries"
    cp "$sidecar" "$work/$tray/binaries/"
    sha256sum "$work/$tray/binaries/$(basename "$sidecar")" > "$ev/tray-sidecar.sha256"
    CARGO_TARGET_DIR=$TRAY_TARGET_DIR MSYS2_ARG_CONV_EXCL='*' cmd /c "$(cygpath -w "$here/scripts/windows/frozen-repro-tray.cmd")" "$(cygpath -w "$work")" > "$ev/frozen-run-tray.log" 2>&1
    echo "$?" > "$ev/frozen-run-tray.exit"
    tray_ran=1
fi

# A test that runs a child (frozen spawn_child starts this test binary) can get the child's
# "running 0 tests" lines in the middle of its own result line, so the result is read from the
# `... ok` line, or from the `failures:` list cargo prints at the end of the run.
logs=("$ev/frozen-run.log")
[[ $tray_ran == 1 ]] && logs+=("$ev/frozen-run-tray.log")
tr -d '\r' < <(cat "${logs[@]}") > "$ev/frozen-all.txt"
grep -E '^    [A-Za-z0-9_:]+$' "$ev/frozen-all.txt" | sed 's/^ *//' | sort -u > "$ev/frozen-failed-names.txt"
: > "$ev/frozen-results.txt"
while read -r name; do
    if grep -qE "^test ${name} \.\.\. ok$" "$ev/frozen-all.txt"; then r=ok
    elif grep -qxF "$name" "$ev/frozen-failed-names.txt"; then r=FAILED
    else r=UNKNOWN; fi
    echo "test $name ... $r" >> "$ev/frozen-results.txt"
done < <(grep -oE '^test [A-Za-z0-9_:]+ \.\.\.' "$ev/frozen-all.txt" | awk '{print $2}' | sort -u)
bad=0
while read -r _ name _ result; do
    case $name in
        *control_* | *characterization_*) [[ $result == ok ]] || { echo "EXPECTED OK: $name ($result)" >&2; bad=1; } ;;
        *) [[ $result == FAILED ]] || { echo "NOT RED at the frozen SHA: $name ($result)" >&2; bad=1; } ;;
    esac
done < "$ev/frozen-results.txt"
[[ -s "$ev/frozen-results.txt" ]] || { echo "no test ran: see $ev/frozen-run.log" >&2; bad=1; }
cat "$ev/frozen-results.txt"
[[ $bad == 0 ]] || exit 1
[[ $tray_ran == 1 ]] || { echo "tray run NOT RUN: no sidecar at $sidecar" >&2; exit 3; }
exit 0
