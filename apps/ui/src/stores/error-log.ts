/**
 * In-memory ring buffer of caught runtime errors.
 *
 * Populated by `installCrashCapture` in `lib/crashes.ts`. Surfaced
 * in the lower dock's Logs tab so the user can see what tripped
 * without digging through the pending-crash JSON files.
 *
 * This is the renderer-visible mirror of the crash queue — clearing
 * it doesn't drop the pending files on disk; that's intentional, so
 * we don't lose data the user might still want submitted.
 */

import { createSignal } from 'solid-js';

export type ErrorLogEntry = {
  /** Wall-clock ms — used both for display and to dedupe spam. */
  timestamp: number;
  /** "renderer:onerror" | "renderer:unhandledrejection" | "manual". */
  source: string;
  /** Single-line summary as it was sent / will be sent to the server. */
  message: string;
  /** Source file + line/col, or stack frame for promise rejections. */
  location: string;
};

const MAX_ENTRIES = 200;

const [entries, setEntries] = createSignal<ErrorLogEntry[]>([]);

export const errorLogEntries = entries;

/** Push a new entry. Older entries beyond `MAX_ENTRIES` fall off the
 *  back. Identical (source, message, location) tuples arriving within
 *  500 ms get collapsed — JS errors in render loops would otherwise
 *  fill the buffer in a heartbeat. */
export function pushErrorLog(entry: Omit<ErrorLogEntry, 'timestamp'>): void {
  const ts = Date.now();
  setEntries((prev) => {
    const last = prev[prev.length - 1];
    if (
      last &&
      ts - last.timestamp < 500 &&
      last.source === entry.source &&
      last.message === entry.message &&
      last.location === entry.location
    ) {
      // Same error firing in a tight loop — keep one entry, just bump
      // its timestamp so the user sees "still happening".
      const dedup = prev.slice();
      dedup[dedup.length - 1] = { ...last, timestamp: ts };
      return dedup;
    }
    const next = prev.concat({ ...entry, timestamp: ts });
    if (next.length > MAX_ENTRIES) {
      return next.slice(next.length - MAX_ENTRIES);
    }
    return next;
  });
}

export function clearErrorLog(): void {
  setEntries([]);
}
