/**
 * Open-tab management.
 *
 * Each tab is a UI handle to a request being edited. The actual request
 * payload (method, URL, headers, body, response) lives in `stores/request.ts`
 * keyed by tab id.
 *
 * Tabs that came from a workspace tree carry the absolute YAML `path` of
 * their backing file. Tabs created via `+` start unsaved (`path: null`)
 * and resolve to a real path when the user picks one through Save As (T2.5
 * follow-up).
 */

import { createSignal } from 'solid-js';
import { nanoid } from 'nanoid';

import { dropTabState, getRequest, initTabState, patchRequest, type DraftRequest } from './request';
import { clearRuns, hydrateRuns } from './runs';
import { workspace } from './workspace';
import type { HttpMethod, HttpRequest } from '../types/http';
import type { RequestDraft } from '../types/workspace';

export type Tab = {
  /** Stable identifier, persisted across reorders. */
  id: string;
  /** Display name (request name from the workspace file). */
  title: string;
  /** Absolute path of the backing YAML file. `null` for unsaved tabs. */
  path: string | null;
  /** True if the tab is pinned (survives "Close all unpinned"). */
  pinned: boolean;
  /** True if the tab has unsaved local edits. */
  dirty: boolean;
};

function makeTab(partial: { title: string; path?: string | null; pinned?: boolean }): Tab {
  return {
    id: nanoid(8),
    title: partial.title,
    path: partial.path ?? null,
    pinned: partial.pinned ?? false,
    dirty: false,
  };
}

const [tabs, setTabs] = createSignal<Tab[]>([]);
const [activeTabId, setActiveTabId] = createSignal<string | null>(null);

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

export function closeAllTabs(): void {
  for (const t of tabs()) {
    dropTabState(t.id);
    clearRuns(t.id);
  }
  setTabs([]);
  setActiveTabId(null);
}

export function togglePin(id: string): void {
  setTabs((list) => list.map((t) => (t.id === id ? { ...t, pinned: !t.pinned } : t)));
}

export function markDirty(id: string, dirty: boolean): void {
  setTabs((list) => list.map((t) => (t.id === id ? { ...t, dirty } : t)));
}

export function setTabPath(id: string, path: string): void {
  setTabs((list) => list.map((t) => (t.id === id ? { ...t, path } : t)));
}

export function setTabTitle(id: string, title: string): void {
  setTabs((list) => list.map((t) => (t.id === id ? { ...t, title } : t)));
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

/**
 * Open a fresh, unsaved tab pre-filled from a wire-shape `HttpRequest`.
 * Used by import flows (cURL, Postman, …). The tab is marked dirty so
 * the user knows they need to save or rename before it sticks.
 */
export function openTabFromHttpRequest(req: HttpRequest, title = 'Imported'): void {
  const tab = makeTab({ title, pinned: false });
  initTabState(tab.id, { method: req.method });
  patchRequest(tab.id, (r) => {
    r.method = req.method;
    r.url = req.url;
    r.headers = req.headers.map((h) => ({
      name: h.name,
      value: h.value,
      enabled: true,
    }));
    r.query = req.query.map(([name, value]) => ({ name, value, enabled: true }));
    if (!req.body) {
      r.bodyKind = 'none';
      r.bodyText = '';
    } else if (req.body.kind === 'json') {
      r.bodyKind = 'json';
      r.bodyContentType = 'application/json';
      r.bodyText = JSON.stringify(req.body.value, null, 2);
    } else if (req.body.kind === 'text') {
      r.bodyKind = 'text';
      r.bodyContentType = req.body.content_type;
      r.bodyText = req.body.content;
    } else if (req.body.kind === 'form_url_encoded') {
      r.bodyKind = 'form';
      r.bodyContentType = 'application/x-www-form-urlencoded';
      r.bodyText = req.body.fields
        .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
        .join('&');
    }
  });
  setTabs((list) => [...list, { ...tab, dirty: true }]);
  setActiveTabId(tab.id);
}

/**
 * Open a tab for the given request file, or focus it if already open.
 *
 * Hydrates the request store from the on-disk draft so the editor shows
 * the persisted state. Subsequent edits stay in-memory until the user
 * saves (T2.5).
 */
export function openOrFocusTabForRequest(path: string, draft: RequestDraft): void {
  const existing = tabs().find((t) => t.path === path);
  if (existing) {
    setActiveTabId(existing.id);
    return;
  }
  const tab = makeTab({ title: draft.name, path });

  // Initial draft → store. We map the on-disk draft to our editor draft.
  initTabState(tab.id);

  // Pull values out of the on-disk RestRequest into the editor draft via
  // patchRequest (we already have the tab state, ensure() is a no-op now).
  patchRequest(tab.id, (r) => {
    if (draft.type !== 'rest') return;
    r.method = draft.method;
    r.url = draft.url;
    r.headers = draft.headers.map((h) => ({ name: h.name, value: h.value, enabled: h.enabled }));
    r.query = draft.query.map((q) => ({ name: q.name, value: q.value, enabled: q.enabled }));

    // auth — map on-disk AuthConfig to DraftAuth.
    if (!draft.auth) {
      r.auth = { kind: 'none' };
    } else if (draft.auth.type === 'inherit') {
      r.auth = { kind: 'inherit' };
    } else if (draft.auth.type === 'bearer') {
      r.auth = { kind: 'bearer', token: draft.auth.token };
    } else if (draft.auth.type === 'basic') {
      r.auth = { kind: 'basic', username: draft.auth.username, password: draft.auth.password };
    } else if (draft.auth.type === 'api_key') {
      r.auth = {
        kind: 'api_key',
        name: draft.auth.name,
        value: draft.auth.value,
        location: draft.auth.location,
      };
    }

    r.preRequest = draft.scripts?.pre_request ?? '';
    r.tests = draft.scripts?.tests ?? '';

    if (!draft.body) {
      r.bodyKind = 'none';
      r.bodyText = '';
      return;
    }
    if (draft.body.kind === 'json') {
      r.bodyKind = 'json';
      r.bodyContentType = 'application/json';
      r.bodyText = JSON.stringify(draft.body.value, null, 2);
    } else if (draft.body.kind === 'text') {
      r.bodyKind = 'text';
      r.bodyContentType = draft.body.content_type;
      r.bodyText = draft.body.content;
    } else if (draft.body.kind === 'form_url_encoded') {
      r.bodyKind = 'form';
      r.bodyContentType = 'application/x-www-form-urlencoded';
      r.bodyText = draft.body.fields
        .map((f) => `${encodeURIComponent(f.name)}=${encodeURIComponent(f.value)}`)
        .join('&');
    }
  });

  setTabs((list) => [...list, tab]);
  setActiveTabId(tab.id);

  // Pull persisted runs (if the workspace is loaded) into the in-memory
  // history. Best-effort — if the runs file is missing or corrupt the
  // tab still opens cleanly.
  const ws = workspace();
  if (ws) {
    void hydrateRuns(tab.id, { workspaceRoot: ws.root, requestPath: path });
  }
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

/**
 * Build a `RequestDraft` (YAML wire shape) from the active editor draft.
 *
 * Used by the save flow. Returns `null` if the tab id has no request state.
 */
export function tabAsDraft(tabId: string): RequestDraft | null {
  const r = getRequest(tabId);
  const t = tabs().find((x) => x.id === tabId);
  if (!r || !t) return null;
  return {
    kind: 'request',
    name: t.title,
    description: null,
    type: 'rest',
    method: r.method,
    url: r.url,
    headers: r.headers
      .filter((h) => h.name.length > 0)
      .map((h) => ({ name: h.name, value: h.value, enabled: h.enabled })),
    query: r.query
      .filter((q) => q.name.length > 0)
      .map((q) => ({ name: q.name, value: q.value, enabled: q.enabled })),
    auth: buildAuth(r),
    body: buildBody(r),
    scripts: {
      pre_request: r.preRequest.trim().length > 0 ? r.preRequest : null,
      tests: r.tests.trim().length > 0 ? r.tests : null,
    },
    schema_ref: null,
  };
}

function buildAuth(r: DraftRequest): RequestDraft['auth'] {
  switch (r.auth.kind) {
    case 'none':
      return null;
    case 'inherit':
      return { type: 'inherit' };
    case 'bearer':
      return { type: 'bearer', token: r.auth.token };
    case 'basic':
      return { type: 'basic', username: r.auth.username, password: r.auth.password };
    case 'api_key':
      return {
        type: 'api_key',
        name: r.auth.name,
        value: r.auth.value,
        location: r.auth.location,
      };
  }
}

function buildBody(r: DraftRequest | null): RequestDraft['body'] {
  if (!r) return null;
  if (r.bodyKind === 'none' || !r.bodyText) return null;
  if (r.bodyKind === 'json') {
    try {
      return { kind: 'json', value: JSON.parse(r.bodyText) };
    } catch {
      return { kind: 'text', content: r.bodyText, content_type: r.bodyContentType };
    }
  }
  if (r.bodyKind === 'form') {
    const fields = r.bodyText
      .split('&')
      .filter(Boolean)
      .map((pair) => {
        const eq = pair.indexOf('=');
        const name = eq < 0 ? pair : pair.slice(0, eq);
        const value = eq < 0 ? '' : pair.slice(eq + 1);
        try {
          return { name: decodeURIComponent(name), value: decodeURIComponent(value), enabled: true };
        } catch {
          return { name, value, enabled: true };
        }
      });
    return { kind: 'form_url_encoded', fields };
  }
  return { kind: 'text', content: r.bodyText, content_type: r.bodyContentType };
}
