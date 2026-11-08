import { invoke } from "@tauri-apps/api/core";
import {
  ALL_DICTATE_KEYS, ditadoUpdates, keyChoice, languageNote, legacyControls, NOT_ON_WINDOWS,
  type Platform,
} from "./settings-logic";

type Settings = {
  laris: Record<string, string>;
  miris: {
    vault_root?: string;
    llm_command?: string[];
    categories?: Record<string, string>;
  };
  theme: {
    archetype: string;
    palette: string;
    position: string;
    size: number;
  } | null;
  laris_alive: boolean;
  version: string;
  // absent from an older tray: every key, nothing marked unavailable
  platform?: Platform;
  dictate_keys?: string[];
};

let platform: Platform = "other";

const $ = <T extends HTMLElement>(sel: string) =>
  document.querySelector(sel) as T;

function feedback(msg: string, isError = false) {
  const el = $<HTMLParagraphElement>("#feedback");
  el.textContent = msg;
  el.classList.toggle("error", isError);
  setTimeout(() => { el.textContent = ""; }, 4000);
}

function categoryRow(name = "", folder = ""): HTMLTableRowElement {
  const tr = document.createElement("tr");
  tr.innerHTML = `
    <td><input type="text" class="cat-name" value=""></td>
    <td><input type="text" class="cat-folder" value=""></td>
    <td><button type="button" class="remove" aria-label="Remover categoria">x</button></td>`;
  (tr.querySelector(".cat-name") as HTMLInputElement).value = name;
  (tr.querySelector(".cat-folder") as HTMLInputElement).value = folder;
  (tr.querySelector(".remove") as HTMLButtonElement).onclick = () => tr.remove();
  return tr;
}

type AudioDevices = {
  inputs: string[]; outputs: string[];
  default_input: string; default_output: string;
  mic: string; speaker: string;
};

function fillDevices(sel: HTMLSelectElement, names: string[], fallback: string, chosen: string) {
  sel.innerHTML = "";
  const def = document.createElement("option");
  def.value = "";
  def.textContent = `Padrão do sistema${fallback ? ` (${fallback})` : ""}`;
  sel.appendChild(def);
  // a chosen device that is unplugged stays listed, so saving doesn't drop it
  const all = chosen && !names.includes(chosen) ? [...names, chosen] : names;
  for (const n of all) {
    const o = document.createElement("option");
    o.value = n;
    o.textContent = names.includes(n) ? n : `${n} (desconectado)`;
    sel.appendChild(o);
  }
  sel.value = chosen;
}

async function loadAudio() {
  try {
    const d = await invoke<AudioDevices>("get_audio_devices");
    fillDevices($<HTMLSelectElement>("#audio-mic"), d.inputs, d.default_input, d.mic);
    fillDevices($<HTMLSelectElement>("#audio-speaker"), d.outputs, d.default_output, d.speaker);
  } catch (e) {
    feedback(`Dispositivos de áudio: ${e}`, true);
  }
}

async function load() {
  loadAudio();
  const s = await invoke<Settings>("get_settings");

  platform = s.platform ?? "other";
  const keys = s.dictate_keys ?? ALL_DICTATE_KEYS;
  const keySelect = $<HTMLSelectElement>("#dictate-key");
  keySelect.innerHTML = keys
    .map((k) => `<option value="${k}">${k}</option>`)
    .join("");
  const choice = keyChoice(keys, s.laris.DICTATE_KEY, platform);
  keySelect.value = choice.selected;
  const notice = $<HTMLParagraphElement>("#dictate-key-notice");
  notice.textContent = choice.notice;
  notice.hidden = !choice.notice;
  const language = $<HTMLParagraphElement>("#language");
  language.textContent = languageNote(platform);
  language.hidden = !language.textContent;
  $<HTMLInputElement>("#hands-free").checked =
    s.laris.ASSISTANT_MODE === "always_on";
  $<HTMLInputElement>("#stt-model").value =
    s.laris.STT_MODEL ?? "mlx-community/whisper-large-v3-turbo-q4";
  $<HTMLSelectElement>("#noise").value = s.laris.NOISE_REDUCTION ?? "on";
  $<HTMLTextAreaElement>("#initial-prompt").value = s.laris.INITIAL_PROMPT ?? "";

  $<HTMLInputElement>("#vault-root").value = s.miris.vault_root ?? "~/vault";
  $<HTMLTextAreaElement>("#llm-command").value =
    (s.miris.llm_command ?? []).join("\n");
  const tbody = $<HTMLTableSectionElement>("#categories tbody");
  tbody.innerHTML = "";
  for (const [name, folder] of Object.entries(s.miris.categories ?? {})) {
    tbody.appendChild(categoryRow(name, folder));
  }

  const orbOffline = !s.laris_alive || s.theme === null;
  $<HTMLParagraphElement>("#orb-offline").hidden = !orbOffline;
  for (const id of ["#orb-archetype", "#orb-palette", "#orb-position", "#orb-size", "#save-orb"]) {
    ($(id) as HTMLInputElement | HTMLSelectElement | HTMLButtonElement).disabled = orbOffline;
  }
  if (s.theme) {
    $<HTMLSelectElement>("#orb-archetype").value = s.theme.archetype;
    $<HTMLSelectElement>("#orb-palette").value = s.theme.palette;
    $<HTMLSelectElement>("#orb-position").value = s.theme.position;
    $<HTMLInputElement>("#orb-size").value = String(s.theme.size);
  }

  for (const c of legacyControls(platform)) {
    const box = $<HTMLElement>(`#${c.id}`);
    for (const el of box.matches("input, select, textarea, button") ? [box] : box.querySelectorAll<HTMLInputElement>("input, select, textarea, button")) {
      (el as HTMLInputElement).disabled = true;
    }
    const note = document.createElement("small");
    note.className = "unavailable";
    note.textContent = NOT_ON_WINDOWS;
    box.insertAdjacentElement("afterend", note);
  }

  $<HTMLSpanElement>("#version").textContent = s.version;
}

function bind() {
  $<HTMLButtonElement>("#save-audio").onclick = async () => {
    try {
      await invoke("save_audio_devices", {
        mic: $<HTMLSelectElement>("#audio-mic").value,
        speaker: $<HTMLSelectElement>("#audio-speaker").value,
      });
      feedback("Áudio salvo.");
    } catch (e) {
      feedback(String(e), true);
    }
  };

  $<HTMLButtonElement>("#add-category").onclick = () =>
    $<HTMLTableSectionElement>("#categories tbody").appendChild(categoryRow());

  $<HTMLButtonElement>("#save-laris").onclick = async () => {
    try {
      await invoke("save_laris_settings", {
        updates: ditadoUpdates(platform, {
          key: $<HTMLSelectElement>("#dictate-key").value,
          handsFree: $<HTMLInputElement>("#hands-free").checked,
          sttModel: $<HTMLInputElement>("#stt-model").value,
          noise: $<HTMLSelectElement>("#noise").value,
          prompt: $<HTMLTextAreaElement>("#initial-prompt").value,
        }),
      });
      feedback("Ditado salvo.");
    } catch (e) {
      feedback(String(e), true);
    }
  };

  $<HTMLButtonElement>("#save-miris").onclick = async () => {
    const categories: [string, string][] = [];
    let incomplete = 0;
    for (const tr of $<HTMLTableSectionElement>("#categories tbody").rows) {
      const name = (tr.querySelector(".cat-name") as HTMLInputElement).value.trim();
      const folder = (tr.querySelector(".cat-folder") as HTMLInputElement).value.trim();
      if (name && folder) categories.push([name, folder]);
      else if (name || folder) incomplete += 1;
    }
    if (incomplete > 0) {
      feedback(`${incomplete} categoria(s) incompleta(s) - preencha nome e pasta.`, true);
      return;
    }
    try {
      await invoke("save_miris_config", {
        vaultRoot: $<HTMLInputElement>("#vault-root").value.trim(),
        llmCommand: $<HTMLTextAreaElement>("#llm-command").value
          .split("\n").map((l) => l.trim()).filter(Boolean),
        categories,
      });
      feedback("Reuniões salvas.");
    } catch (e) {
      feedback(String(e), true);
    }
  };

  $<HTMLButtonElement>("#save-orb").onclick = async () => {
    try {
      await invoke("save_theme", {
        updates: {
          archetype: $<HTMLSelectElement>("#orb-archetype").value,
          palette: $<HTMLSelectElement>("#orb-palette").value,
          position: $<HTMLSelectElement>("#orb-position").value,
          size: Number($<HTMLInputElement>("#orb-size").value),
        },
      });
      feedback("Tema aplicado.");
    } catch (e) {
      feedback(String(e), true);
    }
  };
}

load().then(bind).catch((e) => feedback(String(e), true));
