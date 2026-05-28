/**
 * Crash reports inspector — modal listing locally-archived crash reports
 * that were submitted to the server. Master / detail layout: list on the
 * left, full JSON of the selected report on the right.
 *
 * Opens from the "View" action on the post-submit toast, from
 * Settings → Advanced, and any other entry point that wants to show the
 * user what was actually sent.
 */

import { createEffect, createResource, For, Match, onCleanup, Show, Switch, createSignal } from 'solid-js';
import { FolderOpen, RefreshCw, X } from 'lucide-solid';

import { crashLogOpen, closeCrashLog } from '../stores/crash-log-panel';
import { crashListSubmitted, crashRevealDir, type SubmittedCrashEntry } from '../lib/api';
import { isTauri } from '../lib/tauri';
import { notifyError } from '../lib/toast';

export default function CrashReportsPanel() {
  const [reloadToken, setReloadToken] = createSignal(0);
  const [entries] = createResource<SubmittedCrashEntry[], number>(
    () => (crashLogOpen() ? reloadToken() : -1),
    async (token) => {
      if (token < 0) return [];
      if (!isTauri()) return [];
      try {
        return await crashListSubmitted();
      } catch (e) {
        notifyError('Could not load crash reports', e);
        return [];
      }
    },
  );

  const [selectedPath, setSelectedPath] = createSignal<string | null>(null);
  const selected = (): SubmittedCrashEntry | null => {
    const list = entries() ?? [];
    const path = selectedPath();
    if (path) {
      const match = list.find((e) => e.path === path);
      if (match) return match;
    }
    return list[0] ?? null;
  };

  // Auto-select the first entry whenever the list reloads.
  createEffect(() => {
    const list = entries() ?? [];
    if (list.length === 0) {
      setSelectedPath(null);
      return;
    }
    if (!selectedPath() || !list.some((e) => e.path === selectedPath())) {
      setSelectedPath(list[0]!.path);
    }
  });

  function onKey(e: KeyboardEvent) {
    if (!crashLogOpen()) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      closeCrashLog();
    }
  }
  createEffect(() => {
    if (crashLogOpen()) window.addEventListener('keydown', onKey);
  });
  onCleanup(() => window.removeEventListener('keydown', onKey));

  async function reveal() {
    try {
      await crashRevealDir();
    } catch (e) {
      notifyError('Could not open folder', e);
    }
  }

  return (
    <Show when={crashLogOpen()}>
      <div
        class="fixed inset-0 z-50 flex items-center justify-center bg-bg-primary/70"
        role="dialog"
        aria-modal="true"
        aria-labelledby="crash-log-title"
        onClick={(e) => {
          if (e.target === e.currentTarget) closeCrashLog();
        }}
      >
        <div class="flex h-[600px] w-[860px] flex-col overflow-hidden rounded-xl border border-border bg-bg-card shadow-xl">
          <header class="flex items-center justify-between border-b border-border px-4 py-3">
            <div>
              <h2 id="crash-log-title" class="text-[14px] font-semibold">
                Crash reports
              </h2>
              <p class="text-[11px] text-fg-secondary">
                Local archive of reports submitted to the server. Sent automatically only
                if you opted into "Submit always".
              </p>
            </div>
            <div class="flex items-center gap-1">
              <button
                type="button"
                class="flex items-center gap-1.5 rounded px-2 py-1 text-[12px] text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
                onClick={() => setReloadToken((n) => n + 1)}
                title="Refresh"
              >
                <RefreshCw size={12} />
                Refresh
              </button>
              <button
                type="button"
                class="flex items-center gap-1.5 rounded px-2 py-1 text-[12px] text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
                onClick={() => void reveal()}
                title="Reveal folder"
              >
                <FolderOpen size={12} />
                Show in Finder
              </button>
              <button
                type="button"
                class="rounded p-1 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
                onClick={() => closeCrashLog()}
                aria-label="Close"
              >
                <X size={14} />
              </button>
            </div>
          </header>

          <div class="flex min-h-0 flex-1">
            <nav class="flex w-72 shrink-0 flex-col overflow-y-auto border-r border-border bg-bg-secondary/40 scrollbar-thin">
              <Switch>
                <Match when={entries.loading}>
                  <p class="p-4 text-[12px] text-fg-secondary">Loading…</p>
                </Match>
                <Match when={(entries()?.length ?? 0) === 0}>
                  <p class="p-4 text-[12px] text-fg-secondary">
                    No submitted reports yet. They appear here after Argos sends a crash
                    report — either automatically (with "Submit always") or after you
                    pick "Submit" on the consent prompt.
                  </p>
                </Match>
                <Match when={(entries()?.length ?? 0) > 0}>
                  <ul class="flex flex-col">
                    <For each={entries() ?? []}>
                      {(entry) => (
                        <li>
                          <button
                            type="button"
                            class="block w-full border-b border-border/60 px-3 py-2 text-left hover:bg-bg-secondary"
                            classList={{
                              'bg-bg-secondary': selectedPath() === entry.path,
                            }}
                            onClick={() => setSelectedPath(entry.path)}
                          >
                            <div class="truncate text-[12px] font-medium text-fg-primary">
                              {entry.report.panic.message || '(no message)'}
                            </div>
                            <div class="mt-0.5 truncate font-mono text-[10px] text-fg-secondary">
                              {entry.report.panic.location}
                            </div>
                            <div class="mt-0.5 flex items-center justify-between text-[10px] text-fg-secondary">
                              <span>{formatTs(entry.report.ts)}</span>
                              <span class="font-mono">v{entry.report.app_version}</span>
                            </div>
                          </button>
                        </li>
                      )}
                    </For>
                  </ul>
                </Match>
              </Switch>
            </nav>

            <section class="min-w-0 flex-1 overflow-y-auto p-5 scrollbar-thin">
              <Show
                when={selected()}
                fallback={
                  <p class="text-[12px] text-fg-secondary">
                    Select a report on the left to see its full contents.
                  </p>
                }
              >
                {(entry) => <Details entry={entry()} />}
              </Show>
            </section>
          </div>
        </div>
      </div>
    </Show>
  );
}

function Details(props: { entry: SubmittedCrashEntry }) {
  const report = () => props.entry.report;
  return (
    <div class="flex flex-col gap-4">
      <Field label="When">
        <span class="font-mono">{formatTs(report().ts)}</span>
        <span class="ml-2 text-fg-secondary">({report().ts})</span>
      </Field>
      <Field label="Argos version">
        <span class="font-mono">{report().app_version}</span>
      </Field>
      <Field label="OS">
        <span class="font-mono">{report().os}</span>
      </Field>
      <Field label="Panic message">
        <pre class="whitespace-pre-wrap break-words rounded border border-border bg-bg-secondary/50 p-2 font-mono text-[11px]">
          {report().panic.message}
        </pre>
      </Field>
      <Field label="Location">
        <span class="font-mono">{report().panic.location}</span>
      </Field>
      <Show when={report().panic.backtrace}>
        <Field label="Backtrace">
          <pre class="max-h-72 overflow-auto whitespace-pre rounded border border-border bg-bg-secondary/50 p-2 font-mono text-[10px] leading-snug scrollbar-thin">
            {report().panic.backtrace}
          </pre>
        </Field>
      </Show>
      <Field label="Session id">
        <Show
          when={report().session_id}
          fallback={<span class="text-fg-secondary">— (anonymous one-shot)</span>}
        >
          <span class="font-mono">{report().session_id}</span>
        </Show>
      </Field>
      <Field label="Schema">
        <span class="font-mono">{report().schema}</span>
      </Field>
      <Field label="On disk">
        <span class="break-all font-mono text-[11px] text-fg-secondary">{props.entry.path}</span>
      </Field>
    </div>
  );
}

function Field(props: { label: string; children: ReturnType<typeof Number> | unknown }) {
  return (
    <div class="flex flex-col gap-1">
      <div class="font-mono text-[10px] uppercase tracking-widest text-fg-secondary">
        {props.label}
      </div>
      <div class="text-[12px] text-fg-primary">{props.children as unknown as never}</div>
    </div>
  );
}

function formatTs(rfc3339: string): string {
  // The Rust side writes UTC. Format in the user's locale so people don't
  // have to mentally convert. Falls back to raw string if parsing fails.
  const t = Date.parse(rfc3339);
  if (Number.isNaN(t)) return rfc3339;
  const d = new Date(t);
  return d.toLocaleString(undefined, {
    year: 'numeric',
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}
