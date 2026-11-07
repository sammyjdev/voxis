#!/usr/bin/env bash
# Reproduces, against the frozen source, the failures that criterion 64 asks for.
#
#   scripts/windows/frozen-repro.sh <new worktree dir> <evidence dir>
#
# It checks out commit 889d48c in a new worktree, verifies the blobs of the three files it
# touches, appends the test modules in .specs/features/windows-port/frozen-repro/ (they add
# tests only, no production line), runs them, and keeps the sources' hashes, the test
# modules' hashes and the raw output. The tests call the frozen functions themselves
# (serve_http, start, stop, writer); the "control_" tests pass on the frozen code and show the
# harness reaches the code; every other test must FAIL there. Exit 0 only when that is so.
#
# This is a retrospective, independent reproduction on the frozen base. It does not claim the
# reds were seen before the first implementation, and it does not stand in for the tests that
# live with each change. Needs the same toolchain as scripts/windows/meetrec-test.cmd and
# CARGO_TARGET_DIR set outside the worktree. Remove the worktree afterwards with
#   git worktree remove --force <worktree dir>
set -u
FROZEN=889d48c9a2cca7f61a0648b832c3c3ac20953c78
here=$(cd "$(dirname "$0")/../.." && pwd)
work=${1:?usage: frozen-repro.sh <new worktree dir> <evidence dir>}
ev=${2:?usage: frozen-repro.sh <new worktree dir> <evidence dir>}
inc="$here/.specs/features/windows-port/frozen-repro"
src=spikes/windows-rust/meetrec/src
: "${CARGO_TARGET_DIR:?set CARGO_TARGET_DIR outside the worktree}"
mkdir -p "$ev" || exit 2

git -C "$here" worktree add --detach "$work" "$FROZEN" > "$ev/worktree.log" 2>&1 || { cat "$ev/worktree.log" >&2; exit 2; }
[[ $(git -C "$work" rev-parse HEAD) == "$FROZEN" ]] || { echo "worktree is not at $FROZEN" >&2; exit 2; }

declare -A BLOB=(
    [dictate.rs]=454881e185fb4f9a57f4c571cafc9c89e1e9612b
    [session.rs]=428327c5ffba5b511f5bc608631566495e81870d
    [main.rs]=772a5f51d7b32361950f6c1cb958deea8e44a93e
)
: > "$ev/frozen-sources.sha256"
for f in "${!BLOB[@]}"; do
    [[ $(git -C "$work" rev-parse "HEAD:$src/$f") == "${BLOB[$f]}" ]] || { echo "blob of $f is not the frozen one" >&2; exit 2; }
    (cd "$work" && sha256sum "$src/$f") >> "$ev/frozen-sources.sha256"
done
(cd "$inc" && sha256sum ./*.rs.inc) > "$ev/test-modules.sha256"

cat "$inc/dictate.f17.rs.inc" >> "$work/$src/dictate.rs"
cat "$inc/session.owners.rs.inc" >> "$work/$src/session.rs"
cat "$inc/main.writer.rs.inc" >> "$work/$src/main.rs"

MSYS2_ARG_CONV_EXCL='*' cmd /c "$(cygpath -w "$here/scripts/windows/frozen-repro.cmd")" "$(cygpath -w "$work")" > "$ev/frozen-run.log" 2>&1
echo "$?" > "$ev/frozen-run.exit"

grep -E '^test .* \.\.\. ' "$ev/frozen-run.log" | sed 's/\r$//' > "$ev/frozen-results.txt"
bad=0
while read -r _ name _ result; do
    case $name in
        *control_*) [[ $result == ok ]] || { echo "CONTROL FAILED: $name" >&2; bad=1; } ;;
        *) [[ $result == FAILED ]] || { echo "NOT RED at the frozen SHA: $name ($result)" >&2; bad=1; } ;;
    esac
done < "$ev/frozen-results.txt"
[[ -s "$ev/frozen-results.txt" ]] || { echo "no test ran: see $ev/frozen-run.log" >&2; bad=1; }
cat "$ev/frozen-results.txt"
exit $bad
