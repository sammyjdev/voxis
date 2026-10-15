# VOXIS - Backlog

Running list of open work, most actionable first. Last updated 2026-10-15.

VOXIS runs on macOS with Apple Silicon. Dictation and the tray also run on
Linux (GNOME on Wayland) since 2026-10-14; see the P1 Linux section. The
Windows port was split off to
[Allyhgf/voxis-windows](https://github.com/Allyhgf/voxis-windows) on
2026-07-28; its code, packaging, CI matrix and design docs were removed here.
They remain in git history and in that fork.

---

## P1 - Linux as a second platform

Plan, decisions, criteria and spike evidence in
`docs/superpowers/plans/2026-10-14-linux-dictation.md`; install steps in
`docs/INSTALL.md` ("Linux (GNOME on Wayland)"). Additive: the macOS path and
the v1.0.0 ledger stay untouched.

- [x] **Dictation (slice 1).** Super+C toggles hands-free through `laris
      toggle`; faster-whisper on CUDA; paste via wl-copy + uinput
      Shift+Insert. In daily use since 2026-10-14.
- [x] **Orb on Linux.** X11 override-redirect popup under XWayland, on the
      system Python (it was cut at first). Checked live 2026-10-15: 190x190,
      override-redirect, mapped while listening, unmapped after; the owner
      confirmed a click on it passes through.
- [x] **Tray (slice 2).** Builds with `npx tauri build --no-bundle`, owns the
      daemon, icon red while dictating, starts at login via an XDG autostart
      entry.
- [ ] **Criterion 11:** dictate into a browser field and a Ptyxis prompt;
      record real per-utterance latency in the plan. The 2026-10-15 run logged
      no transcribed utterance (one 4 s listening window).
- [x] **Stopping hands-free dropped the speech in progress:** a tap right
      after speaking, before 400 ms of silence closed the utterance, lost it
      (both platforms). The open speech is now delivered on stop; a blip
      under 400 ms is still dropped.
- [x] **Tray default `laris_dir`** pointed at `~/code/voxis`, wrong on both
      owner machines. Now it is the checkout the tray was built from, when
      that checkout has a laris venv.
- [x] **Boot log says `denoise=on` on Linux,** where DeepFilterNet is not
      installed and nothing is denoised. Now it says `off (DeepFilterNet not
      installed)`, and a toggle-driven daemon no longer claims `modo: ptt`.
- [ ] **Tray menu actions on Linux.** Checked 2026-10-15: theme switching,
      meeting start/stop, "Sair" reaping the daemon. Left: the hands-free
      checkbox, which opens the mic.
- [x] **MIRIS on Linux (slice 3):** `pw-record` on the default sink monitor,
      `pipeline.py` through the platform backend; `miris doctor`, the CLI and
      the tray's "Gravar reunião" pass live. Set up for daily use
      2026-10-15: `~/.local/bin/miris`, `vault_root = "~/vault"` (the
      Obsidian vault; work meetings to `work/meetings`, interviews to
      `career/interviews`, own companies to `projects/meetings`, the rest to
      `inbox`), `agy` by full path. A real tray run saved its note in 26 s.
- [x] **Meetings were transcribed in the wrong language** (both platforms):
      Whisper's per-utterance guess put 12% of pt-BR lines in English,
      Russian, Japanese and others. The language is now guessed once per
      channel over its first 30 s of speech. Benchmark:
      `docs/superpowers/specs/2026-10-15-stt-model-benchmark.md`.
- [x] **Every tray meeting failed at the summary on Linux:** the tray started
      at login has no `~/.local/bin` on PATH, so a bare `agy` in
      `llm_command` was "No such file or directory", while `miris doctor` in
      a terminal said `[ok]`. The doctor now warns about a bare command and
      names the full path to use.
- [x] **The default `llm_command` named a model `agy` no longer offers**
      (`Gemini 3.5 Flash (Medium)`): every note failed at the summary. Now
      `Gemini 3.8 Flash (Medium)`.
- [ ] **Check the Mac's `~/.miris/config.toml`:** it was created with the old
      model name and a bare `agy`, which both break the summary from the
      tray. Run `miris doctor` there.
- [x] **Hands-free pasted consecutive sentences glued** ("falhas.Faz o
      commit"), both platforms. Every utterance after the first of a session
      now gets a leading space.
- [x] **The VAD clipped the first word** ("Qual a latência" -> "a
      latência"), both platforms. LARIS keeps 200 ms from before the VAD
      fires; WER on a replayed conversation 20.6-21.4% -> 16.6%.
- [x] **Dictation test with the owner's voice** (2026-10-15): turbo stays
      the best model (11.3% WER, 31/42 terms); with a vocabulary
      `INITIAL_PROMPT`, 4.4% and 40/42, at the cost of prompt words leaking
      on fillers. Numbers in the benchmark spec. The prompt is on for
      dictation on Linux since 2026-10-15; meetings never use it. If pasted
      fillers like "MIRIS, MIRIS" show up, a guard that drops output made
      only of prompt words caught 4 of the 9 leaks on CORAA.
- [ ] **Parakeet v3 for English meetings?** It cut English meeting errors by
      24% against Whisper turbo (18.4 vs 24.1 WER) and tied it in pt-BR. Costs
      a second STT backend on both platforms; worth it only if English
      meetings are common.
- [x] **The tray said "Falha ao processar reunião" after every meeting**
      (both platforms, since `1020c41`, 2026-09-14): the pipeline child
      stopped printing the `note: <path>` line the tray reads. Fixed
      2026-10-15.
- [ ] **Tray icon updates can be dropped.** The AppIndicator extension
      logged `Failed to recognize image format` reading the tray's icon PNG
      right as it changed (2026-10-15 00:26), likely reading a file still
      being written. Seen twice; the next poll repaints.
- [x] **Ducking on Linux (slice 4):** hands-free drops the default output
      to 20% through `wpctl` and gives the same sink its volume back after.
- [ ] **Per-app formatting on Linux.** GNOME gives clients no focused-window
      API. The disabled `voxis-context@sammyjdev` extension (not in this
      repo) already exports `org.voxis.Context.GetFocusedAppContext` on the
      session bus, but it also grabs `<Super>c` with an empty handler, which
      ate the dictation shortcut. Needs the extension in the repo without the
      grab, and a Linux reader in laris.
- [x] **Linux job in CI.** `.github/workflows/linux.yml` runs the laris and
      miris suites on Python 3.12 with `requirements-linux.txt` and the
      tray's `cargo test` on ubuntu-24.04. Green since run 38689785980
      (2026-10-15), after fixing an ETXTBSY race in the tests' fake python.

## P1 - macOS UX gaps

- [ ] **Settle versioning.** Every manifest declares `0.1.0` and there are no
      tags here. The fork tagged `v0.1.0-alpha` and `v0.2.0` on its own line;
      those were never pushed to this repo, so a `v0.1.0` tag here would read
      as older than a release that was never ours. The fork triage that blocked
      this is done - pick a scheme and tag.
- [ ] **INSTALL.md:** document that the **tray path** needs VOXIS.app in
      Accessibility + Input Monitoring. The doc currently only covers the
      source-install path (section 1).
- [ ] **Finish the tray smoke matrix:** meeting stop -> note written,
      hands-free, theme switching, and "Sair" tearing down the daemon.
- [ ] **Confirm "Sair" reaps the daemon.** An orphaned laris daemon (PPID=1,
      holding `:8421`) was seen after the tray exited. This matters more now
      that the daemon is spawned at startup and lives for the whole session:
      `LarisDaemon::stop` must actually reap the child, and a stale daemon must
      not block a fresh tray from spawning. (On Linux "Sair" reaps it: the
      tray, the daemon and the orb were gone within 0.5 s, 2026-10-15.)

## P2 - Deferred by design

- [ ] **Code signing / notarization.** Builds are unsigned, so Gatekeeper needs
      a one-time `xattr -cr` (documented in INSTALL.md). Real signing needs a
      paid Apple developer certificate.
- [ ] **Tray autostart on login.** macOS users can already do this via Login
      Items (documented in INSTALL.md); this item is only about shipping it as
      an in-app toggle, which costs a `tauri-plugin-autostart` dependency.
- [x] **A WER harness.** `scripts/stt_bench/` (2026-10-15) runs CORAA v1.1
      and AMI through any faster-whisper or onnx-asr config on an NVIDIA GPU.
      First use: model choice for both apps (keep large-v3-turbo) and the
      meeting language fix. Open questions it can now settle: "is
      `initial_prompt` helping?" and "is DeepFilterNet worth its 17% of
      per-utterance latency?". Only on Linux; the Mac's mlx path is not
      wired in.

---

## Done recently (for context)

- Triaged the fork's 46 commits (2026-07-28). Two were platform-independent
  bugs and were ported: `4c051b7` (ASSISTANT_MODE=always_on was read and never
  consumed, so the tray's Hands-free checkbox did nothing) and the server half
  of `d75b04d` (/ws/voice accepted before authenticating). Everything else is
  Windows-specific - Win32 `pid_alive`, WASAPI loopback, PyInstaller packaging,
  and the orb-in-a-Tauri-window line whose `overlay.rs` this repo removed.
  Two remain worth a look, neither urgent:
  - `149f4e9` - explicit UTF-8 when decoding LLM output. A cp1252 bug on
    Windows, but its `errors="replace"` would stop a misbehaving LLM from
    corrupting a note's filename.
  - `e1d208d` - surface `recorder.log` when the miris recorder fails to start.

- Honest startup state: the tray reads a `ready` flag instead of port
  liveness, the dictation toggle is gone (`cmd_r` is the control), and the
  daemon persists for the session instead of reloading the model per toggle.
- PortAudio now recovers from a USB replug instead of holding a dead device.
- The daemon runs unbuffered, so `laris.log` actually captures `[Voice]` lines.
- Closing the settings window no longer quits the tray.
- Repo made public + MIT license added.
