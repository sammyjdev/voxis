# VOXIS - Backlog

Running list of open work, most actionable first. Last updated 2026-07-17.

Context: Phase 3 (Windows dictation) is code-complete and CI-green - a 174MB
Windows installer with a frozen torch-free engine builds end-to-end. The only
thing between here and a v0.1.0 release is a real Windows smoke test. See
`docs/superpowers/specs/2026-07-17-windows-dictation-design.md` and the three
`docs/superpowers/plans/2026-07-17-windows-dictation-p*.md` for the design.

---

## P0 - Ship Windows dictation (the active thread)

- [ ] **Windows smoke test.** Install the built installer on a real Windows
      machine (a friend, or SammyJrDesktop over Tailscale/RDP), grant the mic
      under Settings > Privacy > Microphone, hold Right-Shift, speak, confirm
      the text lands in Notepad and the orb appears/reacts.
      - Installer: CI run artifact `voxis-tray-windows-latest`
        (`gh run download <run-id> --repo sammyjdev/voxis -n voxis-tray-windows-latest`),
        or a local copy at `~/Downloads/voxis-v0.1.0-win/nsis/VOXIS_0.1.0_x64-setup.exe`.
      - Risk: the engine *builds* but has never *run* - PyInstaller can produce
        a binary that crashes at launch on a missing runtime hidden-import.
- [ ] **If the smoke fails:** read `%USERPROFILE%\.config\voice-at\laris-engine.log`
      (the tray redirects the windowed engine's stdout/stderr there), fix the
      PyInstaller hidden-import / DLL gap in `packages/laris/packaging/laris.spec`,
      re-dispatch `tray-build.yml`, re-test.
- [ ] **On a green smoke:** tag `v0.1.0` (`git tag v0.1.0 && git push origin v0.1.0`)
      to draft the public GitHub release with all installers - then friends
      download the `.exe` directly (repo is public now, no account needed).

## P1 - macOS UX gap (found 2026-07-17)

- [x] **Surface the Accessibility permission requirement.** (done 2026-07-25)
      `PushToTalk.check_trusted()` now probes a listen-only `CGEventTap`
      (catches BOTH Accessibility and Input Monitoring - `AXIsProcessTrusted`
      alone gave false positives), laris caches it at boot into
      `app.state.ptt_trusted`, `/theme` carries it, and the tray notifies once
      on the edge into "daemon up but untrusted" (`should_warn_untrusted`).
- [ ] **INSTALL.md:** document that the **tray path** needs VOXIS.app in
      Accessibility + Input Monitoring (the doc currently only covers the
      source-install path, section 1).

## P1 - Tray lifecycle / remaining macOS smoke

- [ ] **Confirm "Sair" kills the spawned daemon.** An orphaned laris daemon
      (PPID=1, holding `:8421`) was seen after the tray exited. Verify the
      tray's quit path (`LarisDaemon::stop`) actually reaps the child, and that
      a stale daemon does not block a fresh tray from spawning (port conflict).
- [ ] **Finish the macOS tray smoke matrix:** stop -> note written, dictation /
      hands-free / theme switching, "Sair" tears down the daemon.

## P2 - Deferred by design (open a phase when needed)

- [ ] **CUDA acceleration (Windows).** v1 ships CPU int8 only; CUDA is a
      documented manual upgrade. The spec's original opportunistic
      CUDA-runtime-swap was deferred as too fragile to build without Windows
      hardware to test. Revisit if CPU latency is a real complaint.
- [ ] **Linux dictation (Phase 4).** Not built. Needs a `LinuxBackend`
      (largely reusable from `WindowsBackend`), engine packaging on the Linux
      runner (`tauri.linux.conf.json` overlay, same pattern as Windows), and a
      Linux orb window. **Real blocker:** input injection - pynput global
      hotkey + synthetic Ctrl+V work on **X11** but are restricted on
      **Wayland** (the modern default). X11 is tractable; Wayland needs heavy
      work. Only start with a concrete Linux tester (ask X11 vs Wayland first).
- [ ] **MIRIS meetings on Windows.** Dictation-only today; meetings need WASAPI
      loopback capture. A phase of its own after Windows dictation lands.
- [ ] **Code signing / notarization.** Builds are unsigned (macOS Gatekeeper,
      Windows SmartScreen); workarounds are documented in INSTALL.md. Real
      signing needs paid Apple/Windows developer certificates. Deferred.
- [ ] **Tray autostart on login** (macOS/Windows). v1.1 nicety. macOS users can
      already do this via Login Items (documented in INSTALL.md); this item is
      only about shipping it as an in-app toggle, which costs a
      `tauri-plugin-autostart` dependency.

---

## Done recently (for context)

- Phase 3 Windows dictation: platform abstraction, faster-whisper backend,
  clipboard/Ctrl+V, Tauri orb window, PyInstaller packaging, CI installer -
  all merged, CI green (run 29607463362).
- Windows engine logging to `laris-engine.log` (crash diagnostics for testers).
- Repo made public + MIT license added.
