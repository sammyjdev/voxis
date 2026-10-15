# VOXIS on Linux - Plan

**Status (2026-10-14):**

| Slice | Status |
|---|---|
| 1 - dictation | Done and used live. Open: criterion 11's browser + Ptyxis run and its latency record. |
| Orb on Linux | Implemented and showing live (was cut, now back: decision 5). Open: criterion 16's click-through and focus check. |
| 2 - tray | Working: runs, owns the daemon, icon red while dictating, starts at login, defaults `laris_dir` to its build checkout. Open: the fresh-login check (criterion 19); menu actions not checked on Linux. |
| 3 - MIRIS | Planned (criteria 20-29), not started. |
| 4 - polish | Autostart moved to slice 2 and done. Ducking open. |

Install steps: `docs/INSTALL.md`, section "Linux (GNOME on Wayland)".

**Goal:** use VOXIS day to day on the owner's Linux desktop, as a second
platform next to macOS. macOS behavior and the v1.0.0 validation in
`docs/releases/2026-09-14-v1.0.0-validation.md` stay untouched: every Linux
change is a new file or a new branch behind a platform check.

## Target machine

Ubuntu 26.04, GNOME 50.1 on Wayland, PipeWire 1.6, RTX 4070 Ti (12 GB,
driver 595), system Python 3.14. The user already has rw on `/dev/uinput`
(ACL); the user is not in the `input` group. That ACL comes only from the
`steam-devices` udev rule (`60-steam-input.rules`, `TAG+="uaccess"`); the
install doc gives a standalone rule for machines without it.

## Decisions (2026-10-14)

1. **Second platform, same repo.** The Linux backend is additive; `get_backend`
   gains one `linux` branch.
2. **Dictation first.** MIRIS and the tray come after dictation works.
3. **Trigger = GNOME custom shortcut + VAD, not push-to-talk.**
   - Evidence at the time: the `GlobalShortcuts` portal binds (`GrabAccelerators
     "<Super>c"` reaches gnome-shell, no error) but gnome-shell never emitted
     `AcceleratorActivated`, for physical presses or for uinput-injected ones.
   - Real cause, found later the same day: a leftover GNOME extension from an
     Oct 7 attempt, `voxis-context@sammyjdev` (never in this repo), grabbed
     `<Super>c` with an empty handler and ate every press. It is disabled, not
     deleted. With it off, the custom shortcut works. The portal was not
     re-tested; the decision stands because the custom shortcut works and
     needs no portal session.
   - `pynput` cannot see global keys on Wayland. `evdev` would need the `input`
     group, which lets any user process read the whole keyboard - rejected.
   - So: a GNOME custom shortcut (Settings > Keyboard > Custom Shortcuts) runs
     `python -m laris toggle`, which flips the daemon's existing hands-free
     mode. One tap starts it, silero-vad cuts each utterance on silence, each
     utterance is transcribed and pasted, the next tap stops it. The loop in
     `daemon.py` already does all of this when `hands_free` is set.
   - GNOME repeats a custom shortcut on key autorepeat (once on press, then
     every 30 ms after 500 ms), one `laris toggle` run each; a key held 1.2 s
     flipped hands-free 25 times. With no key-release signal, the daemon
     ignores a toggle less than 1 s after the previous one (criterion 6).
4. **Cut on Linux:** per-app formatting (GNOME Wayland gives clients no
   focused-window API; a Shell extension can expose it over D-Bus, and the
   disabled `voxis-context` extension does - deferred), the macOS
   Accessibility probe. Ducking is a no-op until slice 4.
5. **The orb is back, as an X11 popup under XWayland.** First cut because a
   native Wayland client gets no always-on-top window or position on GNOME.
   An X11 override-redirect popup (`Gtk.WindowType.POPUP`, `GDK_BACKEND=x11`)
   is kept above every window by mutter, placed where asked, and never
   focused, so the paste still lands in the user's app.
   - PyGObject does not build in the venv: the child runs on
     `/usr/bin/python3` (`python3-gi`, WebKit2 4.1) with the laris package on
     `PYTHONPATH` (`overlay_linux.py`).
   - Click-through: `gtk_widget_input_shape_combine_region` with an empty cairo
     region, called through ctypes (`python3-gi-cairo` is not installed by
     default). One set on the X window from outside was reset by GTK's first
     map. If it cannot be applied the orb stays hidden: an orb that swallows
     clicks is worse than none.

## Slice 1 - dictation tracer bullet

**Status: done.** Every automated criterion is green; dictation is in daily
use through Super+C. Open: criterion 11's browser + Ptyxis run and latency
record.

Thinnest path through the real entry point: GNOME shortcut -> `laris toggle`
-> daemon `/toggle` -> hands-free loop -> faster-whisper on CUDA -> paste into
the focused app.

### Files

- Create `packages/laris/laris/stt_faster.py`: restore from `b39a406^`, add
  `INITIAL_PROMPT` parity with `transcriber.py` (omit when empty). CTranslate2
  dlopens cuBLAS/cuDNN on first inference, so they are preloaded from the
  `nvidia-*` wheels; if they do not load, the model runs on CPU.
- Create `packages/laris/laris/dictation_linux.py`: `wl-copy` (CLIPBOARD and
  PRIMARY), one Shift+Insert through a python-evdev `UInput`, best-effort
  clipboard restore after the settle delay.
- Create `packages/laris/laris/platform/linux.py`: `LinuxBackend` with
  faster-whisper, the Linux paste, the XWayland orb and a no-op ducker;
  `ToggleTrigger`.
- Create `packages/laris/laris/overlay_linux.py`: the orb child (decision 5).
- Modify `packages/laris/laris/overlay.py`: the child's argv and env are
  injectable (macOS default unchanged).
- Modify `packages/laris/laris/platform/__init__.py`: `linux` branch.
- Modify `packages/laris/laris/server.py`: `POST /toggle?token=` flips the
  hands-free event; bad or missing token -> 403, no change; no toggle
  trigger (macOS) -> 409. `GET /theme` reports `dictating`.
- Modify `packages/laris/laris/__main__.py`: `python -m laris toggle` reads the
  laris peer from `peers.json` and POSTs `/toggle`; on Linux, skip the Quartz
  probe and drive the loop with an event-only trigger instead of `PushToTalk`.
- Modify `packages/laris/laris/daemon.py`: accept an injected trigger
  (default `PushToTalk(config.DICTATE_KEY)`, so macOS is unchanged).
- Create `packages/laris/requirements-linux.txt`: faster-whisper, the CUDA 12
  cuBLAS and cuDNN 9 wheels, sounddevice, silero-vad, evdev, fastapi,
  uvicorn, numpy, pytest, httpx. torch comes from the CPU index first. Venv
  on Python 3.12 via `uv`, not the system 3.14.

### Acceptance criteria

1. Given `sys.platform == "linux"`, when `get_backend()` runs, then it returns
   a `LinuxBackend`.
2. Given ctranslate2 reports one CUDA device, when the transcriber is built,
   then the model loads with `device="cuda"`, `compute_type="float16"`; with
   zero devices it loads with `"cpu"`, `"int8"`.
3. Given `INITIAL_PROMPT="VOXIS, LARIS"`, when an utterance is transcribed,
   then faster-whisper receives `initial_prompt="VOXIS, LARIS"`; given it is
   empty, then no `initial_prompt` argument is passed.
4. Given the clipboard holds `"antigo"`, when `type_text("ação, olá")` runs,
   then `wl-copy` receives exactly `"ação, olá"` for CLIPBOARD and PRIMARY, one
   Shift+Insert (down/up) is written to uinput, and afterwards the clipboard is
   restored to `"antigo"`.
5. Given `wl-copy` fails, when `type_text` runs, then no key is sent and the
   voice loop keeps running.
6. Given a running daemon with token `T`, when `POST /toggle?token=T` is sent
   twice more than 1 s apart, then hands-free is on after the first call and
   off after the second; a call less than 1 s after the previous one (a GNOME
   key repeat, or a quick re-tap) changes nothing.
7. Given a wrong or missing token, when `POST /toggle` is sent, then the
   response is 403 and hands-free does not change.
8. Given `peers.json` has the laris URL and token, when `python -m laris
   toggle` runs, then exactly one `POST /toggle` with that token is sent;
   given no daemon answers, then it exits non-zero with a one-line reason.
9. Given Linux, when the daemon boots, then it never imports `Quartz`,
   `AppKit`, `pynput` or `mlx_whisper`.
10. Given the existing LARIS suite, when it runs after the change, then every
    test that passed before still passes (macOS path untouched).
11. Manual, end to end: with a GNOME shortcut bound to `laris toggle`, tap,
    say "teste de ditado no linux", and the text lands in a browser text field
    and in a Ptyxis terminal prompt; tap again and capture stops. Record the
    per-utterance latency from the `[Voice]` log line.
12. Given the uinput keystroke fails partway, when `type_text` returns, then
    both Shift and Insert have been released and the dictated text stays on
    the clipboard.
13. Given Linux, when the daemon boots and when a toggled session starts, then
    the log names the faster-whisper model and `laris toggle`, never `cmd_r`
    or an mlx model.

### Orb criteria (decision 5)

14. Given Linux, when the daemon shows the orb, then the child runs
    `/usr/bin/python3 -m laris.overlay_linux` with `GDK_BACKEND=x11`,
    `OVERLAY_URL` set and the laris package root on `PYTHONPATH`.
15. Given a theme position and the primary monitor's geometry, when the orb is
    placed, then it lands at the same visual spot as on macOS, in X11
    top-left coordinates, on that monitor (bottom-center: 16 px above the
    bottom edge; top-right: 40 px below the top edge).
16. Manual: while hands-free is on, `xwininfo` shows a 190x190
    override-redirect window; clicks pass through it and focus stays in the
    user's app.

### Log criteria (2026-10-15)

Numbered after slice 3's 20-29 so earlier numbers stay stable.

30. Given hands-free is on, when a 1.5 s utterance is transcribed in 0.42 s
    and pasted, then its line reads
    `[Voice] hands-free: '<text>' (audio=1.5s, stt=0.42s, amp=<amp>)`.
31. Given Linux with `NOISE_REDUCTION=on` and no DeepFilterNet, when the daemon
    boots, then the STT line says `denoise=off (DeepFilterNet not installed)`;
    with DeepFilterNet it says `denoise=on`; macOS still prints the configured
    value.
32. Given a toggle trigger drives the loop and `MODE=ptt`, when the daemon is
    ready, then the ready line ends at `toggle: laris toggle`, with no
    `modo: ptt`; with `MODE=always_on` it ends `, modo: always_on`; a
    push-to-talk daemon still ends `PTT: <key>, modo: <mode>`.

### To verify in the tracer, not assumed

- `wl-copy` on mutter does not steal focus from the target window: verified
  live, the paste lands in the focused app.
- Shift+Insert pastes CLIPBOARD (not PRIMARY) in Ptyxis; setting both covers
  either. Pending criterion 11. If a target app ignores Shift+Insert, the
  fallback is a configurable combo (`PASTE_KEYS`).
- `sounddevice` at 16 kHz through PipeWire: verified live.
- `NOISE_REDUCTION`: DeepFilterNet is not in the Linux requirements, so audio
  is not denoised; the boot log says so (criterion 31).

## Slice 2 - tray on Linux

**Status: working (2026-10-14).**

- Builds with `npx tauri build --no-bundle`; `tauri.linux.conf.json` drops
  the macOS-only `GrantMic.app` resource.
- Spawns and owns the daemon the same way as on macOS; log in
  `~/.config/voice-at/laris.log`. `~/.config/voice-at/tray.json` points
  `laris_dir` at the checkout.
- The icon turns red while dictation listens: `/theme` gains `dictating`.
- Starts at login from an XDG autostart entry (moved here from slice 4),
  which GNOME runs as the user unit `app-voxis\x2dtray@autostart.service`.
- Default `laris_dir` (2026-10-15): the checkout the tray was built from,
  baked at compile time, used only when its `.venv/bin/python` exists;
  otherwise `~/code/voxis/packages/laris` as before. `tray.json` still wins.
  A CI release bakes a runner path that exists nowhere else and falls back.

Open:

- Not checked on Linux: the menu actions (hands-free checkbox, theme
  switching, "Sair" reaping the daemon).

### Acceptance criteria

17. Given a daemon with a toggle trigger, when the tray polls `/theme`, then
    `dictating` is `true` while hands-free is on and `false` while it is off;
    a daemon with no toggle trigger (macOS) always answers `false`.
18. Given no meeting is recording, when the tray's snapshot says `dictating`,
    then the icon is the red live icon; with neither, it is the idle icon.
19. Manual: given a fresh GNOME login with the autostart entry, when the
    session starts, then the tray runs and owns a daemon whose `/theme`
    answers `ready: true`.
33. Given the tray was built from a checkout whose `packages/laris/.venv`
    exists and `tray.json` sets no `laris_dir`, when the tray starts, then it
    runs the daemon from that checkout; given the checkout has no venv (a CI
    release), the default stays `~/code/voxis/packages/laris`. (Numbered
    after the log criteria.)

## Criterion -> test

Python tests are in `packages/laris/tests/`; Rust tests in
`apps/tray/src-tauri/src/lib.rs`.

| # | Tests |
|---|---|
| 1 | `test_platform.py::test_get_backend_linux_returns_linux_backend`, `test_platform.py::test_linux_backend_transcribes_with_faster_whisper_and_pastes_with_the_linux_path` |
| 2 | `test_stt_faster.py::test_with_a_cuda_gpu_the_model_runs_on_cuda_in_float16`, `::test_without_a_cuda_gpu_the_model_runs_on_cpu_in_int8`, `::test_a_gpu_whose_cuda_libraries_cannot_load_falls_back_to_cpu`, `::test_a_gpu_with_loadable_cuda_libraries_runs_on_cuda`, `::test_without_a_gpu_the_cuda_libraries_are_never_loaded` |
| 3 | `test_stt_faster.py::test_initial_prompt_is_forwarded_when_configured`, `::test_no_initial_prompt_is_sent_when_unset` |
| 4 | `test_dictation_linux.py::test_dictated_text_is_pasted_with_shift_insert_and_the_old_clipboard_comes_back`, `::test_the_users_primary_selection_comes_back_after_the_paste` |
| 5 | `test_dictation_linux.py::test_when_the_clipboard_cannot_be_set_no_key_is_typed`, `::test_when_the_primary_selection_cannot_be_set_the_old_clipboard_comes_back` |
| 6 | `test_toggle.py::test_each_toggle_command_flips_hands_free_on_the_running_daemon`, `::test_a_held_shortcut_repeating_every_30_ms_turns_hands_free_on_only_once`, `::test_a_press_more_than_a_second_after_the_last_repeat_turns_hands_free_off`, `::test_a_second_tap_more_than_a_second_later_counts_as_a_new_press`, `::test_a_second_tap_within_a_second_is_taken_for_the_same_held_press`, `::test_concurrent_toggles_from_one_press_never_flip_twice` |
| 7 | `test_toggle.py::test_a_toggle_with_a_wrong_token_is_refused_and_changes_nothing`, `::test_a_toggle_without_a_token_is_refused_and_changes_nothing` |
| 8 | `test_toggle.py::test_each_toggle_command_flips_hands_free_on_the_running_daemon`, `::test_the_toggle_command_fails_with_a_reason_when_no_daemon_answers`, `::test_the_toggle_command_fails_with_a_reason_when_no_daemon_is_recorded` |
| 9 | `test_linux_boot.py::test_a_daemon_driven_by_the_toggle_trigger_reaches_ready_without_a_key_listener`, `::test_on_linux_a_toggle_reaches_the_event_the_daemon_loop_waits_on` |
| 10 | the whole suite: `cd packages/laris && .venv/bin/python -m pytest -q` (one failure predates this work: `test_audio_capture.py::test_denoise_on_changes_audio`, red on `e635955` too) |
| 11 | manual |
| 12 | `test_dictation_linux.py::test_a_keystroke_that_fails_midway_still_releases_shift`, `::test_a_failed_insert_release_still_releases_shift`, `::test_when_the_keystroke_fails_the_text_stays_on_the_clipboard_for_a_manual_paste` |
| 13 | `test_linux_boot.py::test_on_linux_the_boot_log_names_the_faster_whisper_model_and_the_toggle`, `::test_a_toggled_hands_free_session_tells_how_to_stop_it_with_laris_toggle` |
| 14 | `test_platform.py::test_on_linux_the_orb_runs_in_the_system_python_on_xwayland` |
| 15 | `test_overlay_linux.py::test_a_bottom_center_orb_sits_16_px_above_the_bottom_edge`, `::test_a_top_right_orb_sits_40_px_below_the_top_edge`, `::test_the_orb_lands_on_a_monitor_to_the_right_of_another` |
| 16 | manual |
| 17 | `test_toggle.py::test_the_theme_poll_reports_dictating_while_toggled_hands_free_is_on`, `::test_a_daemon_without_a_toggle_trigger_never_reports_dictating` |
| 18 | `lib.rs::tests::the_icon_turns_red_while_dictation_is_listening`, `::the_icon_stays_idle_with_no_meeting_and_no_dictation` |
| 19 | manual |
| 20-29 | slice 3, not written yet |
| 30 | `test_daemon.py::test_each_hands_free_line_shows_the_audio_length_and_the_stt_time` |
| 31 | `test_linux_boot.py::test_on_linux_without_deepfilternet_the_boot_log_says_denoise_is_off`, `::test_on_linux_with_deepfilternet_the_boot_log_says_denoise_is_on`, `::test_on_macos_the_boot_log_still_reports_the_configured_denoise_setting` |
| 32 | `test_linux_boot.py::test_a_toggle_driven_daemon_does_not_claim_push_to_talk_mode`, `::test_a_toggle_driven_daemon_that_boots_listening_says_always_on`, `test_daemon.py::test_a_push_to_talk_daemon_names_its_key_and_mode_when_ready` |
| 33 | `settings.rs::tests::a_source_build_defaults_to_the_laris_venv_of_its_own_checkout`, `::a_build_whose_checkout_has_no_venv_falls_back_to_code_voxis`, `::the_baked_in_build_checkout_is_this_repos_laris_package`, `::tray_config_defaults`; live 2026-10-15: with `tray.json` moved aside, the tray ran `<checkout>/packages/laris/.venv/bin/python -u -m laris` |

## Slice 3 - MIRIS on Linux

**Status: planned, not started.** Meeting recording from the tray on Linux:
microphone on one channel, everything the speakers play on the other, then
the existing transcript -> notes -> vault pipeline.

Approach: a second system-audio tap with the same four methods as
`SystemTap` (`start`, `alive`, `bytes_written`, `stop`), so `recorder.record`
and the whole manifest lifecycle run unchanged; only the tap and the
transcriber are picked per platform. The mic channel already works on Linux
(`sounddevice` through PipeWire, same as dictation).

### Files

- Create `packages/miris/miris/systemtap_linux.py`: `PwRecordTap`, which runs
  `pw-record` on the default sink's monitor (`-P '{ stream.capture.sink=true }'`,
  no `--target`), `--rate 16000 --channels 1 --format s16`, into
  `system.wav`. Mono 16 kHz s16 is what `transcribe.load_wav_mono16k` reads
  without resampling.
- Modify `packages/miris/miris/systemtap.py`: a `default_tap_factory()` that
  returns `PwRecordTap` on Linux and `SystemTap` (audiotap) on macOS.
- Modify `packages/miris/miris/recorder.py`: `record()` defaults its
  `tap_factory` to that selector instead of `SystemTap`.
- Modify `packages/miris/miris/cli.py`: `miris doctor` checks `pw-record` on
  Linux instead of the audiotap binary; the bounded capture is unchanged.
- Modify `packages/miris/miris/pipeline.py:27`: the default transcriber comes
  from `laris.platform.get_backend()` (keeping the `STT_LANGUAGE=""`
  default), wrapped so MIRIS's one-argument `transcribe(audio)` passes
  `sample_rate=16000`.
- Create `packages/miris/requirements-linux.txt`: `-r ../laris/requirements-linux.txt`, `-e ../laris`, `-e .`.
- Docs: a MIRIS step in the INSTALL.md Linux section (venv, `pipewire-bin`
  for `pw-record`, `~/.local/bin/miris`, `miris doctor`).

### Acceptance criteria

20. Given Linux, when the recorder starts the system tap for
    `<session>/system.wav`, then it runs exactly
    `pw-record -P '{ stream.capture.sink=true }' --rate 16000 --channels 1 --format s16 <session>/system.wav`.
21. Given `pw-record` is not on PATH, when the recorder starts, then the
    session ends `FAILED` with error code `system_tap_start_failed` and a
    detail that names `pw-record`.
22. Given a running `PwRecordTap`, when `stop()` is called, then `pw-record`
    receives SIGTERM, and is killed if it has not exited within the grace
    period; a second `stop()` does nothing.
23. Given a running `PwRecordTap`, when `pw-record` exits on its own, then
    `alive()` is `False` (the recorder turns that into `system_tap_died`).
24. Given `sys.platform` is `linux`, when `default_tap_factory()` runs, then
    it returns a `PwRecordTap`; given `darwin`, the audiotap `SystemTap`,
    with the same binary resolution as today.
25. Given Linux, when `build_transcript` builds its default transcriber, then
    it uses the Linux backend (faster-whisper), never imports
    `laris.transcriber` or `mlx_whisper`, keeps `STT_LANGUAGE` at `""` unless
    the user set it, and every span reaches the backend with
    `sample_rate=16000`.
26. Given macOS, when `build_transcript` builds its default transcriber, then
    the text still comes from `laris.transcriber.Transcriber` (mlx), with the
    same calls as before.
27. Given Linux, when `miris doctor` runs, then it reports `pw-record` instead
    of the audiotap binary, and its 3 s capture reports frames on both
    channels.
28. Given the existing MIRIS and tray suites, when they run after the change,
    then every test that passed before still passes (macOS untouched).
29. Manual, end to end on Linux: with `tray.json`'s `miris_bin` pointing at
    the Linux install, click "Gravar reunião", play a video with speech while
    talking into the mic for ~1 min, click stop; the tray icon is red while
    recording, the notification names the note, and the note's transcript has
    lines from both "Eu" and "Eles".

### To verify in a spike before the code, not assumed

- `pw-record` on the sink monitor keeps writing while nothing plays. The
  recorder fails a session whose `system.wav` stops growing for 2 s
  (`system_tap_stalled`) and one that writes nothing in the first 5 s; a
  suspended sink would trip both.
- `bytes_written()` (file size) grows as audio arrives, not in large
  buffered bursts.
- On SIGTERM `pw-record` finalizes the WAV header, so `session.inspect_wav`
  and `validate_capture` accept the file; the header is 44 bytes.
- `stream.capture.sink=true` with no `--target` links to the default sink's
  monitor, not to a microphone.
- What happens when the default sink changes mid-meeting (headset plugged
  in): the stream follows it, or keeps recording the old sink.
- VRAM: the daemon's resident model and MIRIS's second faster-whisper
  instance fit together on the 12 GB card.
- The tray's alive check (`ps -p PID -o command=`) and its notifications work
  on Linux.

## Slice 4 - polish (outline)

Ducking with `wpctl set-volume @DEFAULT_AUDIO_SINK@`. (Autostart moved to
slice 2 and is done.)
