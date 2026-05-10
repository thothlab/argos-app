/**
 * Per-tab request + response state.
 *
 * **Critical**: state is initialised explicitly via `initTabState()` when a
 * tab is created. Getters read only — they never mutate the store. Lazy
 * "create on first read" was abandoned because store mutations triggered
 * inside reactive reads caused stale Switch/Match selections in
 * `<ResponsePane>` (response from a different tab leaked across).
 *
 * The store keeps a *draft* shape (`DraftRequest`) that's friendlier to the
 * UI than the wire-format shape:
 * - headers / query are `RowEntry[]` so we can carry an `enabled` flag and
 *   surface it as a checkbox in the table.
 * - body is split into a `kind` + raw text content.
 *
 * `toWireRequest()` converts a draft to the `argos-core::HttpRequest` wire
 * shape immediately before IPC. Disabled entries are filtered out.
 */

import { createStore, produce } from 'solid-js/store';

import type { RowEntry } from '../components/KeyValueTable';
import type { HttpBody, HttpMethod, HttpRequest, HttpResponse } from '../types/http';

// ---- draft shape ----------------------------------------------------------

export type DraftBodyKind = 'none' | 'text' | 'json' | 'form';

export type DraftRequest = {
  method: HttpMethod;
  url: string;
  headers: RowEntry[];
  query: RowEntry[];
  bodyKind: DraftBodyKind;
  bodyText: string;
  bodyContentType: string;
};

export type ResponseState =
  | { status: 'idle' }
  | { status: 'loading'; startedAt: number }
  | { status: 'ok'; response: HttpResponse }
  | { status: 'error'; message: string };

type TabState = {
  request: DraftRequest;
  response: ResponseState;
};

type StoreShape = Record<string, TabState>;

export function emptyDraft(method: HttpMethod = 'GET'): DraftRequest {
  return {
    method,
    url: '',
    headers: [],
    query: [],
    bodyKind: 'none',
    bodyText: '',
    bodyContentType: 'application/json',
  };
}

const IDLE_RESPONSE: ResponseState = { status: 'idle' };

const [tabStates, setTabStatesRaw] = createStore<StoreShape>({});

// ---- lifecycle ------------------------------------------------------------

/**
 * Initialise state for a new tab. Idempotent — does nothing if state
 * already exists.
 */
export function initTabState(tabId: string, partial?: Partial<DraftRequest>): void {
  if (tabStates[tabId]) return;
  setTabStatesRaw(tabId, {
    request: { ...emptyDraft(), ...partial },
    response: IDLE_RESPONSE,
  });
}

/** Drop a tab's state. Called when a tab is closed. */
export function dropTabState(tabId: string): void {
  setTabStatesRaw(produce((s) => { delete s[tabId]; }));
}

// ---- selectors (pure reads, never mutate) ---------------------------------

export function getRequest(tabId: string): DraftRequest | null {
  return tabStates[tabId]?.request ?? null;
}

export function getResponse(tabId: string): ResponseState {
  return tabStates[tabId]?.response ?? IDLE_RESPONSE;
}

// ---- mutations ------------------------------------------------------------

export function setMethod(tabId: string, method: HttpMethod): void {
  if (!tabStates[tabId]) return;
  setTabStatesRaw(tabId, 'request', 'method', method);
}

/**
 * Set URL with auto-extraction of `?...` query params into the table.
 *
 * Mirrors Postman / Bruno behaviour: pasting `https://x.com/foo?a=1&b=2`
 * leaves `https://x.com/foo` in the URL field and adds `a=1, b=2` to the
 * query table. `enabled` flags from existing rows are preserved when the
 * key matches (so toggling stays sticky across re-types).
 */
export function setUrl(tabId: string, url: string): void {
  if (!tabStates[tabId]) return;

  const queryIdx = url.indexOf('?');
  if (queryIdx < 0) {
    setTabStatesRaw(tabId, 'request', 'url', url);
    return;
  }

  const baseUrl = url.slice(0, queryIdx);
  const queryStr = url.slice(queryIdx + 1);
  const extracted = parseQueryString(queryStr);
  const merged = mergeQueryRows(tabStates[tabId]!.request.query, extracted);

  setTabStatesRaw(
    tabId,
    'request',
    produce((r) => {
      r.url = baseUrl;
      r.query = merged;
    }),
  );
}

export function setHeaders(tabId: string, headers: RowEntry[]): void {
  if (!tabStates[tabId]) return;
  setTabStatesRaw(tabId, 'request', 'headers', headers);
}

export function setQuery(tabId: string, query: RowEntry[]): void {
  if (!tabStates[tabId]) return;
  setTabStatesRaw(tabId, 'request', 'query', query);
}

export function setBodyKind(tabId: string, kind: DraftBodyKind): void {
  if (!tabStates[tabId]) return;
  setTabStatesRaw(tabId, 'request', 'bodyKind', kind);
}

export function setBodyText(tabId: string, text: string): void {
  if (!tabStates[tabId]) return;
  setTabStatesRaw(tabId, 'request', 'bodyText', text);
}

export function setBodyContentType(tabId: string, ct: string): void {
  if (!tabStates[tabId]) return;
  setTabStatesRaw(tabId, 'request', 'bodyContentType', ct);
}

export function setResponse(tabId: string, response: ResponseState): void {
  if (!tabStates[tabId]) return;
  setTabStatesRaw(tabId, 'response', response);
}

// ---- wire conversion ------------------------------------------------------

/**
 * Convert a draft to the `argos-core::HttpRequest` wire shape.
 *
 * - Filters out disabled headers / query rows and rows with empty keys.
 * - Maps the body kind to the right `HttpBody` variant.
 *
 * Note: query params from the URL are kept in the URL string itself; we
 * pass them through verbatim. The `query` Vec on the wire side carries only
 * the table-defined params. This avoids the duplication that the engine's
 * `query_pairs_mut` would otherwise produce.
 */
export function toWireRequest(draft: DraftRequest): HttpRequest {
  return {
    method: draft.method,
    url: buildFullUrl(draft.url, draft.query),
    headers: draft.headers
      .filter((r) => r.enabled && r.name.length > 0)
      .map((r) => ({ name: r.name, value: r.value })),
    query: [],
    body: buildBody(draft),
    timeout: null,
  };
}

function buildFullUrl(baseUrl: string, query: RowEntry[]): string {
  const enabled = query.filter((r) => r.enabled && r.name.length > 0);
  if (enabled.length === 0) return baseUrl;
  const sep = baseUrl.includes('?') ? '&' : '?';
  const qs = enabled
    .map((r) => `${encodeURIComponent(r.name)}=${encodeURIComponent(r.value)}`)
    .join('&');
  return `${baseUrl}${sep}${qs}`;
}

function buildBody(draft: DraftRequest): HttpBody | null {
  if (draft.bodyKind === 'none') return null;
  const text = draft.bodyText;
  if (text.length === 0) return null;
  const contentType = draft.bodyContentType || 'text/plain';

  if (draft.bodyKind === 'form') {
    const fields: Array<[string, string]> = text
      .split('&')
      .filter(Boolean)
      .map((pair) => {
        const eq = pair.indexOf('=');
        return eq < 0
          ? [pair, '']
          : [decodeURIComponent(pair.slice(0, eq)), decodeURIComponent(pair.slice(eq + 1))];
      });
    return { kind: 'form_url_encoded', fields };
  }

  return { kind: 'text', content: text, content_type: contentType };
}

// ---- helpers --------------------------------------------------------------

function parseQueryString(qs: string): Array<{ name: string; value: string }> {
  if (qs.length === 0) return [];
  return qs.split('&').map((pair) => {
    const eq = pair.indexOf('=');
    if (eq < 0) {
      return { name: safeDecode(pair), value: '' };
    }
    return {
      name: safeDecode(pair.slice(0, eq)),
      value: safeDecode(pair.slice(eq + 1)),
    };
  });
}

function safeDecode(s: string): string {
  try {
    return decodeURIComponent(s.replace(/\+/g, ' '));
  } catch {
    return s;
  }
}

/**
 * Merge incoming query rows with the existing table, preserving `enabled`
 * for rows whose names match. New incoming rows are enabled by default.
 */
function mergeQueryRows(
  existing: RowEntry[],
  incoming: Array<{ name: string; value: string }>,
): RowEntry[] {
  return incoming.map((entry) => {
    const prev = existing.find((r) => r.name === entry.name);
    return {
      name: entry.name,
      value: entry.value,
      enabled: prev?.enabled ?? true,
    };
  });
}
