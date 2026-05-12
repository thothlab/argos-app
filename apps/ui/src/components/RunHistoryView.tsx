/**
 * Run history list — newest at the top, click to load that run's response
 * back into the response pane.
 *
 * Lives in the lower dock. Reads runs for the active tab; auto-refreshes
 * when new runs are recorded.
 */

import { Download, Trash2 } from 'lucide-solid';
import { For, Show } from 'solid-js';

import { runExportHar } from '../lib/api';
import { notify, notifyError } from '../lib/toast';
import { setResponse } from '../stores/request';
import { clearRuns, runsFor, type Run } from '../stores/runs';
import { activeTab, activeTabId } from '../stores/tabs';
import { workspace } from '../stores/workspace';
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

  function clearForActiveTab(): void {
    const id = activeTabId();
    if (!id) return;
    const count = runsFor(id).length;
    if (count === 0) return;
    const tab = activeTab();
    const ws = workspace();
    // If the tab is backed by a file, also drop the persisted runs
    // file on disk so the next launch starts fresh.
    const persist = ws && tab?.path
      ? { workspaceRoot: ws.root, requestPath: tab.path }
      : undefined;
    clearRuns(id, persist);
    notify.info(`Cleared ${count} run${count === 1 ? '' : 's'} from history.`);
  }

  return (
    <div class="flex h-full flex-col">
      <div class="flex shrink-0 items-center justify-between border-b border-border px-3 py-1.5 text-[11px] text-fg-secondary">
        <span>
          {list().length} run{list().length === 1 ? '' : 's'} for this tab
        </span>
        <button
          type="button"
          class="flex items-center gap-1 rounded px-2 py-1 hover:bg-bg-secondary hover:text-fg-primary disabled:opacity-30"
          disabled={list().length === 0}
          onClick={clearForActiveTab}
          title="Clear in-memory list and the on-disk runs file for this request"
        >
          <Trash2 size={11} />
          Clear
        </button>
      </div>
    <Show
      when={list().length > 0}
      fallback={
        <div class="flex flex-1 items-center justify-center px-6 text-center">
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
            <th class="w-8 px-2 py-2" />
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
                <td class="px-2 py-1.5">
                  <button
                    type="button"
                    class="rounded p-1 text-fg-secondary hover:bg-bg-card hover:text-fg-primary"
                    title="Export this run as HAR"
                    onClick={(e) => {
                      e.stopPropagation();
                      void exportRunAsHar(run);
                    }}
                  >
                    <Download size={12} />
                  </button>
                </td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
    </Show>
    </div>
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

async function exportRunAsHar(run: Run): Promise<void> {
  const iso = new Date(run.startedAt).toISOString();
  // Stamp the file with method + status so a folder of HARs is
  // self-documenting.
  const safeUrl = run.request.url
    .replace(/^https?:\/\//, '')
    .replace(/[^a-zA-Z0-9._-]+/g, '-')
    .slice(0, 60);
  const defaultName = `${run.request.method.toLowerCase()}-${safeUrl}-${run.response.status}.har`;

  let target: string | null = null;
  try {
    const { save } = await import('@tauri-apps/plugin-dialog');
    target = await save({
      defaultPath: defaultName,
      filters: [{ name: 'HAR 1.2', extensions: ['har'] }],
    });
  } catch (e) {
    notifyError('Could not open file picker', e);
    return;
  }
  if (!target) return;

  try {
    const written = await runExportHar(run.request, run.response, iso, target);
    notify.success('HAR export', `Saved to ${written}.`);
  } catch (e) {
    notifyError('HAR export failed', e);
  }
}
