/**
 * Run history list — newest at the top, click to load that run's response
 * back into the response pane.
 *
 * Lives in the lower dock. Reads runs for the active tab; auto-refreshes
 * when new runs are recorded.
 */

import { For, Show } from 'solid-js';

import { setResponse } from '../stores/request';
import { runsFor, type Run } from '../stores/runs';
import { activeTabId } from '../stores/tabs';
import type { HttpMethod } from '../types/http';

const METHOD_VAR: Record<HttpMethod, string> = {
  GET: 'var(--method-get)',
  POST: 'var(--method-post)',
  PUT: 'var(--method-put)',
  PATCH: 'var(--method-patch)',
  DELETE: 'var(--method-delete)',
  HEAD: 'var(--fg-secondary)',
  OPTIONS: 'var(--fg-secondary)',
};

export default function RunHistoryView() {
  const list = (): Run[] => {
    const id = activeTabId();
    return id ? runsFor(id) : [];
  };

  function loadRun(run: Run): void {
    // Materialise a past run as the current response — the user can still
    // edit the request and re-send to get a fresh one.
    setResponse(run.tabId, { status: 'ok', response: run.response });
  }

  return (
    <Show
      when={list().length > 0}
      fallback={
        <div class="flex h-full items-center justify-center px-6 text-center">
          <p class="font-mono text-[12px] text-fg-secondary">
            No runs yet. Send a request to populate the history.
          </p>
        </div>
      }
    >
      <table class="w-full font-mono text-[12px]">
        <thead class="sticky top-0 bg-bg-card">
          <tr class="text-left text-[10px] uppercase tracking-widest text-fg-secondary">
            <th class="px-3 py-2 font-medium">Time</th>
            <th class="px-3 py-2 font-medium">Method</th>
            <th class="px-3 py-2 font-medium">URL</th>
            <th class="px-3 py-2 font-medium">Status</th>
            <th class="px-3 py-2 font-medium">Took</th>
            <th class="px-3 py-2 font-medium">Size</th>
          </tr>
        </thead>
        <tbody>
          <For each={list()}>
            {(run) => (
              <tr
                class="cursor-pointer border-t border-border hover:bg-bg-secondary"
                onClick={() => loadRun(run)}
                title="Load this run into the response pane"
              >
                <td class="px-3 py-1.5 text-fg-secondary">{formatTime(run.startedAt)}</td>
                <td
                  class="px-3 py-1.5 font-bold"
                  style={{ color: METHOD_VAR[run.request.method] }}
                >
                  {run.request.method}
                </td>
                <td class="max-w-md truncate px-3 py-1.5" title={run.request.url}>
                  {run.request.url || '—'}
                </td>
                <td
                  class="px-3 py-1.5 font-bold"
                  style={{ color: statusColor(run.response.status) }}
                >
                  {run.response.status}
                </td>
                <td class="px-3 py-1.5 text-fg-secondary">{run.response.timing.total_ms} ms</td>
                <td class="px-3 py-1.5 text-fg-secondary">{formatBytes(run.response.body.size_bytes)}</td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
    </Show>
  );
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}

function statusColor(s: number): string {
  if (s >= 500) return 'var(--color-error-foreground)';
  if (s >= 400) return 'var(--color-warning-foreground)';
  if (s >= 300) return 'var(--color-info-foreground)';
  return 'var(--color-success-foreground)';
}
