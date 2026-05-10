/**
 * Tiny localStorage wrapper.
 *
 * Used for UI preferences that should survive a reload but are otherwise
 * disposable (theme, sidebar width, last-active env, etc.). Anything important
 * lives in workspace files on disk, not here.
 */

export function loadJSON<T>(key: string, fallback: T): T {
  if (typeof window === 'undefined' || !window.localStorage) return fallback;
  try {
    const raw = window.localStorage.getItem(key);
    if (raw === null) return fallback;
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

export function saveJSON(key: string, value: unknown): void {
  if (typeof window === 'undefined' || !window.localStorage) return;
  try {
    window.localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Quota exceeded / private mode — silently ignore. Preferences are best-effort.
  }
}

export function clearKey(key: string): void {
  if (typeof window === 'undefined' || !window.localStorage) return;
  window.localStorage.removeItem(key);
}
