/**
 * Request editor — URL bar + tabbed details (Params / Headers / Body / …).
 *
 * For T1.3 the live tabs are Params, Headers, Body. Auth and Tests are
 * shown but disabled so the layout is fixed; they wire up in E3 / E4.
 */

import { createSignal, For, Match, Show, Switch } from 'solid-js';

import { activeTabId } from '../stores/tabs';
import {
  getRequest,
  setBodyContentType,
  setBodyKind,
  setBodyText,
  setHeaders,
  setQuery,
  type DraftBodyKind,
} from '../stores/request';

import KeyValueTable, { type RowEntry } from './KeyValueTable';
import UrlBar from './UrlBar';

type EditorTab = 'params' | 'headers' | 'body' | 'auth' | 'tests';

const VISIBLE_TABS: Array<{ id: EditorTab; label: string; disabled?: boolean }> = [
  { id: 'params', label: 'Params' },
  { id: 'headers', label: 'Headers' },
  { id: 'body', label: 'Body' },
  { id: 'auth', label: 'Auth', disabled: true },
  { id: 'tests', label: 'Tests', disabled: true },
];

export default function RequestEditor() {
  const [activeEditorTab, setActiveEditorTab] = createSignal<EditorTab>('params');

  const tabId = () => activeTabId();
  const draft = () => {
    const id = tabId();
    return id ? getRequest(id) : null;
  };
  const headerCount = () => draft()?.headers.filter((h) => h.enabled && h.name).length ?? 0;
  const queryCount = () => draft()?.query.filter((q) => q.enabled && q.name).length ?? 0;
  const hasBody = () => {
    const d = draft();
    return d ? d.bodyKind !== 'none' && d.bodyText.length > 0 : false;
  };

  return (
    <div class="flex h-full w-full flex-col">
      <UrlBar />

      <div class="flex h-9 shrink-0 items-end border-b border-border px-3">
        <For each={VISIBLE_TABS}>
          {(t) => (
            <button
              type="button"
              class="relative flex items-center gap-1.5 px-3 py-2 text-[13px]"
              classList={{
                'text-fg-primary': activeEditorTab() === t.id,
                'text-fg-secondary hover:text-fg-primary': activeEditorTab() !== t.id && !t.disabled,
                'cursor-not-allowed text-fg-secondary opacity-50': !!t.disabled,
              }}
              disabled={t.disabled}
              onClick={() => !t.disabled && setActiveEditorTab(t.id)}
              title={t.disabled ? 'Available later (Auth: E3, Tests: E4)' : undefined}
            >
              <span>{t.label}</span>
              <Show when={t.id === 'params' && queryCount() > 0}>
                <Badge>{queryCount()}</Badge>
              </Show>
              <Show when={t.id === 'headers' && headerCount() > 0}>
                <Badge>{headerCount()}</Badge>
              </Show>
              <Show when={t.id === 'body' && hasBody()}>
                <span class="h-1.5 w-1.5 rounded-full bg-primary" aria-label="has body" />
              </Show>
              <Show when={activeEditorTab() === t.id}>
                <span class="absolute inset-x-3 -bottom-px h-0.5 bg-primary" aria-hidden />
              </Show>
            </button>
          )}
        </For>
      </div>

      <div class="flex-1 overflow-auto scrollbar-thin">
        <Switch>
          <Match when={activeEditorTab() === 'params'}>
            <ParamsTab />
          </Match>
          <Match when={activeEditorTab() === 'headers'}>
            <HeadersTab />
          </Match>
          <Match when={activeEditorTab() === 'body'}>
            <BodyTab />
          </Match>
        </Switch>
      </div>
    </div>
  );
}

function Badge(props: { children: number }) {
  return (
    <span class="rounded-full bg-bg-secondary px-1.5 font-mono text-[10px] text-fg-secondary">
      {props.children}
    </span>
  );
}

function ParamsTab() {
  const id = activeTabId;
  const rows = () => {
    const tabId = id();
    return tabId ? (getRequest(tabId)?.query ?? []) : [];
  };
  return (
    <div class="px-2 pb-4">
      <KeyValueTable
        rows={rows()}
        keyPlaceholder="Param"
        valuePlaceholder="Value"
        onChange={(r: RowEntry[]) => {
          const tabId = id();
          if (tabId) setQuery(tabId, r);
        }}
      />
      <p class="mt-3 px-3 text-[11px] text-fg-secondary">
        These params are appended to the URL on send. URLs in the bar above can
        already include their own query string — both are merged.
      </p>
    </div>
  );
}

function HeadersTab() {
  const id = activeTabId;
  const rows = () => {
    const tabId = id();
    return tabId ? (getRequest(tabId)?.headers ?? []) : [];
  };
  return (
    <div class="px-2 pb-4">
      <KeyValueTable
        rows={rows()}
        keyPlaceholder="Header"
        valuePlaceholder="Value"
        onChange={(r: RowEntry[]) => {
          const tabId = id();
          if (tabId) setHeaders(tabId, r);
        }}
      />
    </div>
  );
}

const BODY_KINDS: Array<{ id: DraftBodyKind; label: string }> = [
  { id: 'none', label: 'None' },
  { id: 'text', label: 'Text' },
  { id: 'json', label: 'JSON' },
  { id: 'form', label: 'Form-urlencoded' },
];

function BodyTab() {
  const id = activeTabId;
  const draft = () => {
    const tabId = id();
    return tabId ? getRequest(tabId) : null;
  };

  // Auto-fill content-type when switching kind, unless user has overridden it.
  function changeKind(kind: DraftBodyKind) {
    const tabId = id();
    if (!tabId) return;
    setBodyKind(tabId, kind);
    if (kind === 'json') setBodyContentType(tabId, 'application/json');
    else if (kind === 'text') setBodyContentType(tabId, 'text/plain');
    else if (kind === 'form') setBodyContentType(tabId, 'application/x-www-form-urlencoded');
  }

  return (
    <div class="flex h-full flex-col p-3">
      <div class="flex items-center gap-3 pb-2">
        <For each={BODY_KINDS}>
          {(k) => (
            <label class="flex cursor-pointer items-center gap-1.5 text-[13px]">
              <input
                type="radio"
                name="body-kind"
                class="accent-primary"
                checked={draft()?.bodyKind === k.id}
                onChange={() => changeKind(k.id)}
              />
              <span>{k.label}</span>
            </label>
          )}
        </For>

        <Show when={draft() && draft()!.bodyKind !== 'none' && draft()!.bodyKind !== 'form'}>
          <div class="ml-auto flex items-center gap-2 text-[12px]">
            <span class="text-fg-secondary">Content-Type:</span>
            <input
              type="text"
              spellcheck={false}
              autocomplete="off"
              class="w-56 rounded border border-border bg-bg-card px-2 py-1 font-mono outline-none focus:border-primary"
              value={draft()?.bodyContentType ?? ''}
              onInput={(e) => {
                const tabId = id();
                if (tabId) setBodyContentType(tabId, e.currentTarget.value);
              }}
            />
          </div>
        </Show>
      </div>

      <Show
        when={draft() && draft()!.bodyKind !== 'none'}
        fallback={
          <p class="mt-2 text-[12px] text-fg-secondary">
            No body. Switch to Text / JSON / Form to add one.
          </p>
        }
      >
        <textarea
          spellcheck={false}
          autocomplete="off"
          autocorrect="off"
          class="min-h-[200px] flex-1 resize-none rounded border border-border bg-bg-card p-3 font-mono text-[12px] outline-none focus:border-primary scrollbar-thin"
          placeholder={
            draft()?.bodyKind === 'json'
              ? '{\n  "key": "value"\n}'
              : draft()?.bodyKind === 'form'
                ? 'key1=value1&key2=value2'
                : 'Body content…'
          }
          value={draft()?.bodyText ?? ''}
          onInput={(e) => {
            const tabId = id();
            if (tabId) setBodyText(tabId, e.currentTarget.value);
          }}
        />
      </Show>
    </div>
  );
}
