// Orb engine: voice ballistics + glitch scheduler + theme registry.
// Themes live in /static/themes/*.js and register window.ORB_THEMES[name].
// ?demo=1 replaces the websocket with a fake idle/speaking cycle.
(function () {
  const params = new URLSearchParams(window.location.search);
  const token = params.get("token");
  const demo = params.get("demo") === "1";
  if (!token && !demo) return;

  window.ORB_UTIL = {
    rgba(hex, a) {
      const r = parseInt(hex.slice(1, 3), 16), g = parseInt(hex.slice(3, 5), 16),
            b = parseInt(hex.slice(5, 7), 16);
      return `rgba(${r},${g},${b},${a})`;
    },
  };

  const wrap = document.getElementById("orb-canvas-wrap");
  const caption = document.getElementById("phaseCaption");
  const ampFill = document.getElementById("ampFill");
  const fallback = document.getElementById("orb-fallback");

  const PHASE_CAPTIONS = {
    recording: "ouvindo...",
    awaiting_project: "qual projeto?",
    awaiting_dictation: "pode ditar a mensagem...",
    awaiting_send: 'digitado - diga "manda" ou aperte enter',
  };

  let theme = { archetype: "waveform", palette: "blade", size: 190 };
  let palettes = { blade: { accent: "#3fb6c4", hot: "#d8f4f7" } };
  let instance = null;

  function mountTheme() {
    if (instance) instance.dispose();
    // wipe only the canvas slot - #ampFill and friends live in the wrap and
    // must stay attached (dashboard reuse contract)
    const slot = document.getElementById("orb-canvas-slot") || wrap;
    slot.innerHTML = "";
    const size = slot.clientWidth || wrap.clientWidth || theme.size;
    const factory = window.ORB_THEMES[theme.archetype] || window.ORB_THEMES.waveform;
    instance = factory(slot, size);
    if (fallback) fallback.classList.add("hide");
  }

  let fetchInFlight = false;
  async function fetchTheme() {
    if (demo || fetchInFlight) return;
    fetchInFlight = true;
    try {
      const resp = await fetch(`/theme?token=${encodeURIComponent(token)}`);
      if (!resp.ok) return;
      const body = await resp.json();
      palettes = body.palettes;
      const changed = body.theme.archetype !== theme.archetype
        || body.theme.size !== theme.size;  // size change needs a fresh canvas
      theme = body.theme;
      if (changed || !instance) mountTheme();
    } catch (e) { /* server restarting - keep current theme */ }
    finally { fetchInFlight = false; }
  }

  // --- voice state (ported ballistics: attack .55, release .10, pulse .88) ---
  let t = 0, targetEnergy = 0, smoothedVoice = 0, activationPulse = 0;
  let active = false, idleFrameSkip = 0;
  let glitchAt = 4 + Math.random() * 4, glitchLife = 0;

  function setPhase(phase) {
    const wasIdle = !active;
    active = phase !== "idle";
    if (wasIdle && active) activationPulse = 1.0;
    wrap.classList.toggle("active", active);
    const text = PHASE_CAPTIONS[phase];
    if (text) { caption.textContent = text; caption.classList.add("show"); }
    else caption.classList.remove("show");
  }

  function animate() {
    requestAnimationFrame(animate);
    if (!instance) return;
    const settled = !active && smoothedVoice < 0.01;
    if (settled) {
      idleFrameSkip = (idleFrameSkip + 1) % 8;
      if (idleFrameSkip !== 0) return;
    } else idleFrameSkip = 0;

    t += 0.014;
    if (activationPulse > 0.001) activationPulse *= 0.88; else activationPulse = 0;
    targetEnergy *= 0.96;
    const rate = targetEnergy > smoothedVoice ? 0.55 : 0.10;
    smoothedVoice += (targetEnergy - smoothedVoice) * rate;
    const v = Math.min(1, smoothedVoice * 0.95 + activationPulse * 0.5 + (active ? 0.12 : 0));
    if (ampFill) ampFill.style.width = Math.min(100, v * 100) + "%";

    glitchAt -= 1 / 60;
    if (glitchAt <= 0) { glitchLife = 0.12; glitchAt = 4 + Math.random() * 4; }
    if (glitchLife > 0) glitchLife -= 1 / 60;

    instance.draw(
      { t, active, v, glitch: glitchLife > 0, phase: null },
      palettes[theme.palette] || palettes.blade
    );
  }

  function connectVoice() {
    if (demo) {
      setInterval(() => {
        const on = (Math.floor(Date.now() / 7000) % 2) === 1;
        setPhase(on ? "recording" : "idle");
        if (on) {
          const phrase = (Math.sin(Date.now() / 1100) + 1) / 2;
          targetEnergy = phrase < 0.22 ? 0.02
            : 0.18 + Math.max(0, Math.sin(Date.now() / 190) * 0.5) * 0.6;
        }
      }, 100);
      return;
    }
    // The orb process lives as long as the daemon (hours), so a single dropped
    // socket used to leave it frozen in idle for the rest of the session: the
    // panel still showed and hid over stdin, but no phase or amplitude ever
    // arrived again. Backoff caps at 10s and only resets on a real open, so a
    // stale token (refused before the handshake, no onopen) backs off instead
    // of hammering.
    let delay = 500;
    (function open() {
      const ws = new WebSocket(`ws://${window.location.host}/ws/voice?token=${encodeURIComponent(token)}`);
      ws.onopen = () => { delay = 500; };
      ws.onmessage = (event) => {
        const data = JSON.parse(event.data);
        if (data.error) return;
        targetEnergy = data.amplitude;
        setPhase(data.phase);
      };
      ws.onclose = () => {
        setPhase("idle");
        setTimeout(open, delay);
        delay = Math.min(delay * 2, 10000);
      };
    })();
  }

  fetchTheme().then(() => { if (!instance) mountTheme(); });
  setInterval(fetchTheme, 3000);
  connectVoice();
  animate();
  setPhase("idle");
})();
