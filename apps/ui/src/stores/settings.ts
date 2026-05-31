/**
 * User settings — appearance, editor preferences, keybinding overrides.
 *
 * Backed by `~/Library/Application Support/<bundle>/settings.json` via
 * `settings_load` / `settings_save` Tauri commands. In browser-only mode
 * (no Tauri shell) loads/saves no-op and we keep the in-memory defaults.
 *
 * Migration: on first boot we copy `argos:theme` from localStorage (the
 * pre-T8.2 storage) into settings so users don't lose their preference.
 */

import { batch, createEffect, createSignal } from 'solid-js';

import { settingsLoad, settingsSave } from '../lib/api';
import { clearKey, loadJSON } from '../lib/persist';

export type Theme = 'light' | 'dark' | 'system';
export type EditorThemeMode = 'follow-app' | 'one-dark';
export type ReleaseChannel = 'stable' | 'beta' | 'nightly';

export const RELEASE_CHANNELS: ReleaseChannel[] = ['stable', 'beta', 'nightly'];

export type Settings = {
  appearance: {
    theme: Theme;
  };
  editor: {
    fontSize: number; // px
    tabSize: number;
    lineWrapping: boolean;
    theme: EditorThemeMode;
  };
  updates: {
    /**
     * Release channel the auto-updater queries. Sent to argos-web as
     * the `X-Argos-Channel` header — `stable` (or missing) reads
     * the default manifest, `beta` / `nightly` read separate files.
     */
    channel: ReleaseChannel;
  };
  /**
   * actionId → combo string (e.g. "Mod+K"), or `null` to explicitly disable
   * the action. Keys missing here fall back to the action's default combo.
   */
  keybindings: Record<string, string | null>;
};

export const FONT_SIZE_MIN = 11;
export const FONT_SIZE_MAX = 20;
export const FONT_SIZE_DEFAULT = 13;
export const TAB_SIZES = [2, 4, 8] as const;

export const DEFAULT_SETTINGS: Settings = {
  appearance: { theme: 'system' },
  editor: {
    fontSize: FONT_SIZE_DEFAULT,
    tabSize: 2,
    lineWrapping: true,
    theme: 'follow-app',
  },
  updates: { channel: 'stable' },
  keybindings: {},
};

const [settings, setSettings] = createSignal<Settings>(DEFAULT_SETTINGS);
const [loaded, setLoaded] = createSignal(false);

export { settings, loaded as settingsLoaded };

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

// JSON-round-trip clone — settings are plain JSON, no Dates/Maps/etc.
// Avoids a global that ESLint doesn't recognise without env tweaks.
function cloneDefaults(): Settings {
  return JSON.parse(JSON.stringify(DEFAULT_SETTINGS)) as Settings;
}
function deepClone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T;
}

export function mergeWithDefaults(raw: unknown): Settings {
  if (!isObject(raw)) return DEFAULT_SETTINGS;
  const out = cloneDefaults();
  const appearance = raw.appearance;
  if (isObject(appearance)) {
    if (
      appearance.theme === 'light' ||
      appearance.theme === 'dark' ||
      appearance.theme === 'system'
    ) {
      out.appearance.theme = appearance.theme;
    }
  }
  const editor = raw.editor;
  if (isObject(editor)) {
    if (typeof editor.fontSize === 'number' && Number.isFinite(editor.fontSize)) {
      out.editor.fontSize = clamp(editor.fontSize, FONT_SIZE_MIN, FONT_SIZE_MAX);
    }
    if (typeof editor.tabSize === 'number' && (TAB_SIZES as readonly number[]).includes(editor.tabSize)) {
      out.editor.tabSize = editor.tabSize;
    }
    if (typeof editor.lineWrapping === 'boolean') {
      out.editor.lineWrapping = editor.lineWrapping;
    }
    if (editor.theme === 'follow-app' || editor.theme === 'one-dark') {
      out.editor.theme = editor.theme;
    }
  }
  const updates = raw.updates;
  if (isObject(updates)) {
    if (
      updates.channel === 'stable' ||
      updates.channel === 'beta' ||
      updates.channel === 'nightly'
    ) {
      out.updates.channel = updates.channel;
    }
  }
  const kb = raw.keybindings;
  if (isObject(kb)) {
    const cleaned: Record<string, string | null> = {};
    for (const [id, v] of Object.entries(kb)) {
      if (v === null || typeof v === 'string') cleaned[id] = v;
    }
    out.keybindings = cleaned;
  }
  return out;
}

function clamp(n: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, Math.round(n)));
}

/** Load settings from disk and migrate legacy localStorage values once. */
export async function initSettings(): Promise<void> {
  let raw: unknown = null;
  try {
    raw = await settingsLoad();
  } catch {
    // Browser mode or first-run pre-Tauri — keep defaults.
    setLoaded(true);
    return;
  }
  const merged = mergeWithDefaults(raw);

  // One-time migration of the pre-T8.2 theme + layout localStorage keys.
  const fileWasEmpty = !isObject(raw) || Object.keys(raw).length === 0;
  if (fileWasEmpty) {
    const legacyTheme = loadJSON<Theme | null>('argos:theme', null);
    if (legacyTheme === 'light' || legacyTheme === 'dark' || legacyTheme === 'system') {
      merged.appearance.theme = legacyTheme;
    }
    clearKey('argos:theme');
  }

  batch(() => {
    setSettings(merged);
    setLoaded(true);
  });
}

/**
 * Apply an update and persist. Mutator receives a deep clone so callers
 * can mutate freely; the returned value becomes the new settings.
 */
export function updateSettings(mut: (draft: Settings) => void | Settings): void {
  const draft = deepClone(settings());
  const next = mut(draft);
  setSettings(next ?? draft);
}

export function setTheme(t: Theme): void {
  updateSettings((s) => {
    s.appearance.theme = t;
  });
}

export function setEditorFontSize(px: number): void {
  updateSettings((s) => {
    s.editor.fontSize = clamp(px, FONT_SIZE_MIN, FONT_SIZE_MAX);
  });
}

export function setEditorTabSize(n: 2 | 4 | 8): void {
  updateSettings((s) => {
    s.editor.tabSize = n;
  });
}

export function setEditorLineWrapping(on: boolean): void {
  updateSettings((s) => {
    s.editor.lineWrapping = on;
  });
}

export function setEditorTheme(t: EditorThemeMode): void {
  updateSettings((s) => {
    s.editor.theme = t;
  });
}

export function setReleaseChannel(c: ReleaseChannel): void {
  updateSettings((s) => {
    s.updates.channel = c;
  });
}

export function setKeybinding(actionId: string, combo: string | null | undefined): void {
  updateSettings((s) => {
    if (combo === undefined) {
      delete s.keybindings[actionId];
    } else {
      s.keybindings[actionId] = combo;
    }
  });
}

export function resetAllKeybindings(): void {
  updateSettings((s) => {
    s.keybindings = {};
  });
}

export function replaceAllSettings(next: Settings): void {
  setSettings(next);
}

// Auto-persist on every change after the initial load. 200ms debounce so
// scrubbing a slider doesn't hammer the disk.
let saveTimer: ReturnType<typeof setTimeout> | null = null;
createEffect(() => {
  const s = settings();
  if (!loaded()) return;
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    void settingsSave(s).catch(() => {
      // Best-effort — UI keeps working from memory.
    });
  }, 200);
});
