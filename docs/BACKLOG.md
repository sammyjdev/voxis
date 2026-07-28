# VOXIS - Backlog

Running list of open work, most actionable first. Last updated 2026-07-28.

VOXIS is macOS on Apple Silicon only. The Windows port was split off to
[Allyhgf/voxis-windows](https://github.com/Allyhgf/voxis-windows) on
2026-07-28; its code, packaging, CI matrix and design docs were removed here.
They remain in git history and in that fork.

---

## P0 - Decide the relationship with the Windows fork

- [ ] **Triage the fork's 44 commits.** The fork diverged with work that is not
      all Windows-specific. At least two look worth porting back to macOS:
      - `d75b04d` "Make the orb actually recover from a daemon restart"
      - `4c051b7` "Make the tray 'Hands-free' toggle actually do something at boot"

      Add the remote (`git remote add ally https://github.com/Allyhgf/voxis-windows.git`),
      read the diff, cherry-pick what applies.
- [ ] **Settle versioning after that.** Both repos currently declare `0.1.0`.
      The fork already tagged `v0.1.0-alpha` and `v0.2.0` on its own line, and
      those tags were never pushed here. Pick a scheme that does not read as a
      regression before tagging anything.

## P1 - macOS UX gaps

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

- Honest startup state: the tray reads a `ready` flag instead of port
  liveness, the dictation toggle is gone (`cmd_r` is the control), and the
  daemon persists for the session instead of reloading the model per toggle.
- PortAudio now recovers from a USB replug instead of holding a dead device.
- The daemon runs unbuffered, so `laris.log` actually captures `[Voice]` lines.
- Closing the settings window no longer quits the tray.
- Repo made public + MIT license added.
