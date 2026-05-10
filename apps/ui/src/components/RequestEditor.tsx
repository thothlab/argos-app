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
  setPreRequest,
  setQuery,
  setTests,
  type DraftBodyKind,
} from '../stores/request';
import { snippetsFor, type Snippet, type SnippetKind } from '../lib/snippets';

import AuthTab from './AuthTab';
import CodeEditor, { type CodeEditorRef } from './CodeEditor';
import KeyValueTable, { type RowEntry } from './KeyValueTable';
import UrlBar from './UrlBar';

type EditorTab = 'params' | 'headers' | 'body' | 'auth' | 'scripts';

const VISIBLE_TABS: Array<{ id: EditorTab; label: string; disabled?: boolean }> = [
  { id: 'params', label: 'Params' },
  { id: 'headers', label: 'Headers' },
  { id: 'body', label: 'Body' },
  { id: 'auth', label: 'Auth' },
  { id: 'scripts', label: 'Scripts' },
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
  const hasAuth = () => {
    const a = draft()?.auth;
    return !!a && a.kind !== 'none';
  };
  const hasScripts = () => {
    const d = draft();
    return !!d && (d.preRequest.trim().length > 0 || d.tests.trim().length > 0);
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
              <Show when={t.id === 'auth' && hasAuth()}>
                <span class="h-1.5 w-1.5 rounded-full bg-primary" aria-label="has auth" />
              </Show>
              <Show when={t.id === 'scripts' && hasScripts()}>
                <span class="h-1.5 w-1.5 rounded-full bg-primary" aria-label="has scripts" />
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
          <Match when={activeEditorTab() === 'auth'}>
            <AuthTab />
          </Match>
          <Match when={activeEditorTab() === 'scripts'}>
            <ScriptsTab />
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

function ScriptsTab() {
  const id = activeTabId;
  const draft = () => {
    const tabId = id();
    return tabId ? getRequest(tabId) : null;
  };
  return (
    <div class="flex h-full flex-col gap-4 p-4">
      <ScriptBlock
        title="Pre-request"
        kind="pre"
        hint="Runs in the Argos sandbox before the request leaves your machine. Use bru.req.* to mutate the wire request, bru.env.set/get for variables, bru.log/info/warn for output, bru.fail(msg) to abort."
        value={draft()?.preRequest ?? ''}
        onInput={(v) => {
          const tabId = id();
          if (tabId) setPreRequest(tabId, v);
        }}
      />
      <ScriptBlock
        title="Tests"
        kind="tests"
        hint="Runs after the response arrives. Use bru.test(name, fn) and bru.expect(value).toBe / .toEqual / .toBeTruthy / .toContain. Read response via bru.res.status / .body / .json() / .getHeader()."
        value={draft()?.tests ?? ''}
        onInput={(v) => {
          const tabId = id();
          if (tabId) setTests(tabId, v);
        }}
      />
    </div>
  );
}

function ScriptBlock(props: {
  title: string;
  kind: SnippetKind;
  hint: string;
  value: string;
  onInput: (v: string) => void;
}) {
  let editorRef: CodeEditorRef | null = null;

  function insertSnippet(s: Snippet) {
    if (editorRef) {
      const next = editorRef.insertAtCursor(s.body);
      props.onInput(next);
    } else {
      // Fallback (editor not yet mounted): append.
      const sep = props.value.length > 0 && !props.value.endsWith('\n') ? '\n' : '';
      props.onInput(props.value + sep + s.body);
    }
  }

  return (
    <div class="flex min-h-[200px] flex-1 flex-col">
      <div class="mb-1 flex items-baseline gap-2">
        <h3 class="text-[13px] font-medium text-fg-primary">{props.title}</h3>
        <span class="flex-1 text-[11px] text-fg-secondary">{props.hint}</span>
        <SnippetMenu kind={props.kind} onPick={insertSnippet} />
      </div>
      <CodeEditor
        value={props.value}
        onChange={props.onInput}
        ref={(api) => (editorRef = api)}
        minHeight="160px"
      />
    </div>
  );
}

function SnippetMenu(props: { kind: SnippetKind; onPick: (s: Snippet) => void }) {
  const [open, setOpen] = createSignal(false);
  const items = () => snippetsFor(props.kind);

  return (
    <div class="relative shrink-0">
      <button
        type="button"
        class="rounded border border-border bg-bg-card px-2 py-1 text-[11px] text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
        onClick={() => setOpen((v) => !v)}
        title="Insert snippet"
      >
        Snippets ▾
      </button>
      <Show when={open()}>
        <div
          class="absolute right-0 top-full z-20 mt-1 w-[280px] rounded border border-border bg-bg-card shadow-lg"
          onClick={(e) => e.stopPropagation()}
        >
          <div class="max-h-[280px] overflow-auto py-1 scrollbar-thin">
            <For each={items()}>
              {(s) => (
                <button
                  type="button"
                  class="block w-full px-3 py-1.5 text-left text-[12px] hover:bg-bg-secondary"
                  onClick={() => {
                    props.onPick(s);
                    setOpen(false);
                  }}
                >
                  <div class="font-medium text-fg-primary">{s.label}</div>
                  <div class="text-[11px] leading-snug text-fg-secondary">{s.description}</div>
                </button>
              )}
            </For>
          </div>
        </div>
      </Show>
    </div>
  );
}

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
