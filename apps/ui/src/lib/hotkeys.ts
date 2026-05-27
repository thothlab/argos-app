/**
 * Global keyboard shortcut registration.
 *
 * Matches a single combo against `event.key` plus modifier flags.
 * Only one handler per combo (last bind wins) — call `unbind` for cleanup
 * via Solid's `onCleanup` to keep this from leaking when components unmount.
 *
 * `meta` is the macOS ⌘ key. On Windows / Linux we additionally accept the
 * Ctrl modifier so shortcuts behave naturally cross-platform without the
 * caller having to branch.
 */

import { onCleanup } from 'solid-js';

export type Combo = {
  key: string; // letter or "Enter" / "Escape" / "ArrowDown" — matches KeyboardEvent.key (case-insensitive)
  meta?: boolean; // ⌘ on macOS, Ctrl elsewhere (we treat them as equivalent for cross-platform sanity)
  shift?: boolean;
  alt?: boolean;
};

type Handler = (e: KeyboardEvent) => void;

const handlers: Array<{ combo: Combo; handler: Handler }> = [];
let installed = false;

function matches(e: KeyboardEvent, c: Combo): boolean {
  if (e.key.toLowerCase() !== c.key.toLowerCase()) return false;
  const cmdOrCtrl = e.metaKey || e.ctrlKey;
  if (!!c.meta !== cmdOrCtrl) return false;
  if (!!c.shift !== e.shiftKey) return false;
  if (!!c.alt !== e.altKey) return false;
  return true;
}

function ensureListener() {
  if (installed || typeof window === 'undefined') return;
  installed = true;
  window.addEventListener('keydown', (e) => {
    // Skip when an input/textarea/contenteditable is focused, unless the
    // combo uses meta (those are app-level shortcuts that should always fire).
    const target = e.target as HTMLElement | null;
    const inEditable =
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      target?.isContentEditable === true;
    for (const h of handlers) {
      if (!matches(e, h.combo)) continue;
      if (inEditable && !h.combo.meta) continue;
      e.preventDefault();
      h.handler(e);
      return;
    }
  });
}

/**
 * Bind a keyboard shortcut for the lifetime of the calling component.
 *
 * Use inside a component body — the binding is automatically removed via
 * `onCleanup` when the component unmounts.
 */
export function bind(combo: Combo, handler: Handler): void {
  ensureListener();
  const entry = { combo, handler };
  handlers.push(entry);
  onCleanup(() => {
    const i = handlers.indexOf(entry);
    if (i >= 0) handlers.splice(i, 1);
  });
}

/**
 * Pretty label for the combo, e.g. `⌘K`, `⌘⇧P`, `⌥Enter`.
 *
 * On non-mac platforms `⌘` is shown as `Ctrl`. We don't try to detect platform
 * server-side — the small visual mismatch in the few SSR-rendered frames is OK.
 */
export function label(combo: Combo): string {
  const isMac =
    typeof navigator !== 'undefined' &&
    /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent || '');
  let s = '';
  if (combo.meta) s += isMac ? '⌘' : 'Ctrl+';
  if (combo.alt) s += isMac ? '⌥' : 'Alt+';
  if (combo.shift) s += isMac ? '⇧' : 'Shift+';
  s += combo.key.length === 1 ? combo.key.toUpperCase() : combo.key;
  return s;
}

/**
 * Combo ↔ portable string ("Mod+Shift+K", "Mod+,", "Alt+Enter"). `Mod` stands
 * for ⌘/Ctrl. Used to persist user keybinding overrides in `settings.json`
 * — visual labels are intentionally not the storage format.
 */
export function comboToString(c: Combo): string {
  const parts: string[] = [];
  if (c.meta) parts.push('Mod');
  if (c.alt) parts.push('Alt');
  if (c.shift) parts.push('Shift');
  parts.push(c.key.length === 1 ? c.key.toUpperCase() : c.key);
  return parts.join('+');
}

export function parseCombo(s: string): Combo | null {
  const parts = s.split('+').map((p) => p.trim()).filter(Boolean);
  if (parts.length === 0) return null;
  const combo: Combo = { key: '' };
  for (let i = 0; i < parts.length - 1; i++) {
    const m = parts[i]!.toLowerCase();
    if (m === 'mod' || m === 'cmd' || m === 'ctrl') combo.meta = true;
    else if (m === 'shift') combo.shift = true;
    else if (m === 'alt' || m === 'option') combo.alt = true;
    else return null;
  }
  combo.key = parts[parts.length - 1]!;
  if (combo.key.length === 0) return null;
  return combo;
}

/**
 * Read a `keydown` event into a Combo. Returns `null` for modifier-only
 * events (so the keybinding capture UI can wait for a real key).
 */
export function eventToCombo(e: KeyboardEvent): Combo | null {
  const k = e.key;
  if (k === 'Meta' || k === 'Control' || k === 'Shift' || k === 'Alt' || k === 'AltGraph') {
    return null;
  }
  return {
    key: k,
    meta: e.metaKey || e.ctrlKey,
    shift: e.shiftKey,
    alt: e.altKey,
  };
}

export function comboEquals(a: Combo, b: Combo): boolean {
  return (
    a.key.toLowerCase() === b.key.toLowerCase() &&
    !!a.meta === !!b.meta &&
    !!a.shift === !!b.shift &&
    !!a.alt === !!b.alt
  );
}

export function comboMatchesEvent(e: KeyboardEvent, c: Combo): boolean {
  return matches(e, c);
}
