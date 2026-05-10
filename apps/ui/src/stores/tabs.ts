/**
 * Open-tab management.
 *
 * Tabs are *transient* — they describe what's currently open in the editor,
 * not what's saved on disk. The actual request files live in the workspace
 * (`workspace/collections/...`). A tab references a request by its file id.
 *
 * For T1.2 (UI shell) we model the state shape; T1.3 wires it to real
 * request files and Tauri IPC. Until then, tabs hold mock data so the shell
 * has something to render.
 */

import { createSignal } from 'solid-js';
import { nanoid } from 'nanoid';

export type HttpMethod =
  | 'GET'
  | 'POST'
  | 'PUT'
  | 'PATCH'
  | 'DELETE'
  | 'HEAD'
  | 'OPTIONS';

export type Tab = {
  /** Stable identifier, persisted across reorders. */
  id: string;
  /** Display name (request name from the workspace file). */
  title: string;
  /** REST verb — drives the colour-coded badge in the tab. */
  method: HttpMethod;
  /** True if the tab is pinned (survives "Close all unpinned"). */
  pinned: boolean;
  /** True if the tab has unsaved local edits. */
  dirty: boolean;
};

function makeTab(partial: Partial<Tab> & { title: string; method: HttpMethod }): Tab {
  return {
    id: nanoid(8),
    title: partial.title,
    method: partial.method,
    pinned: partial.pinned ?? false,
    dirty: partial.dirty ?? false,
  };
}

// Mock data for the empty shell — replaced by a real workspace loader in T1.3.
const SEED: Tab[] = [
  makeTab({ title: 'List users', method: 'GET' }),
  makeTab({ title: 'Create user', method: 'POST' }),
];

const [tabs, setTabs] = createSignal<Tab[]>(SEED);
const [activeTabId, setActiveTabId] = createSignal<string | null>(SEED[0]?.id ?? null);

export { tabs, activeTabId };

export function activeTab(): Tab | null {
  const id = activeTabId();
  if (id === null) return null;
  return tabs().find((t) => t.id === id) ?? null;
}

export function selectTab(id: string): void {
  if (tabs().some((t) => t.id === id)) {
    setActiveTabId(id);
  }
}

export function closeTab(id: string): void {
  const list = tabs();
  const idx = list.findIndex((t) => t.id === id);
  if (idx < 0) return;

  const next = list.filter((t) => t.id !== id);
  setTabs(next);

  if (activeTabId() === id) {
    // Pick a neighbour: prefer the tab to the right, fall back to the left.
    const replacement = next[idx] ?? next[idx - 1] ?? null;
    setActiveTabId(replacement ? replacement.id : null);
  }
}

export function togglePin(id: string): void {
  setTabs((list) => list.map((t) => (t.id === id ? { ...t, pinned: !t.pinned } : t)));
}

export function openNewTab(template?: Partial<Tab>): void {
  const t = makeTab({
    title: template?.title ?? 'Untitled',
    method: template?.method ?? 'GET',
    pinned: template?.pinned,
    dirty: true,
  });
  setTabs((list) => [...list, t]);
  setActiveTabId(t.id);
}

export function moveTab(id: string, toIndex: number): void {
  const list = tabs();
  const from = list.findIndex((t) => t.id === id);
  if (from < 0) return;
  const target = Math.max(0, Math.min(list.length - 1, toIndex));
  if (target === from) return;
  const reordered = list.slice();
  const [moved] = reordered.splice(from, 1);
  reordered.splice(target, 0, moved!);
  setTabs(reordered);
}
