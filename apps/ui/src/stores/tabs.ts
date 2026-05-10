/**
 * Open-tab management.
 *
 * Each tab is a UI handle to a request being edited. The actual request
 * payload (method, URL, headers, body, response) lives in `stores/request.ts`
 * keyed by tab id — this keeps the tab strip light (just title + flags) and
 * avoids the temptation to duplicate state.
 *
 * For T1.3 the seed tabs hold mock metadata so the shell has something to
 * render. Real workspace-backed tabs land in E2.
 */

import { createSignal } from 'solid-js';
import { nanoid } from 'nanoid';

import { dropTabState, initTabState } from './request';
import { clearRuns } from './runs';
import type { HttpMethod } from '../types/http';

export type Tab = {
  /** Stable identifier, persisted across reorders. */
  id: string;
  /** Display name (request name from the workspace file). */
  title: string;
  /** True if the tab is pinned (survives "Close all unpinned"). */
  pinned: boolean;
  /** True if the tab has unsaved local edits. */
  dirty: boolean;
};

function makeTab(partial: { title: string; pinned?: boolean }): Tab {
  return {
    id: nanoid(8),
    title: partial.title,
    pinned: partial.pinned ?? false,
    dirty: false,
  };
}

// Seed tabs — initialised together with their request state so the method
// badge in the tab strip and the URL bar share one source of truth.
const SEEDED: Array<{ tab: Tab; method: HttpMethod }> = [
  { tab: makeTab({ title: 'List users' }), method: 'GET' },
  { tab: makeTab({ title: 'Create user' }), method: 'POST' },
];
for (const { tab, method } of SEEDED) {
  initTabState(tab.id, { method });
}

const SEED: Tab[] = SEEDED.map(({ tab }) => tab);

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
  dropTabState(id);
  clearRuns(id);

  if (activeTabId() === id) {
    const replacement = next[idx] ?? next[idx - 1] ?? null;
    setActiveTabId(replacement ? replacement.id : null);
  }
}

export function togglePin(id: string): void {
  setTabs((list) => list.map((t) => (t.id === id ? { ...t, pinned: !t.pinned } : t)));
}

export function openNewTab(template?: { title?: string; method?: HttpMethod }): void {
  const tab = makeTab({
    title: template?.title ?? 'Untitled',
    pinned: false,
  });
  initTabState(tab.id, { method: template?.method ?? 'GET' });
  setTabs((list) => [...list, tab]);
  setActiveTabId(tab.id);
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
