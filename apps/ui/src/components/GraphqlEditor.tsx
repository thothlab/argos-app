/**
 * GraphQL editor — URL bar + tabbed details (Query / Variables /
 * Headers / Auth / Scripts).
 *
 * Sends GraphQL as a regular POST through `send_request` with a JSON
 * body of `{ query, variables, operationName }`. Schema introspection
 * and autocomplete are deferred to chunk 4 — the goal here is "I can
 * send a GraphQL query and read the JSON response", parity with curl.
 */

import { Loader2, Send } from 'lucide-solid';
import { createSignal, For, Show } from 'solid-js';

import { sendRequest } from '../lib/api';
import { bind, label } from '../lib/hotkeys';
import { activeEnvVars } from '../stores/active-env';
import { activeTab, activeTabId } from '../stores/tabs';
import {
  getRequest,
  getResponse,
  setGraphqlOperationName,
  setGraphqlQuery,
  setGraphqlVariables,
  setHeaders,
  setPreRequest,
  setResponse,
  setTests,
  setUrl,
} from '../stores/request';
import { recordRun } from '../stores/runs';
import { workspace } from '../stores/workspace';
import type { HttpBody, HttpHeader, HttpRequest } from '../types/http';

import AuthTab from './AuthTab';
import CodeEditor from './CodeEditor';
import KeyValueTable, { type RowEntry } from './KeyValueTable';

type EditorTab = 'query' | 'variables' | 'headers' | 'auth' | 'scripts';

const TABS: Array<{ id: EditorTab; label: string }> = [
  { id: 'query', label: 'Query' },
  { id: 'variables', label: 'Variables' },
  { id: 'headers', label: 'Headers' },
  { id: 'auth', label: 'Auth' },
  { id: 'scripts', label: 'Scripts' },
];

export default function GraphqlEditor() {
  const [tab, setTab] = createSignal<EditorTab>('query');

  bind({ key: 'Enter', meta: true }, () => void sendActive());

  const tabId = () => activeTabId();
  const draft = () => {
    const id = tabId();
    return id ? getRequest(id) : null;
  };
  const isLoading = () => {
    const id = tabId();
    return !!id && getResponse(id).status === 'loading';
  };

  const variablesError = (): string | null => {
    const t = draft()?.graphql.variablesText.trim() ?? '';
    if (t.length === 0) return null;
    try {
      JSON.parse(t);
      return null;
    } catch (e) {
      return e instanceof Error ? e.message : String(e);
    }
  };

  async function sendActive(): Promise<void> {
    const id = tabId();
    if (id === null) return;
    const d = draft();
    if (!d || !d.url.trim()) return;

    // GraphQL is plain JSON over POST. Build the body here so the
    // Tauri send path stays REST-shaped.
    const variables: unknown =
      d.graphql.variablesText.trim().length > 0
        ? safeParse(d.graphql.variablesText)
        : undefined;

    const payload: Record<string, unknown> = { query: d.graphql.query };
    if (variables !== undefined) payload.variables = variables;
    if (d.graphql.operationName.trim().length > 0) {
      payload.operationName = d.graphql.operationName;
    }

    const headers: HttpHeader[] = d.headers
      .filter((h) => h.enabled && h.name.length > 0)
      .map((h) => ({ name: h.name, value: h.value }));
    // Make sure Content-Type is right even if the user didn't add it.
    if (!headers.some((h) => h.name.toLowerCase() === 'content-type')) {
      headers.push({ name: 'Content-Type', value: 'application/json' });
    }

    const body: HttpBody = { kind: 'json', value: payload };
    const wire: HttpRequest = {
      method: 'POST',
      url: d.url,
      query: [],
      headers,
      body,
      timeout: null,
    };

    setResponse(id, { status: 'loading', startedAt: Date.now() });
    const env = activeEnvVars();
    const ws = workspace();
    const t = activeTab();
    const persist =
      ws && t?.path ? { workspaceRoot: ws.root, requestPath: t.path } : undefined;
    const preScript = d.preRequest.trim().length > 0 ? d.preRequest : null;
    const testsScript = d.tests.trim().length > 0 ? d.tests : null;

    try {
      const outcome = await sendRequest(wire, env, preScript, testsScript);
      setResponse(id, {
        status: 'ok',
        response: outcome.response,
        tests: outcome.tests,
        preRequestLogs: outcome.pre_request_logs,
        testsLogs: outcome.tests_logs,
      });
      recordRun(id, wire, outcome.response, persist);
    } catch (e) {
      setResponse(id, { status: 'error', message: e instanceof Error ? e.message : String(e) });
    }
  }

  return (
    <div class="flex h-full w-full flex-col">
      {/* URL bar — method fixed to POST, but visible so the layout matches REST. */}
      <div class="flex h-12 shrink-0 items-center gap-0 px-3 py-2">
        <span class="flex h-9 shrink-0 items-center rounded-l-md border border-r-0 border-border bg-bg-secondary px-3 font-mono text-[11px] font-bold text-fg-secondary">
          GQL
        </span>
        <input
          type="text"
          spellcheck={false}
          autocomplete="off"
          class="h-9 min-w-0 flex-1 border-y border-border bg-bg-card px-3 font-mono text-[13px] outline-none focus:border-primary"
          placeholder="https://api.example.com/graphql"
          value={draft()?.url ?? ''}
          disabled={!tabId()}
          onInput={(e) => {
            const id = tabId();
            if (id) setUrl(id, e.currentTarget.value);
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.metaKey && !e.ctrlKey) {
              e.preventDefault();
              void sendActive();
            }
          }}
        />
        <button
          type="button"
          class="flex h-9 shrink-0 items-center gap-2 rounded-r-full bg-primary px-5 font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:opacity-50"
          disabled={!tabId() || isLoading()}
          title={`Send (${label({ key: 'Enter', meta: true })})`}
          onClick={() => void sendActive()}
        >
          {isLoading() ? <Loader2 size={14} class="animate-spin" /> : <Send size={14} />}
          <span class="text-[13px]">{isLoading() ? 'Sending…' : 'Send'}</span>
        </button>
      </div>

      <div class="flex h-9 shrink-0 items-end border-b border-border px-3">
        <For each={TABS}>
          {(t) => (
            <button
              type="button"
              class="relative px-3 py-2 text-[13px]"
              classList={{
                'text-fg-primary': tab() === t.id,
                'text-fg-secondary hover:text-fg-primary': tab() !== t.id,
              }}
              onClick={() => setTab(t.id)}
            >
              {t.label}
              <Show when={t.id === tab()}>
                <span class="absolute inset-x-2 -bottom-px h-0.5 bg-primary" />
              </Show>
            </button>
          )}
        </For>
      </div>

      <div class="min-h-0 flex-1 overflow-auto p-3">
        <Show when={tab() === 'query'}>
          <CodeEditor
            value={draft()?.graphql.query ?? ''}
            placeholder="query ListUsers($limit: Int) { users(limit: $limit) { id name } }"
            minHeight="280px"
            onChange={(v) => {
              const id = tabId();
              if (id) setGraphqlQuery(id, v);
            }}
          />
          <div class="mt-3 flex items-center gap-2 text-[12px]">
            <label class="text-fg-secondary">Operation name (optional):</label>
            <input
              type="text"
              spellcheck={false}
              class="h-7 min-w-0 flex-1 rounded border border-border bg-bg-card px-2 font-mono text-[12px] outline-none focus:border-primary"
              placeholder="ListUsers"
              value={draft()?.graphql.operationName ?? ''}
              onInput={(e) => {
                const id = tabId();
                if (id) setGraphqlOperationName(id, e.currentTarget.value);
              }}
            />
          </div>
        </Show>

        <Show when={tab() === 'variables'}>
          <CodeEditor
            value={draft()?.graphql.variablesText ?? ''}
            placeholder='{ "limit": 10 }'
            minHeight="240px"
            onChange={(v) => {
              const id = tabId();
              if (id) setGraphqlVariables(id, v);
            }}
          />
          <Show when={variablesError() !== null}>
            <p class="mt-2 text-[12px] text-fg-error">
              Variables JSON is invalid: {variablesError()} — saved as-is, but the request
              will fall back to sending the raw text.
            </p>
          </Show>
        </Show>

        <Show when={tab() === 'headers'}>
          <KeyValueTable
            rows={draft()?.headers ?? []}
            onChange={(rows: RowEntry[]) => {
              const id = tabId();
              if (id) setHeaders(id, rows);
            }}
          />
        </Show>

        <Show when={tab() === 'auth'}>
          <AuthTab />
        </Show>

        <Show when={tab() === 'scripts'}>
          <section class="flex flex-col gap-3">
            <header class="text-[12px] font-medium text-fg-secondary">Pre-request</header>
            <CodeEditor
              value={draft()?.preRequest ?? ''}
              minHeight="120px"
              onChange={(v) => {
                const id = tabId();
                if (id) setPreRequest(id, v);
              }}
            />
            <header class="text-[12px] font-medium text-fg-secondary">Tests</header>
            <CodeEditor
              value={draft()?.tests ?? ''}
              minHeight="120px"
              onChange={(v) => {
                const id = tabId();
                if (id) setTests(id, v);
              }}
            />
          </section>
        </Show>
      </div>
    </div>
  );
}

function safeParse(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}
