/**
 * Named action registry — the layer that makes shortcuts user-customisable.
 *
 * Modules call `defineAction({ id, defaultCombo, handler, label })` at app
 * boot. Each action then has:
 *   • a default combo shipped with the build
 *   • a possible override stored in `settings.json` under `keybindings[id]`
 *     (a string in `comboToString` format, or `null` to disable)
 *
 * A single global `keydown` listener walks the registry on every keystroke
 * and dispatches the first matching action. No bind/unbind cycle is needed
 * when the user remaps a key — the next event simply reads the new override.
 */

import type { Combo } from './hotkeys';
import { comboMatchesEvent, parseCombo } from './hotkeys';
import { settings } from '../stores/settings';

export type ActionId =
  | 'palette.toggle'
  | 'request.save'
  | 'sidebar.toggle'
  | 'dock.toggle'
  | 'theme.cycle'
  | 'settings.open';

export type ActionDef = {
  id: ActionId;
  label: string;
  defaultCombo: Combo;
  handler: () => void;
};

const registry = new Map<ActionId, ActionDef>();
let installed = false;

export function defineAction(def: ActionDef): void {
  registry.set(def.id, def);
}

export function listActions(): ActionDef[] {
  return Array.from(registry.values());
}

/**
 * Combo currently bound to `id` — user override if present, otherwise the
 * action's default. `null` means the user explicitly disabled it.
 */
export function effectiveCombo(id: ActionId): Combo | null {
  const def = registry.get(id);
  if (!def) return null;
  const override = settings().keybindings[id];
  if (override === null) return null; // explicitly disabled
  if (typeof override === 'string') {
    return parseCombo(override) ?? def.defaultCombo;
  }
  return def.defaultCombo;
}

/**
 * Find an action whose current binding (override or default) matches a
 * keyboard event. Skip `excludeId` so the conflict checker can ignore the
 * action currently being rebound.
 */
export function actionForEvent(
  e: KeyboardEvent,
  excludeId?: ActionId,
): ActionDef | null {
  for (const def of registry.values()) {
    if (excludeId && def.id === excludeId) continue;
    const combo = effectiveCombo(def.id);
    if (!combo) continue;
    if (comboMatchesEvent(e, combo)) return def;
  }
  return null;
}

/**
 * Find an action conflicting with the given combo (already-bound elsewhere).
 * Used when the user records a new combo in the settings panel.
 */
export function actionConflicting(
  combo: Combo,
  excludeId: ActionId,
): ActionDef | null {
  for (const def of registry.values()) {
    if (def.id === excludeId) continue;
    const existing = effectiveCombo(def.id);
    if (!existing) continue;
    if (
      existing.key.toLowerCase() === combo.key.toLowerCase() &&
      !!existing.meta === !!combo.meta &&
      !!existing.shift === !!combo.shift &&
      !!existing.alt === !!combo.alt
    ) {
      return def;
    }
  }
  return null;
}

/**
 * Install the global keydown listener. Idempotent — safe to call from the
 * app boot path even though component remounts may re-run it.
 */
export function installActionRouter(): void {
  if (installed || typeof window === 'undefined') return;
  installed = true;
  window.addEventListener('keydown', (e) => {
    const target = e.target as HTMLElement | null;
    const inEditable =
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      target?.isContentEditable === true;
    const def = actionForEvent(e);
    if (!def) return;
    const combo = effectiveCombo(def.id);
    // Skip while typing in inputs unless the combo includes Mod (⌘/Ctrl) —
    // those are app-level shortcuts that should always fire.
    if (inEditable && !combo?.meta) return;
    e.preventDefault();
    def.handler();
  });
}
