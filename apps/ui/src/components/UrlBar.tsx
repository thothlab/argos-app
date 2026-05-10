/**
 * URL bar — method picker, URL input, Send button.
 *
 * `{{var}}` syntax highlighting is intentionally minimal in T1.3 — the input
 * is plain text. Hover-resolution against the active environment lands in
 * E3 alongside variable substitution.
 */

import { Loader2, Send } from 'lucide-solid';

import { sendRequest } from '../lib/api';
import { bind, label } from '../lib/hotkeys';
import { activeEnvVars } from '../stores/active-env';
import { activeTabId } from '../stores/tabs';
import {
  getRequest,
  getResponse,
  setMethod,
  setResponse,
  setUrl,
  toWireRequest,
} from '../stores/request';
import { recordRun } from '../stores/runs';
import type { HttpMethod } from '../types/http';

import MethodPicker from './MethodPicker';

export default function UrlBar() {
  // ⌘Enter sends the active tab's request from anywhere in the app.
  bind({ key: 'Enter', meta: true }, () => sendActive());

  function sendActive() {
    const tabId = activeTabId();
    if (tabId === null) return;
    const draft = getRequest(tabId);
    if (!draft || !draft.url.trim()) return;

    const wire = toWireRequest(draft);
    const env = activeEnvVars();
    setResponse(tabId, { status: 'loading', startedAt: Date.now() });
    sendRequest(wire, env)
      .then((response) => {
        setResponse(tabId, { status: 'ok', response });
        recordRun(tabId, wire, response);
      })
      .catch((e: unknown) => setResponse(tabId, { status: 'error', message: String(e) }));
  }

  const tabId = () => activeTabId();
  const draft = () => {
    const id = tabId();
    return id ? getRequest(id) : null;
  };
  const isLoading = () => {
    const id = tabId();
    if (!id) return false;
    return getResponse(id).status === 'loading';
  };

  // Display URL = base URL with the active query string spliced back in,
  // so users see the same URL they'd hit on send. Editing the input goes
  // through `setUrl()` which auto-extracts `?...` back into the table.
  const displayUrl = () => {
    const d = draft();
    if (!d) return '';
    const enabled = d.query.filter((r) => r.enabled && r.name.length > 0);
    if (enabled.length === 0) return d.url;
    const sep = d.url.includes('?') ? '&' : '?';
    const qs = enabled
      .map((r) => `${encodeURIComponent(r.name)}=${encodeURIComponent(r.value)}`)
      .join('&');
    return `${d.url}${sep}${qs}`;
  };

  return (
    <div class="flex h-12 shrink-0 items-center gap-0 px-3 py-2">
      <MethodPicker
        value={draft()?.method ?? 'GET'}
        onChange={(m: HttpMethod) => {
          const id = tabId();
          if (id) setMethod(id, m);
        }}
      />

      <input
        type="text"
        spellcheck={false}
        autocomplete="off"
        autocorrect="off"
        class="h-9 min-w-0 flex-1 border-y border-border bg-bg-card px-3 font-mono text-[13px] outline-none focus:border-primary"
        placeholder="https://api.example.com/users  —  use {{var}} for environment values"
        value={displayUrl()}
        disabled={!tabId()}
        onInput={(e) => {
          const id = tabId();
          if (id) setUrl(id, e.currentTarget.value);
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.metaKey && !e.ctrlKey) {
            e.preventDefault();
            sendActive();
          }
        }}
      />

      <button
        type="button"
        class="flex h-9 shrink-0 items-center gap-2 rounded-r-full bg-primary px-5 font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:opacity-50"
        disabled={!tabId() || isLoading()}
        title={`Send (${label({ key: 'Enter', meta: true })})`}
        onClick={sendActive}
      >
        {isLoading() ? <Loader2 size={14} class="animate-spin" /> : <Send size={14} />}
        <span class="text-[13px]">{isLoading() ? 'Sending…' : 'Send'}</span>
      </button>
    </div>
  );
}
