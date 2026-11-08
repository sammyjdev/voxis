// What the settings window shows and saves, per platform. Pure: main.ts
// applies it to the page; tests/settings.test.ts runs it under `node --test`.

export type Platform = "windows" | "other";

// mirrors laris/hotkey.py _ALLOWED (ctrl/alt are broken as global hotkeys on macOS)
export const ALL_DICTATE_KEYS = [
  "cmd_r", "cmd_l", "shift_r", "shift_l",
  "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12", "f13", "f14", "f15",
];

export const NOT_ON_WINDOWS = "Não disponível no Windows";
export const LANGUAGE = "Idioma: Português (Brasil)";

/** The key the select shows, and the notice for a saved key it cannot use. */
export function keyChoice(
  keys: readonly string[],
  saved: string | undefined,
  platform: Platform,
): { selected: string; notice: string } {
  // an unknown stored value would leave the select unselected and submit ""
  if (keys.includes(saved ?? "")) return { selected: saved as string, notice: "" };
  if (platform !== "windows") return { selected: "cmd_r", notice: "" };
  // the daemon runs f13 for a missing or unsupported key; the file is untouched
  return {
    selected: "f13",
    notice: saved ? `Tecla ${saved} não é suportada no Windows; usando F13` : "",
  };
}

export type DitadoForm = {
  key: string;
  handsFree: boolean;
  sttModel: string;
  noise: string;
  prompt: string;
};

/** The `save_laris_settings` updates the Ditado form sends. */
export function ditadoUpdates(platform: Platform, form: DitadoForm): Record<string, string> {
  const updates: Record<string, string> = {
    DICTATE_KEY: form.key,
    ASSISTANT_MODE: form.handsFree ? "always_on" : "ptt",
    INITIAL_PROMPT: form.prompt.trim(),
  };
  // Windows ignores both; leaving them out keeps the saved values as they are
  if (platform !== "windows") {
    updates.STT_MODEL = form.sttModel.trim();
    updates.NOISE_REDUCTION = form.noise;
  }
  return updates;
}

/** The controls the platform cannot apply: shown, disabled, with a note. */
export function legacyControls(platform: Platform): { id: string; label: string }[] {
  if (platform !== "windows") return [];
  return [
    { id: "vault-root", label: "Vault" },
    { id: "llm-command", label: "Comando LLM" },
    { id: "categories-box", label: "Categorias" },
    { id: "stt-model", label: "Modelo STT" },
    { id: "noise", label: "Redução de ruído" },
  ];
}

/** The fixed recognition language line, when the platform has one. */
export function languageNote(platform: Platform): string {
  return platform === "windows" ? LANGUAGE : "";
}
