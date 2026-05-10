/**
 * Theme store.
 *
 * Three values:
 * - `light` — force light, ignore system.
 * - `dark`  — force dark, ignore system.
 * - `system` — follow `prefers-color-scheme` and re-evaluate on changes.
 *
 * Persisted in localStorage as `argos:theme`. Applied to `<html>` via
 * the `dark` class — Tailwind picks it up via `darkMode: "class"`.
 */

import { createSignal, createEffect } from 'solid-js';

import { loadJSON, saveJSON } from '../lib/persist';

export type Theme = 'light' | 'dark' | 'system';

const STORAGE_KEY = 'argos:theme';

const [theme, setThemeRaw] = createSignal<Theme>(loadJSON<Theme>(STORAGE_KEY, 'system'));

export { theme };

export function setTheme(t: Theme): void {
  setThemeRaw(t);
}

/** Resolve `system` to the current concrete value. */
export function effectiveTheme(): 'light' | 'dark' {
  const t = theme();
  if (t !== 'system') return t;
  if (typeof window === 'undefined') return 'light';
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function applyTheme() {
  if (typeof document === 'undefined') return;
  const eff = effectiveTheme();
  document.documentElement.classList.toggle('dark', eff === 'dark');
}

// Persist + re-apply whenever the user changes their preference.
createEffect(() => {
  const t = theme();
  saveJSON(STORAGE_KEY, t);
  applyTheme();
});

// Re-apply when the system preference flips (only matters in `system` mode).
if (typeof window !== 'undefined') {
  const mql = window.matchMedia('(prefers-color-scheme: dark)');
  mql.addEventListener('change', () => {
    if (theme() === 'system') applyTheme();
  });
}

/** Cycle light → dark → system → light. Used by the keyboard shortcut. */
export function cycleTheme(): void {
  const order: Theme[] = ['light', 'dark', 'system'];
  const idx = order.indexOf(theme());
  setTheme(order[(idx + 1) % order.length]!);
}
