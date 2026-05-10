/**
 * Per-tab run history.
 *
 * A "run" is one full request → response cycle: the wire request that was
 * sent, the response received, the timing, and a `startedAt` timestamp.
 * Runs are appended on every successful response so users can scroll back
 * through "what did I send earlier in this session" and click any past run
 * to load its response back into the response pane.
 *
 * Runs for tabs backed by a workspace request file are also persisted to
 * `<workspace>/runs/<request-key>.json` (newest first, capped at 100).
 * Scratch tabs (no `path`) stay in-memory only.
 *
 * Cap: 100 runs per tab. Older runs are dropped FIFO.
 */

import { createStore, produce } from 'solid-js/store';
import { nanoid } from 'nanoid';

import { runClear as runClearRpc, runLoad, runRecord } from '../lib/api';
import { isTauri } from '../lib/tauri';
import type { HttpRequest, HttpResponse } from '../types/http';

export type Run = {
  id: string;
  /** Tab the run belongs to. */
  tabId: string;
  /** Wall-clock timestamp when send was initiated. */
  startedAt: number;
  /** Snapshot of the wire request sent. */
  request: HttpRequest;
  /** Server response. */
  response: HttpResponse;
};

const MAX_RUNS_PER_TAB = 100;

type StoreShape = Record<string, Run[]>;

const [runStore, setRunStore] = createStore<StoreShape>({});

export function runsFor(tabId: string): Run[] {
  return runStore[tabId] ?? [];
}

export function latestRun(tabId: string): Run | null {
  const list = runsFor(tabId);
  return list[0] ?? null;
}

export type PersistKey = { workspaceRoot: string; requestPath: string };

export function recordRun(
  tabId: string,
  request: HttpRequest,
  response: HttpResponse,
  persist?: PersistKey,
): Run {
  const run: Run = {
    id: nanoid(8),
    tabId,
    startedAt: Date.now(),
    request,
    response,
  };
  setRunStore(
    produce((s) => {
      const list = s[tabId] ?? [];
      list.unshift(run); // newest first
      if (list.length > MAX_RUNS_PER_TAB) list.length = MAX_RUNS_PER_TAB;
      s[tabId] = list;
    }),
  );
  if (persist && isTauri()) {
    void runRecord(persist.workspaceRoot, persist.requestPath, {
      id: run.id,
      started_at_ms: run.startedAt,
      request: run.request,
      response: run.response,
    });
  }
  return run;
}

/** Replace the in-memory list for a tab with on-disk runs, if any. */
export async function hydrateRuns(tabId: string, persist: PersistKey): Promise<void> {
  if (!isTauri()) return;
  try {
    const persisted = await runLoad(persist.workspaceRoot, persist.requestPath);
    const runs: Run[] = persisted.map((p) => ({
      id: p.id,
      tabId,
      startedAt: p.started_at_ms,
      request: p.request,
      response: p.response,
    }));
    setRunStore(
      produce((s) => {
        s[tabId] = runs;
      }),
    );
  } catch {
    // Best-effort hydrate; corrupt run files shouldn't block opening a tab.
  }
}

export function clearRuns(tabId: string, persist?: PersistKey): void {
  setRunStore(
    produce((s) => {
      delete s[tabId];
    }),
  );
  if (persist && isTauri()) {
    void runClearRpc(persist.workspaceRoot, persist.requestPath);
  }
}

export function findRun(tabId: string, runId: string): Run | null {
  return runsFor(tabId).find((r) => r.id === runId) ?? null;
}
