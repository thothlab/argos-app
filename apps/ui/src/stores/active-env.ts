/**
 * Active environment selection.
 *
 * Holds the *name* of the environment currently selected in the top-bar
 * picker; the actual variables are looked up from the workspace store on
 * demand. `null` means "no env active" — `{{var}}` placeholders pass
 * through unchanged.
 *
 * Persisted per workspace in localStorage so reopening a workspace restores
 * the selection.
 */

import { createEffect, createSignal } from 'solid-js';

import { loadJSON, saveJSON } from '../lib/persist';
import { workspace } from './workspace';

const STORAGE_KEY = 'argos:active-env';

type StoredMap = Record<string, string | null>;

const stored = loadJSON<StoredMap>(STORAGE_KEY, {});
const [activeEnvName, setActiveEnvNameRaw] = createSignal<string | null>(null);

export { activeEnvName };

export function setActiveEnvName(name: string | null): void {
  setActiveEnvNameRaw(name);
}

/** Resolve the active env to its `name → value` map for IPC. */
export function activeEnvVars(): Record<string, string> {
  const ws = workspace();
  const name = activeEnvName();
  if (!ws || !name) return {};
  const env = ws.environments.find((e) => e.name === name);
  if (!env) return {};
  const out: Record<string, string> = {};
  for (const v of env.variables) {
    if (v.enabled) out[v.name] = v.value;
  }
  // Secrets win over plain variables on key collision (matches Argos' policy).
  for (const v of env.secrets) {
    if (v.enabled) out[v.name] = v.value;
  }
  return out;
}

// When the workspace changes, restore the saved env selection for it (if any),
// else default to its config.default_environment, else null.
createEffect(() => {
  const ws = workspace();
  if (!ws) {
    setActiveEnvNameRaw(null);
    return;
  }
  const remembered = stored[ws.root];
  if (remembered && ws.environments.some((e) => e.name === remembered)) {
    setActiveEnvNameRaw(remembered);
    return;
  }
  const fallback =
    ws.manifest.config.default_environment &&
    ws.environments.some((e) => e.name === ws.manifest.config.default_environment)
      ? ws.manifest.config.default_environment
      : ws.environments[0]?.name ?? null;
  setActiveEnvNameRaw(fallback);
});

// Persist whenever the user changes the selection.
createEffect(() => {
  const ws = workspace();
  if (!ws) return;
  stored[ws.root] = activeEnvName();
  saveJSON(STORAGE_KEY, stored);
});
