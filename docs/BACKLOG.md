# VOXIS - Backlog

Running list of open work, most actionable first. Last updated 2026-07-28.

VOXIS is macOS on Apple Silicon only. The Windows port was split off to
[Allyhgf/voxis-windows](https://github.com/Allyhgf/voxis-windows) on
2026-07-28; its code, packaging, CI matrix and design docs were removed here.
They remain in git history and in that fork.

---

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
      not block a fresh tray from spawning.

## P2 - Deferred by design

- [ ] **Code signing / notarization.** Builds are unsigned, so Gatekeeper needs
      a one-time `xattr -cr` (documented in INSTALL.md). Real signing needs a
      paid Apple developer certificate.
- [ ] **Tray autostart on login.** macOS users can already do this via Login
      Items (documented in INSTALL.md); this item is only about shipping it as
      an in-app toggle, which costs a `tauri-plugin-autostart` dependency.
- [ ] **A WER harness, if transcription quality ever becomes a complaint.**
      Nothing measures it today. CORAA ASR v1.1 (290 h of pt-BR, dev split on
      HuggingFace) needs no recording effort and would settle questions like
      "is `initial_prompt` helping?" or "is DeepFilterNet worth its 17% of
      per-utterance latency?" with data instead of impressions. See
      `docs/superpowers/specs/2026-07-27-laris-estado-inicial-design.md`.

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
