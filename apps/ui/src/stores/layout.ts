/**
 * Layout state — sidebar visibility & width, lower-dock visibility.
 *
 * Sidebar width is clamped to [`SIDEBAR_MIN`, `SIDEBAR_MAX`] (per the
 * dimensions spelled out in `docs/06_designer_specification.md` §3.2).
 *
 * Persisted: width + the two visibility flags.
 */

import { createEffect, createSignal } from 'solid-js';

import { loadJSON, saveJSON } from '../lib/persist';

export const SIDEBAR_MIN = 240;
export const SIDEBAR_MAX = 480;
export const SIDEBAR_DEFAULT = 280;

const STATE_KEY = 'argos:layout';

type PersistedLayout = {
  sidebarVisible: boolean;
  sidebarWidth: number;
  dockVisible: boolean;
};

const initial = loadJSON<PersistedLayout>(STATE_KEY, {
  sidebarVisible: true,
  sidebarWidth: SIDEBAR_DEFAULT,
  dockVisible: false,
});

const [sidebarVisible, setSidebarVisible] = createSignal(initial.sidebarVisible);
const [sidebarWidth, setSidebarWidthRaw] = createSignal(clampWidth(initial.sidebarWidth));
const [dockVisible, setDockVisible] = createSignal(initial.dockVisible);

export { sidebarVisible, sidebarWidth, dockVisible };

export function setSidebarWidth(px: number): void {
  setSidebarWidthRaw(clampWidth(px));
}

export function toggleSidebar(): void {
  setSidebarVisible((v) => !v);
}

export function toggleDock(): void {
  setDockVisible((v) => !v);
}

function clampWidth(px: number): number {
  if (Number.isNaN(px)) return SIDEBAR_DEFAULT;
  return Math.max(SIDEBAR_MIN, Math.min(SIDEBAR_MAX, Math.round(px)));
}

createEffect(() => {
  saveJSON(STATE_KEY, {
    sidebarVisible: sidebarVisible(),
    sidebarWidth: sidebarWidth(),
    dockVisible: dockVisible(),
  } satisfies PersistedLayout);
});
