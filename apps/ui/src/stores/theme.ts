/**
 * Theme — facade over the settings store.
 *
 * Three values: `light` / `dark` / `system`. Persisted via `settings.json`
 * (see [[settings.ts]]); this module owns the application side-effects:
 * toggling the `dark` class on `<html>` and re-evaluating when the OS
 * appearance flips while in `system` mode.
 */

import { createEffect, createSignal } from 'solid-js';

import { settings, setTheme as setThemeInSettings } from './settings';

export type Theme = 'light' | 'dark' | 'system';

export function theme(): Theme {
  return settings().appearance.theme;
}

export function setTheme(t: Theme): void {
  setThemeInSettings(t);
}

// OS appearance — kept as a signal so consumers (CodeEditor, etc.)
// re-render when the user flips system-wide light/dark while in `system` mode.
const [osDark, setOsDark] = createSignal(
  typeof window !== 'undefined' &&
    window.matchMedia('(prefers-color-scheme: dark)').matches,
);

if (typeof window !== 'undefined') {
  const mql = window.matchMedia('(prefers-color-scheme: dark)');
  mql.addEventListener('change', (e) => setOsDark(e.matches));
}

/** Resolve `system` to the current concrete value. Reactive in `system` mode. */
export function effectiveTheme(): 'light' | 'dark' {
  const t = theme();
  if (t !== 'system') return t;
  return osDark() ? 'dark' : 'light';
}

function applyTheme() {
  if (typeof document === 'undefined') return;
  document.documentElement.classList.toggle('dark', effectiveTheme() === 'dark');
}

// Re-apply whenever the effective theme changes — handles both the stored
// preference (settings.appearance.theme) and OS flips in `system` mode.
createEffect(() => {
  void effectiveTheme();
  applyTheme();
});

/** Cycle light → dark → system → light. Used by the keyboard shortcut. */
export function cycleTheme(): void {
  const order: Theme[] = ['light', 'dark', 'system'];
  const idx = order.indexOf(theme());
  setTheme(order[(idx + 1) % order.length]!);
}
