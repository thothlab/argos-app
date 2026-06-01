/**
 * Modal that takes a pasted log file and asks the configured AI
 * provider to extract HTTP requests from it. The user reviews the
 * extracted list, deselects any noise, then writes the rest into the
 * workspace via [[aiImportExtracted]].
 *
 * Argos never proxies — the log + the user's API key go straight from
 * the desktop process to the configured provider. The privacy notice
 * above the textarea is dynamic on the destination domain so users
 * see where their paste is about to land.
 */

import { createMemo, createSignal, For, Show } from 'solid-js';
import { Dialog } from '@kobalte/core/dialog';

import {
  AI_MAX_LOG_BYTES,
  aiExtractLog,
  aiImportExtracted,
  workspaceReload,
  type AiExtractedRequest,
} from '../lib/api';
import { notify, notifyError } from '../lib/toast';
import { settings } from '../stores/settings';
import { setWorkspace, workspace } from '../stores/workspace';
import { openSettings } from '../stores/settings-panel';

type Phase =
  | { kind: 'paste' }
  | { kind: 'extracting' }
  | { kind: 'review'; results: AiExtractedRequest[]; selected: boolean[]; raw: string }
  | { kind: 'importing' };

export default function LogImportModal(props: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
}) {
  const [logText, setLogText] = createSignal('');
  const [phase, setPhase] = createSignal<Phase>({ kind: 'paste' });
  const [error, setError] = createSignal<string | null>(null);

  const ai = () => settings().ai;
  const enabled = () => ai().provider !== 'none' && ai().model.trim() !== '';

  const destinationHost = createMemo(() => {
    try {
      return new URL(ai().baseUrl).host || ai().baseUrl;
    } catch {
      return ai().baseUrl;
    }
  });

  const byteSize = createMemo(() => new Blob([logText()]).size);
  const tooBig = () => byteSize() > AI_MAX_LOG_BYTES;

  function reset() {
    setLogText('');
    setPhase({ kind: 'paste' });
    setError(null);
  }

  async function doExtract() {
    if (!enabled()) {
      setError('Set up an AI provider in Settings → AI first.');
      return;
    }
    if (logText().trim() === '') {
      setError('Paste a log first.');
      return;
    }
    if (tooBig()) {
      setError(
        `Log is ${(byteSize() / 1024).toFixed(1)} KB — max is ${
          AI_MAX_LOG_BYTES / 1024
        } KB. Trim or split before extracting.`,
      );
      return;
    }
    setError(null);
    setPhase({ kind: 'extracting' });
    try {
      const a = ai();
      const out = await aiExtractLog({
        provider: a.provider,
        apiKey: a.apiKey,
        baseUrl: a.baseUrl,
        model: a.model,
        logText: logText(),
      });
      setPhase({
        kind: 'review',
        results: out.requests,
        selected: out.requests.map(() => true),
        raw: out.raw,
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setPhase({ kind: 'paste' });
    }
  }

  async function doImport() {
    const p = phase();
    if (p.kind !== 'review') return;
    const picked = p.results.filter((_, i) => p.selected[i]);
    if (picked.length === 0) {
      setError('Select at least one request.');
      return;
    }
    const ws = workspace();
    if (!ws) {
      setError('Open a workspace first.');
      return;
    }
    setError(null);
    setPhase({ kind: 'importing' });
    try {
      const report = await aiImportExtracted(ws.root, picked);
      notify.success('AI log import', `${report.requests_created} requests imported.`);
      const refreshed = await workspaceReload(ws.root);
      setWorkspace(refreshed);
      reset();
      props.onOpenChange(false);
    } catch (e) {
      notifyError('AI import failed', e);
      setPhase({ kind: 'review', results: p.results, selected: p.selected, raw: p.raw });
    }
  }

  function toggleAt(i: number) {
    const p = phase();
    if (p.kind !== 'review') return;
    const next = p.selected.slice();
    next[i] = !next[i];
    setPhase({ ...p, selected: next });
  }

  return (
    <Dialog open={props.open} onOpenChange={(v) => {
      if (!v) reset();
      props.onOpenChange(v);
    }}>
      <Dialog.Portal>
        <Dialog.Overlay class="fixed inset-0 z-50 bg-black/50" />
        <Dialog.Content class="fixed left-1/2 top-1/2 z-50 flex h-[640px] w-[760px] max-w-[95vw] -translate-x-1/2 -translate-y-1/2 flex-col rounded-md border border-border bg-bg-card p-4 shadow-xl">
          <Dialog.Title class="text-[14px] font-medium text-fg-primary">
            Import from log file (AI)
          </Dialog.Title>
          <Dialog.Description class="mt-1 text-[11px] text-fg-secondary">
            Paste any HTTP-ish log (Android logcat, Charles, nginx, ad-hoc
            backend logs). The configured AI provider extracts requests; you
            pick which to import.
          </Dialog.Description>

          <Show when={!enabled()}>
            <div class="mt-3 rounded border border-warning bg-warning/10 px-3 py-2 text-[12px] text-fg-primary">
              No AI provider configured.{' '}
              <button
                type="button"
                class="underline hover:no-underline"
                onClick={() => {
                  props.onOpenChange(false);
                  openSettings('ai');
                }}
              >
                Open Settings → AI
              </button>{' '}
              to add an API key and model.
            </div>
          </Show>

          <Show when={enabled() && phase().kind === 'paste'}>
            <div class="mt-2 rounded border border-border bg-bg-secondary/60 px-3 py-1.5 text-[11px] text-fg-secondary">
              <strong class="text-fg-primary">Privacy:</strong> on Extract,{' '}
              <span class="font-mono">{(byteSize() / 1024).toFixed(1)} KB</span>{' '}
              go directly to{' '}
              <span class="font-mono text-fg-primary">{destinationHost()}</span>{' '}
              with your API key. Argos doesn't see or store the log.
            </div>

            <textarea
              class="mt-2 h-72 w-full flex-1 resize-none rounded border border-border bg-bg-secondary p-3 font-mono text-[11px] leading-relaxed outline-none focus:border-primary scrollbar-thin"
              placeholder={'11:43:05.612  D/Networking  --> POST https://api.example.com/users\n11:43:05.613  D/Networking  Content-Type: application/json\n...'}
              value={logText()}
              spellcheck={false}
              autocomplete="off"
              autocorrect="off"
              onInput={(e) => setLogText(e.currentTarget.value)}
            />
            <div class="mt-1 flex items-center justify-between text-[10px] font-mono text-fg-secondary">
              <span classList={{ 'text-error': tooBig() }}>
                {(byteSize() / 1024).toFixed(1)} KB / {AI_MAX_LOG_BYTES / 1024} KB max
              </span>
              <span>
                model: <span class="text-fg-primary">{ai().model || '<unset>'}</span>
              </span>
            </div>
          </Show>

          <Show when={phase().kind === 'extracting'}>
            <div class="mt-4 flex flex-1 items-center justify-center text-[12px] text-fg-secondary">
              Asking {destinationHost()}…
            </div>
          </Show>

          <Show when={phase().kind === 'review'}>
            {(() => {
              const p = phase() as Extract<Phase, { kind: 'review' }>;
              return (
                <>
                  <div class="mt-2 text-[11px] text-fg-secondary">
                    Found <strong class="text-fg-primary">{p.results.length}</strong> requests.
                    Uncheck anything you don't want; the rest will land under a new
                    "AI import" folder.
                  </div>
                  <Show when={p.results.length === 0}>
                    <div class="mt-3 rounded border border-border bg-bg-secondary/60 p-3 text-[11px] text-fg-secondary">
                      The model didn't find any requests. Raw output:
                      <pre class="mt-2 max-h-40 overflow-auto rounded bg-bg-card p-2 font-mono text-[10px] text-fg-primary scrollbar-thin">
                        {p.raw}
                      </pre>
                    </div>
                  </Show>
                  <ul class="mt-2 flex-1 overflow-auto rounded border border-border scrollbar-thin">
                    <For each={p.results}>
                      {(r, i) => (
                        <li class="flex items-center gap-3 border-b border-border/60 px-3 py-2 text-[12px] last:border-b-0">
                          <input
                            type="checkbox"
                            class="accent-[var(--color-primary)]"
                            checked={p.selected[i()]}
                            onChange={() => toggleAt(i())}
                          />
                          <span class="w-14 shrink-0 font-mono text-[10px] font-bold text-fg-primary">
                            {r.method}
                          </span>
                          <span class="min-w-0 flex-1 truncate font-mono text-[11px] text-fg-primary">
                            {r.url}
                          </span>
                          <span class="ml-auto text-[10px] text-fg-secondary">
                            {r.headers.length} hdrs
                            {r.body ? ` · body: ${r.body.kind}` : ''}
                          </span>
                        </li>
                      )}
                    </For>
                  </ul>
                </>
              );
            })()}
          </Show>

          <Show when={phase().kind === 'importing'}>
            <div class="mt-4 flex flex-1 items-center justify-center text-[12px] text-fg-secondary">
              Writing requests to workspace…
            </div>
          </Show>

          <Show when={error()}>
            <div class="mt-2 rounded border border-error bg-error/10 px-2 py-1 text-[12px] text-error">
              {error()}
            </div>
          </Show>

          <div class="mt-3 flex justify-end gap-2">
            <Show when={phase().kind === 'review'}>
              <button
                type="button"
                class="rounded border border-border px-3 py-1 text-[12px] hover:bg-bg-secondary"
                onClick={() => setPhase({ kind: 'paste' })}
              >
                Back to paste
              </button>
            </Show>
            <button
              type="button"
              class="rounded border border-border px-3 py-1 text-[12px] hover:bg-bg-secondary"
              onClick={() => props.onOpenChange(false)}
            >
              Cancel
            </button>
            <Show when={phase().kind === 'paste'}>
              <button
                type="button"
                class="rounded bg-primary px-3 py-1 text-[12px] text-primary-foreground hover:opacity-90 disabled:opacity-50"
                disabled={!enabled() || tooBig() || logText().trim() === ''}
                onClick={() => void doExtract()}
              >
                Extract requests
              </button>
            </Show>
            <Show when={phase().kind === 'review'}>
              <button
                type="button"
                class="rounded bg-primary px-3 py-1 text-[12px] text-primary-foreground hover:opacity-90 disabled:opacity-50"
                onClick={() => void doImport()}
              >
                Import selected
              </button>
            </Show>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog>
  );
}
