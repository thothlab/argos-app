/**
 * URL bar — method picker, URL input, Send button.
 *
 * `{{var}}` syntax highlighting is intentionally minimal in T1.3 — the input
 * is plain text. Hover-resolution against the active environment lands in
 * E3 alongside variable substitution.
 */

import { ChevronDown, Copy, Loader2, Send } from 'lucide-solid';
import { DropdownMenu } from '@kobalte/core/dropdown-menu';

import { requestToCode, sendRequest, type CodegenTarget } from '../lib/api';
import { bind, label } from '../lib/hotkeys';
import { applyFolderInheritance, findAncestors } from '../lib/inherit';
import { activeEnvVars } from '../stores/active-env';
import { activeTab, activeTabId } from '../stores/tabs';
import {
  getRequest,
  getResponse,
  setMethod,
  setResponse,
  setUrl,
  toWireRequest,
} from '../stores/request';
import { recordRun } from '../stores/runs';
import { workspace } from '../stores/workspace';
import type { HttpMethod } from '../types/http';

import MethodPicker from './MethodPicker';

const COPY_TARGETS: Array<{ id: CodegenTarget; label: string }> = [
  { id: 'curl', label: 'cURL' },
  { id: 'fetch-browser', label: 'JavaScript — browser fetch' },
  { id: 'fetch-node', label: 'JavaScript — Node fetch' },
  { id: 'python', label: 'Python — requests' },
  { id: 'go', label: 'Go — net/http' },
  { id: 'rust', label: 'Rust — reqwest' },
];

export default function UrlBar() {
  // ⌘Enter sends the active tab's request from anywhere in the app.
  bind({ key: 'Enter', meta: true }, () => sendActive());

  function sendActive() {
    const tabId = activeTabId();
    if (tabId === null) return;
    const draft = getRequest(tabId);
    if (!draft || !draft.url.trim()) return;

    const tab = activeTab();
    const ancestors = tab?.path
      ? findAncestors(tab.path, workspace()?.tree ?? null)
      : [];
    const merged = applyFolderInheritance(draft, ancestors);

    const wire = toWireRequest(merged);
    const env = activeEnvVars();
    setResponse(tabId, { status: 'loading', startedAt: Date.now() });
    const ws = workspace();
    const persist =
      ws && tab?.path ? { workspaceRoot: ws.root, requestPath: tab.path } : undefined;

    const preScript = merged.preRequest.trim().length > 0 ? merged.preRequest : null;
    const testsScript = merged.tests.trim().length > 0 ? merged.tests : null;

    sendRequest(wire, env, preScript, testsScript)
      .then((outcome) => {
        setResponse(tabId, {
          status: 'ok',
          response: outcome.response,
          tests: outcome.tests,
          preRequestLogs: outcome.pre_request_logs,
          testsLogs: outcome.tests_logs,
        });
        recordRun(tabId, wire, outcome.response, persist);
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

      <DropdownMenu>
        <DropdownMenu.Trigger
          class="ml-1 flex h-9 shrink-0 items-center gap-1 rounded-md px-1.5 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary disabled:opacity-50"
          disabled={!tabId()}
          title="Copy as code"
        >
          <Copy size={14} />
          <ChevronDown size={11} />
        </DropdownMenu.Trigger>
        <DropdownMenu.Portal>
          <DropdownMenu.Content class="z-50 min-w-56 overflow-hidden rounded-md border border-border bg-bg-card shadow-lg">
            {COPY_TARGETS.map((t) => (
              <DropdownMenu.Item
                class="cursor-pointer px-3 py-1.5 text-[12px] hover:bg-bg-secondary data-[highlighted]:bg-bg-secondary"
                onSelect={() => void copyAs(t.id, t.label)}
              >
                {t.label}
              </DropdownMenu.Item>
            ))}
          </DropdownMenu.Content>
        </DropdownMenu.Portal>
      </DropdownMenu>
    </div>
  );

  async function copyAs(target: CodegenTarget, label: string): Promise<void> {
    const id = tabId();
    if (!id) return;
    const d = draft();
    if (!d) return;
    const tab = activeTab();
    const ancestors = tab?.path ? findAncestors(tab.path, workspace()?.tree ?? null) : [];
    const merged = applyFolderInheritance(d, ancestors);
    const wire = toWireRequest(merged);
    try {
      const snippet = await requestToCode(wire, target, activeEnvVars());
      await navigator.clipboard.writeText(snippet);
    } catch (e) {
      window.alert(
        `Could not copy as ${label}: ${e instanceof Error ? e.message : String(e)}`,
      );
    }
  }
}
