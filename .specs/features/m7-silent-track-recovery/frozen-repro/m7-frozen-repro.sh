#!/usr/bin/env bash
# M7 frozen red, adapted from scripts/windows/frozen-repro.sh (criterion 26).
#
#   m7-frozen-repro.sh <new worktree dir> <evidence dir>
#
# Adds a detached worktree at the untouched base c2d7b6c (no branch), verifies the blob of the one
# file it touches, appends the test-only module session.m7.rs.inc (tests only, no production line),
# and keeps the source hash, the module hash, the exact diff and the numstat. It does not build or
# run: the caller runs the no-run compile and the selected tests separately so their times are
# measured apart. It never removes the worktree (no `git worktree remove`): the worktree is retained.
set -u
BASE=c2d7b6c6294c9fe67709cc6c2b1c1650fdff5de1
here=$(cd "$(dirname "$0")/../../../.." && pwd)
work=${1:?usage: m7-frozen-repro.sh <new worktree dir> <evidence dir>}
ev=${2:?usage: m7-frozen-repro.sh <new worktree dir> <evidence dir>}
inc="$here/.specs/features/m7-silent-track-recovery/frozen-repro/session.m7.rs.inc"
src=spikes/windows-rust/meetrec/src/session.rs
mkdir -p "$ev" || exit 2
[[ -e $work ]] && { echo "worktree dir already exists: $work" >&2; exit 2; }

git -C "$here" worktree add --detach "$work" "$BASE" > "$ev/worktree.log" 2>&1 || { cat "$ev/worktree.log" >&2; exit 2; }
[[ $(git -C "$work" rev-parse HEAD) == "$BASE" ]] || { echo "worktree is not at $BASE" >&2; exit 2; }
[[ -z $(git -C "$work" status --porcelain) ]] || { echo "worktree is not clean before the append" >&2; exit 2; }
blob=$(git -C "$work" rev-parse "HEAD:$src")
[[ $(git -C "$work" hash-object "$src") == "$blob" ]] || { echo "working $src differs from the base blob" >&2; exit 2; }
echo "$blob  $src" > "$ev/base-blob.txt"
(cd "$work" && sha256sum "$src") > "$ev/base-source.sha256"
sha256sum "$inc" > "$ev/test-module.sha256"
git hash-object "$inc" > "$ev/test-module.blob"

cat "$inc" >> "$work/$src"
git -C "$work" diff > "$ev/frozen-test-patch.diff"
git -C "$work" diff --numstat > "$ev/frozen-test-patch.numstat"
sha256sum "$ev/frozen-test-patch.diff" > "$ev/frozen-test-patch.sha256"
(cd "$work" && sha256sum "$src") > "$ev/patched-source.sha256"
# the only change to the base tree is appended text in one file: no deleted line, no other file
awk '$2 != 0 { bad = 1 } END { exit bad }' "$ev/frozen-test-patch.numstat" || { echo "the patch removes lines of the base tree" >&2; exit 2; }
[[ $(wc -l < "$ev/frozen-test-patch.numstat") -eq 1 ]] || { echo "the patch touches more than one file" >&2; exit 2; }
echo "ok: $work at $BASE, $src base blob $blob, module appended"
