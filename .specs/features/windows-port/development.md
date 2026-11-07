# Windows port completion - development evidence

Author development evidence for plan `windows-port` (approved rev3, block
bea16b12-f0f2-4b5b-9450-f299bc7c0350, "Aprovar com PR"). It is not review,
verification or acceptance: Rita reviews the diff and Vera runs the matrix
and S10 independently (AC 65). Logs, exit files and hashes:
`Shared/evidence/dani-voxis-windows-port/` (copied from the build host's
`D:\voxis-port-work\evidence`).

## Identities

| Item | Value |
| --- | --- |
| Frozen PR #4 head and base of this branch | `889d48c9a2cca7f61a0648b832c3c3ac20953c78` (PR #4 draft, base `master` `e2971bf21bfc579bf3cacb207cc97063ecd6fd7a`) |
| Layer 1 baseline `H` (criterion 84, 87) | `77a4ba37d7ee72c99f3831c737fce1c34904bc89`, parent `889d48c`, exactly the 5 harness files |
| Branch | `dev/windows-port-fixes`, draft PR into `spike/windows-rust-stt` |
| Build host | Windows 11 x64 (10.0.26200), AMD Ryzen 9 9900X, 32 GB, NVIDIA GeForce RTX 4070 Ti (driver 617.42), rustc 1.99.1, CUDA toolkit 13.5 with `CUDAARCHS=89` for local native tests (the shipped build is 13.3.1 for `75;86;89;120`, built only by CI) |

## Baseline at `889d48c`

Native suite 25 passed, Windows tray suite 61 passed (`baseline-889d48c/`,
exit 0 each). The generated tray sidecar was copied from the owner's checkout
for the tray build only (SHA-256 `fa9bff6e...0ea0e0` at both ends,
`sidecar-copy.sha256`); it is not evidence of any installer.

## Red before green

Each production change was preceded by tests written from the criteria. The
old rule was first extracted, unchanged, into a function the tests call (the
seam), the tests ran red against it, then the rule changed in the same commit
as its tests. At `889d48c` itself these tests do not compile (the seams did not
exist), so the red runs are against the old rule, not the old commit; F17 and
F23 also reproduce through the real path (HTTP on loopback, a real child
process with a spaced folder).

| Slice | Red (old rule) | Green | Commit |
| --- | --- | --- | --- |
| S1 F17 overlay | 3 failed: `C:/.../sentinel.txt` accepted, the sentinel served 200 over HTTP, a junction served 200 (`red-S1-F17.log`) | 9/9 (`green-S1-F17.log`) | `bc92673` |
| S2 F18 folders, write failure | 6 failed, incl. the write panic at `main.rs:261`; the two `recover` tests already passed (characterization) | native 44/44 | `49c51f6` |
| S3 F19 F23 owners | 11 native failed (a dead recorder blocked `start`, `stop`/`process` ignored the owner); tray: a live recorder under `...\Sam Doe\...` showed Interrupted | native 58/58, tray 63/63 | `c3204a8` |
| S4 F20 F22 settings | frontend 5, tray 4, native 3 failed (`USB Microphone" # desk`, f13 not preselected, ignored keys saved) | frontend 5/5, tray 70, native 63 | `062d1a4` |
| S5 F21 deadlines | 8 failed: a stalled call ended only when the server dropped it (3.0 s, no deadline), a retry created a second Doc | native 74/74 | `c3a1381` |
| S6 dictation | 10 native failed (text in the log, silent paste failures, panics, 400 ms of speech measured 390 ms); tray 3 | native 90/90, tray 73 | `f9e4421` |
| S7 `--verify` | 4 failed (no re-check, corrupt file kept) | native 95/95 | `69fdb35` |
| S8 S9 documents and contract | 13 of 18 contract checks failed on the old docs; the 5 preservation checks passed | 18/18 | `691a6e7` |

Characterization tests (behaviour already right at `889d48c`, no production
change): `recover` header repair (AC 11), live-owner refusal (AC 12 part),
`ata.md` before Drive (AC 32), gesture timings (AC 35-38 except the 400 ms
boundary), the protocol guard of the driver (AC 69).

## Decisions taken while building

- `toml_edit` 0.25 (already in the native lock) instead of 0.22: 0.22 would
  have moved `proc-macro-crate` 2.0.2 to 2.0.0 and `toml_datetime` 0.6.3 to
  0.6.11, which criterion 66 forbids. The lock gains only `sysinfo` 0.37.2,
  `ntapi` 0.4.3 and `objc2-io-kit` 0.3.2.
- `sem GPU: usando CPU` reads ggml's device registry through three
  `extern "C"` declarations: whisper-rs re-exports it only with `raw-api`,
  a feature change that would alter the engine inputs criterion 84 compares.
- dictate's backend is not printed by new code: whisper-rs 0.16 routes
  whisper.cpp's log only with `log_backend`, also a feature change; the
  `using <name> backend` line reaches `laris.log` through stderr (AC 46).
- The tray keeps the "microphone lost" flag on its daemon handle, so the
  existing test that builds `AppState` field by field is untouched.
- Tests that open loopback sockets or start processes share one lock: a
  child started while another test's socket was open reset that connection.
- The harness self-tests land in their own commit after `H`, so `H` holds
  exactly the files criterion 87 lists.
- The native Run line of `checks.md` disables MSYS conversion, because
  `check_proofs.py` runs proofs in Git Bash.

## Development commands and observed results at the branch head

`check_proofs.py windows-port` at `ac0d020`: 131 run proofs and the build pass, 0 fail, tree clean (`proofs.md`, `check_proofs.log`); its 34 `walk` rows are pending, not walked, and
`build-vulkan.log`, `build-tray.log` for the Vulkan edition and tray release
builds (exit 0, no warnings). The harness smoke (`harness-smoke/`, one block,
two sweeps, native CUDA only, local 13.5 build) only shows the driver works
end to end; it is not an S10 measurement.

## Not done here, by the plan

- Matrix rows M1-M17 on candidate bytes, S10 layer 1 runs and layer 2
  (dictation and meeting latency): Vera, inside the Execution gate, after a
  reviewed SHA, Sammy's tag choice and the CI draft release.
- No installer, keyboard hook, recording, clipboard or real provider call ran
  in this session; no tag, release, merge or ready transition.
- Python suites (`packages/laris`, `packages/miris`) and the Linux tray suite
  run in CI; `packages/` is unchanged.
