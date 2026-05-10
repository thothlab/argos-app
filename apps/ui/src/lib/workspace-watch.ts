/**
 * Listen for workspace-changed events from the Rust file watcher and refresh
 * the in-memory workspace tree.
 *
 * Tauri's event system is fire-and-forget — we get a stream of payloads
 * with the workspace root and the changed paths. We debounce slightly more
 * on top of the Rust-side debouncer to coalesce bursts (saves often touch
 * `.argos.tmp` first then rename), and only refetch the tree once.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { isTauri } from './tauri';
import { workspaceReload } from './api';
import { setWorkspace, workspace } from '../stores/workspace';

const EVENT = 'argos:workspace-changed';
const DEBOUNCE_MS = 250;

let unlisten: UnlistenFn | null = null;
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
let pending = false;

type Payload = {
  root: string;
  paths: string[];
};

/**
 * Install the Tauri event listener. Idempotent — calling twice is a no-op.
 */
export async function installWorkspaceWatch(): Promise<void> {
  if (!isTauri() || unlisten) return;
  unlisten = await listen<Payload>(EVENT, (e) => {
    const ws = workspace();
    if (!ws) return;
    // Ignore events from a workspace that's no longer active.
    if (normalise(e.payload.root) !== normalise(ws.root)) return;
    schedule(ws.root);
  });
}

/** Remove the listener. Called when leaving the desktop window. */
export async function uninstallWorkspaceWatch(): Promise<void> {
  if (debounceTimer) {
    clearTimeout(debounceTimer);
    debounceTimer = null;
  }
  if (unlisten) {
    unlisten();
    unlisten = null;
  }
}

function schedule(root: string): void {
  if (debounceTimer) {
    pending = true;
    return;
  }
  debounceTimer = setTimeout(async () => {
    debounceTimer = null;
    try {
      const fresh = await workspaceReload(root);
      setWorkspace(fresh);
    } catch {
      // The workspace might have been moved or deleted; bail silently.
    }
    if (pending) {
      pending = false;
      schedule(root);
    }
  }, DEBOUNCE_MS);
}

function normalise(p: string): string {
  return p.replace(/\/+$/, '');
}
