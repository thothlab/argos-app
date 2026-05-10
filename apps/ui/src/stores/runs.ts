/**
 * Per-tab run history.
 *
 * A "run" is one full request → response cycle: the wire request that was
 * sent, the response received, the timing, and a `startedAt` timestamp.
 * Runs are appended on every successful response so users can scroll back
 * through "what did I send earlier in this session" and click any past run
 * to load its response back into the response pane.
 *
 * v0.1 — **in-memory only**. Disk persistence (`<workspace>/runs/<request-id>/<ts>.json`)
 * lands in E2 once the workspace concept exists. We keep the API surface
 * stable so the move to disk is a localised change.
 *
 * Cap: 100 runs per tab. Older runs are dropped FIFO. The cap matches the
 * default retention from the BRD; configurable via settings in T1.5
 * follow-up.
 */

import { createStore, produce } from 'solid-js/store';
import { nanoid } from 'nanoid';

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

export function recordRun(tabId: string, request: HttpRequest, response: HttpResponse): Run {
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
  return run;
}

export function clearRuns(tabId: string): void {
  setRunStore(
    produce((s) => {
      delete s[tabId];
    }),
  );
}

export function findRun(tabId: string, runId: string): Run | null {
  return runsFor(tabId).find((r) => r.id === runId) ?? null;
}
