// Settings window rules per platform (plan windows-port AC 22, 24, 25, 91).
// Run: node --test apps/tray/tests/settings.test.ts
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  ditadoUpdates, keyChoice, LANGUAGE, languageNote, legacyControls, NOT_ON_WINDOWS,
} from "../src/settings-logic.ts";

const WINDOWS_KEYS = ["f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12", "f13", "f14", "f15"];

test("the key list is the platform's and preselects the key the daemon uses", () => {
  assert.equal(keyChoice(WINDOWS_KEYS, "f5", "windows").selected, "f5");
  assert.equal(keyChoice(WINDOWS_KEYS, "f15", "windows").selected, "f15");
  // nothing saved: the daemon's default
  assert.equal(keyChoice(WINDOWS_KEYS, undefined, "windows").selected, "f13");
  // unsupported on Windows: the daemon runs f13, so the window shows f13
  assert.equal(keyChoice(WINDOWS_KEYS, "shift_r", "windows").selected, "f13");
});

test("a saved unsupported key shows the F13 notice", () => {
  const c = keyChoice(WINDOWS_KEYS, "cmd_r", "windows");

  assert.equal(c.selected, "f13");
  assert.equal(c.notice, "Tecla cmd_r não é suportada no Windows; usando F13");
  assert.equal(keyChoice(WINDOWS_KEYS, "f13", "windows").notice, "");
});

test("legacy controls are disabled with the Windows notice", () => {
  const labels = legacyControls("windows").map((c) => c.label);

  assert.deepEqual(labels, ["Vault", "Comando LLM", "Categorias", "Modelo STT", "Redução de ruído"]);
  assert.equal(NOT_ON_WINDOWS, "Não disponível no Windows");
  assert.deepEqual(legacyControls("other"), []);
});

test("the Windows form leaves STT_MODEL and NOISE_REDUCTION out of the save", () => {
  const form = { key: "f13", handsFree: true, sttModel: " turbo ", noise: "on", prompt: " AXON " };

  const windows = ditadoUpdates("windows", form);
  const other = ditadoUpdates("other", form);

  assert.deepEqual(windows, { DICTATE_KEY: "f13", ASSISTANT_MODE: "always_on", INITIAL_PROMPT: "AXON" });
  assert.deepEqual(Object.keys(other).sort(), ["ASSISTANT_MODE", "DICTATE_KEY", "INITIAL_PROMPT", "NOISE_REDUCTION", "STT_MODEL"]);
});

test("the Windows settings show the fixed Portuguese language", () => {
  assert.equal(languageNote("windows"), "Idioma: Português (Brasil)");
  assert.equal(LANGUAGE, "Idioma: Português (Brasil)");
  assert.equal(languageNote("other"), "");
});
