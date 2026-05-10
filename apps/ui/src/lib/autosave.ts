/**
 * Per-tab debounced auto-save.
 *
 * When the active draft for a tab changes, mark the tab dirty and — if the
 * tab has a backing path — schedule a 1s debounced write to disk. Tabs
 * without a path stay dirty until the user runs Save (As) explicitly.
 *
 * The implementation hooks one effect per open tab. We watch `tabs()` to
 * install / dispose those effects as tabs come and go.
 */

import { createEffect, createRoot, untrack } from 'solid-js';

import { requestSave } from './api';
import { getRequest } from '../stores/request';
import { markDirty, tabAsDraft, tabs } from '../stores/tabs';

const DEBOUNCE_MS = 1000;

const disposers = new Map<string, () => void>();

/** Boot the autosave system. Call once at app start. */
export function installAutosave(): void {
  createEffect(() => {
    const ids = new Set(tabs().map((t) => t.id));
    // Tear down disposers for closed tabs.
    for (const [id, dispose] of disposers) {
      if (!ids.has(id)) {
        dispose();
        disposers.delete(id);
      }
    }
    // Install per-tab effects for new tabs.
    for (const id of ids) {
      if (!disposers.has(id)) {
        disposers.set(id, watchTab(id));
      }
    }
  });
}

function watchTab(tabId: string): () => void {
  let timer: number | null = null;
  let initialised = false;

  return createRoot((dispose) => {
    createEffect(() => {
      const r = getRequest(tabId);
      if (!r) return;

      // Track every relevant field so the effect re-runs on any edit.
      // Read flatly — Solid's store proxy auto-tracks reads.
      void r.method;
      void r.url;
      void r.bodyKind;
      void r.bodyText;
      void r.bodyContentType;
      for (const h of r.headers) {
        void h.name;
        void h.value;
        void h.enabled;
      }
      for (const q of r.query) {
        void q.name;
        void q.value;
        void q.enabled;
      }
      // Serialise the auth union so a kind change re-runs the effect.
      void JSON.stringify(r.auth);

      if (!initialised) {
        initialised = true;
        return;
      }

      // Schedule outside of the reactive read so we don't pull tabs() into
      // tracking — that would re-run the effect when title/dirty change too.
      untrack(() => {
        const tab = tabs().find((t) => t.id === tabId);
        if (!tab) return;
        markDirty(tabId, true);
        if (!tab.path) return;

        if (timer !== null) window.clearTimeout(timer);
        timer = window.setTimeout(() => {
          timer = null;
          void flush(tabId);
        }, DEBOUNCE_MS);
      });
    });

    return () => {
      if (timer !== null) {
        window.clearTimeout(timer);
        timer = null;
      }
      dispose();
    };
  });
}

async function flush(tabId: string): Promise<void> {
  const tab = tabs().find((t) => t.id === tabId);
  if (!tab?.path) return;
  const draft = tabAsDraft(tabId);
  if (!draft) return;
  try {
    await requestSave(tab.path, draft);
    markDirty(tabId, false);
  } catch {
    // Leave the tab dirty so a manual ⌘S can retry.
  }
}
