/**
 * Save flow for the active tab.
 *
 * - Tab has a `path` → write directly to that file.
 * - Tab has no `path` (scratch tab) → ask the user where to save it
 *   inside the active workspace, then write + update the tab.
 *
 * After every successful write the workspace tree is reloaded so the
 * sidebar reflects new / renamed files. (We'll switch to file-watcher-
 * driven reload once T2.4 lands.)
 */

import { save as saveDialog } from '@tauri-apps/plugin-dialog';

import { requestSave, slug, workspaceReload } from './api';
import { isTauri } from './tauri';
import {
  activeTab,
  activeTabId,
  markDirty,
  setTabPath,
  tabAsDraft,
} from '../stores/tabs';
import { setWorkspace, workspace } from '../stores/workspace';

export type SaveOutcome =
  | { kind: 'saved'; path: string }
  | { kind: 'cancelled' }
  | { kind: 'no-tab' }
  | { kind: 'no-workspace' }
  | { kind: 'error'; message: string };

export async function saveActiveTab(): Promise<SaveOutcome> {
  const tabId = activeTabId();
  const t = activeTab();
  if (!tabId || !t) return { kind: 'no-tab' };

  const draft = tabAsDraft(tabId);
  if (!draft) return { kind: 'no-tab' };

  let path = t.path;

  if (!path) {
    const ws = workspace();
    if (!ws) return { kind: 'no-workspace' };
    if (!isTauri()) {
      return { kind: 'error', message: 'Save needs the Tauri shell.' };
    }

    const slugged = (await slug(t.title)) || 'untitled';
    const defaultPath = joinPath(ws.root, ws.manifest.config.collections_dir, `${slugged}.argos.yaml`);

    const picked = await saveDialog({
      title: `Save "${t.title}" as…`,
      defaultPath,
      filters: [{ name: 'Argos request', extensions: ['yaml', 'yml'] }],
    });
    if (typeof picked !== 'string') return { kind: 'cancelled' };
    path = ensureArgosExtension(picked);
  }

  try {
    await requestSave(path, draft);
    setTabPath(tabId, path);
    markDirty(tabId, false);

    // Refresh sidebar so a newly-created file shows up.
    const ws = workspace();
    if (ws) {
      try {
        const fresh = await workspaceReload(ws.root);
        setWorkspace(fresh);
      } catch {
        // Reload is best-effort — surface the save success regardless.
      }
    }

    return { kind: 'saved', path };
  } catch (e) {
    return { kind: 'error', message: String(e) };
  }
}

// ---- helpers --------------------------------------------------------------

function joinPath(...parts: string[]): string {
  return parts
    .map((p) => p.replace(/\/+$/, ''))
    .filter(Boolean)
    .join('/');
}

function ensureArgosExtension(p: string): string {
  if (p.endsWith('.argos.yaml')) return p;
  if (p.endsWith('.yaml')) return p.replace(/\.yaml$/, '.argos.yaml');
  if (p.endsWith('.yml')) return p.replace(/\.yml$/, '.argos.yaml');
  return `${p}.argos.yaml`;
}
