/**
 * Per-tab WebSocket state — connection status + message log.
 *
 * The Tauri backend pushes events on `ws://event`; this store
 * subscribes once at startup and fans events out to the right tab by
 * `connection_id`. We use the tab id as the connection id so the
 * lookup is direct.
 */

import { createStore, produce } from 'solid-js/store';

import { isTauri } from '../lib/tauri';

export type WsMessage = {
  direction: 'incoming' | 'outgoing';
  body: string;
  timestampMs: number;
  /** Bytes for binary frames — set when the message body is empty. */
  binaryBytes?: number;
};

export type WsConnectionStatus = 'idle' | 'connecting' | 'open' | 'closed' | 'error';

export type WsState = {
  status: WsConnectionStatus;
  messages: WsMessage[];
  lastError: string | null;
  closeReason: string | null;
};

const initialState: WsState = {
  status: 'idle',
  messages: [],
  lastError: null,
  closeReason: null,
};

const [store, setStore] = createStore<Record<string, WsState>>({});

export function getWsState(connectionId: string): WsState {
  return store[connectionId] ?? initialState;
}

export function ensureWsState(connectionId: string): void {
  if (store[connectionId]) return;
  setStore(connectionId, { ...initialState, messages: [] });
}

export function setWsStatus(connectionId: string, status: WsConnectionStatus): void {
  ensureWsState(connectionId);
  setStore(connectionId, 'status', status);
}

export function appendWsMessage(connectionId: string, msg: WsMessage): void {
  ensureWsState(connectionId);
  setStore(
    connectionId,
    produce((s) => {
      s.messages.push(msg);
      // Bound the log to the last 1k entries so a chatty subscription
      // doesn't OOM the renderer.
      if (s.messages.length > 1000) {
        s.messages.splice(0, s.messages.length - 1000);
      }
    }),
  );
}

export function clearWsMessages(connectionId: string): void {
  ensureWsState(connectionId);
  setStore(connectionId, 'messages', []);
}

export function setWsError(connectionId: string, message: string): void {
  ensureWsState(connectionId);
  setStore(connectionId, { status: 'error', lastError: message });
}

export function setWsClosed(connectionId: string, reason: string | null): void {
  ensureWsState(connectionId);
  setStore(connectionId, { status: 'closed', closeReason: reason });
}

export function dropWsState(connectionId: string): void {
  setStore(
    produce((s) => {
      delete s[connectionId];
    }),
  );
}

// ---- event listener --------------------------------------------------------

type WsEventPayload =
  | { connection_id: string; kind: 'connected' }
  | {
      connection_id: string;
      kind: 'message';
      direction: 'incoming' | 'outgoing';
      body: string;
      timestamp_ms: string;
    }
  | {
      connection_id: string;
      kind: 'binary';
      direction: 'incoming' | 'outgoing';
      bytes: number;
      timestamp_ms: string;
    }
  | { connection_id: string; kind: 'closed'; code: number | null; reason: string }
  | { connection_id: string; kind: 'error'; message: string };

let installed = false;

/** Subscribe once at app startup. Idempotent. */
export async function installWsEventListener(): Promise<void> {
  if (installed || !isTauri()) return;
  installed = true;
  const { listen } = await import('@tauri-apps/api/event');
  await listen<WsEventPayload>('ws://event', (event) => {
    const p = event.payload;
    if (!p || !p.connection_id) return;
    switch (p.kind) {
      case 'connected':
        setWsStatus(p.connection_id, 'open');
        break;
      case 'message':
        appendWsMessage(p.connection_id, {
          direction: p.direction,
          body: p.body,
          timestampMs: Number(p.timestamp_ms),
        });
        break;
      case 'binary':
        appendWsMessage(p.connection_id, {
          direction: p.direction,
          body: `<binary frame: ${p.bytes} bytes>`,
          timestampMs: Number(p.timestamp_ms),
          binaryBytes: p.bytes,
        });
        break;
      case 'closed':
        setWsClosed(
          p.connection_id,
          p.reason || (p.code !== null ? `code ${p.code}` : null),
        );
        break;
      case 'error':
        setWsError(p.connection_id, p.message);
        break;
    }
  });
}
