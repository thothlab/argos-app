/**
 * WebSocket editor — connect to a `wss://` endpoint, send text frames,
 * watch the log of incoming messages.
 *
 * The connection id == the active tab id, so events from the Tauri
 * backend route directly into the per-tab store in `stores/ws.ts`.
 * Subprotocols, headers and auth come from the on-disk request
 * draft; chunk 4 will surface UI editors for them.
 */

import { ArrowDown, ArrowUp, Loader2, Plug, Power, Send, Trash2 } from 'lucide-solid';
import { createSignal, For, Show } from 'solid-js';

import { wsClose, wsConnect, wsSend } from '../lib/api';
import { activeEnvVars } from '../stores/active-env';
import { activeTabId } from '../stores/tabs';
import { getRequest, setUrl } from '../stores/request';
import {
  clearWsMessages,
  ensureWsState,
  getWsState,
  setWsError,
  setWsStatus,
} from '../stores/ws';

export default function WebsocketEditor() {
  const [draftMessage, setDraftMessage] = createSignal('');

  const tabId = () => activeTabId();
  const draft = () => {
    const id = tabId();
    return id ? getRequest(id) : null;
  };
  const wsState = () => {
    const id = tabId();
    return id ? getWsState(id) : null;
  };

  async function onConnect() {
    const id = tabId();
    const d = draft();
    if (!id || !d || !d.url.trim()) return;
    ensureWsState(id);
    setWsStatus(id, 'connecting');
    const headers: Array<[string, string]> = d.headers
      .filter((h) => h.enabled && h.name.length > 0)
      .map((h) => [h.name, h.value]);
    try {
      await wsConnect(id, d.url, [], headers, activeEnvVars());
    } catch (e) {
      setWsError(id, e instanceof Error ? e.message : String(e));
    }
  }

  async function onDisconnect() {
    const id = tabId();
    if (!id) return;
    try {
      await wsClose(id);
    } catch {
      /* idempotent */
    }
  }

  async function onSend(text: string) {
    const id = tabId();
    if (!id || !text) return;
    try {
      await wsSend(id, text);
      // Outgoing echo also comes back from the backend, but we
      // optimistically render so the input doesn't appear to do
      // nothing while the round-trip resolves.
      setDraftMessage('');
    } catch (e) {
      setWsError(id, e instanceof Error ? e.message : String(e));
    }
  }

  const statusLabel = () => {
    const s = wsState();
    if (!s) return 'idle';
    return s.status;
  };

  const statusColour = () => {
    switch (statusLabel()) {
      case 'open':
        return 'text-fg-success';
      case 'connecting':
        return 'text-fg-secondary';
      case 'error':
        return 'text-fg-error';
      case 'closed':
        return 'text-fg-secondary';
      default:
        return 'text-fg-secondary';
    }
  };

  return (
    <div class="flex h-full w-full flex-col">
      <div class="flex h-12 shrink-0 items-center gap-2 px-3 py-2">
        <span class="flex h-9 shrink-0 items-center rounded-l-md border border-r-0 border-border bg-bg-secondary px-3 font-mono text-[11px] font-bold text-fg-secondary">
          WS
        </span>
        <input
          type="text"
          spellcheck={false}
          autocomplete="off"
          class="h-9 min-w-0 flex-1 border-y border-border bg-bg-card px-3 font-mono text-[13px] outline-none focus:border-primary"
          placeholder="wss://echo.example.com/socket"
          value={draft()?.url ?? ''}
          disabled={!tabId() || statusLabel() === 'open' || statusLabel() === 'connecting'}
          onInput={(e) => {
            const id = tabId();
            if (id) setUrl(id, e.currentTarget.value);
          }}
        />
        <Show when={statusLabel() === 'open' || statusLabel() === 'connecting'}>
          <button
            type="button"
            class="flex h-9 items-center gap-2 rounded-r-md bg-bg-secondary px-4 text-[13px] font-medium hover:bg-bg-tertiary"
            onClick={() => void onDisconnect()}
            title="Disconnect"
          >
            <Power size={14} />
            Disconnect
          </button>
        </Show>
        <Show when={statusLabel() !== 'open' && statusLabel() !== 'connecting'}>
          <button
            type="button"
            class="flex h-9 items-center gap-2 rounded-r-md bg-primary px-4 text-[13px] font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
            disabled={!tabId() || !(draft()?.url.trim())}
            onClick={() => void onConnect()}
          >
            <Plug size={14} />
            Connect
          </button>
        </Show>
      </div>

      <div class="flex items-center gap-3 border-b border-border px-3 py-1.5 text-[12px]">
        <span class={`font-mono uppercase tracking-wider ${statusColour()}`}>
          {statusLabel() === 'connecting' && (
            <Loader2 size={11} class="mr-1 inline animate-spin" />
          )}
          {statusLabel()}
        </span>
        <Show when={wsState()?.lastError}>
          <span class="text-fg-error">{wsState()?.lastError}</span>
        </Show>
        <Show when={wsState()?.closeReason && statusLabel() === 'closed'}>
          <span class="text-fg-secondary">closed: {wsState()?.closeReason}</span>
        </Show>
        <div class="flex-1" />
        <button
          type="button"
          class="flex items-center gap-1 rounded px-2 py-1 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
          onClick={() => {
            const id = tabId();
            if (id) clearWsMessages(id);
          }}
          title="Clear log"
        >
          <Trash2 size={12} />
          Clear
        </button>
      </div>

      <div class="min-h-0 flex-1 overflow-auto font-mono text-[12px]">
        <For each={wsState()?.messages ?? []}>
          {(msg) => <MessageRow msg={msg} />}
        </For>
        <Show when={(wsState()?.messages.length ?? 0) === 0}>
          <p class="px-3 py-6 text-center text-fg-secondary">
            Connect to a WebSocket endpoint, then send a message to populate the log.
          </p>
        </Show>
      </div>

      <div class="flex shrink-0 items-stretch gap-2 border-t border-border p-2">
        <textarea
          rows="2"
          spellcheck={false}
          class="min-w-0 flex-1 resize-none rounded border border-border bg-bg-card px-2 py-1.5 font-mono text-[12px] outline-none focus:border-primary disabled:opacity-50"
          placeholder='Type a message — JSON is fine, e.g. {"type":"ping"}'
          value={draftMessage()}
          disabled={statusLabel() !== 'open'}
          onInput={(e) => setDraftMessage(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              void onSend(draftMessage());
            }
          }}
        />
        <button
          type="button"
          class="flex shrink-0 items-center gap-2 rounded bg-primary px-4 text-[13px] font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
          disabled={statusLabel() !== 'open' || !draftMessage().trim()}
          onClick={() => void onSend(draftMessage())}
          title="Send (⌘⏎)"
        >
          <Send size={14} />
          Send
        </button>
      </div>
    </div>
  );
}

function MessageRow(props: {
  msg: {
    direction: 'incoming' | 'outgoing';
    body: string;
    timestampMs: number;
    binaryBytes?: number;
  };
}) {
  const isIn = props.msg.direction === 'incoming';
  const Icon = isIn ? ArrowDown : ArrowUp;
  const ts = new Date(props.msg.timestampMs).toLocaleTimeString([], {
    hour12: false,
  });
  return (
    <div class="flex items-start gap-3 border-b border-border px-3 py-1.5">
      <span class={isIn ? 'text-fg-success' : 'text-primary'}>
        <Icon size={12} />
      </span>
      <span class="shrink-0 font-mono text-[10px] text-fg-secondary">{ts}</span>
      <pre class="min-w-0 flex-1 overflow-x-auto whitespace-pre-wrap break-all">{props.msg.body}</pre>
    </div>
  );
}
