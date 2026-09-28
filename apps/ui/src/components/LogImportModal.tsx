/**
 * Modal that takes a pasted log file and asks the configured AI
 * provider to extract HTTP requests from it. Rust splits the log into
 * ~8 KB chunks and exposes a per-chunk command; the loop, dedup, and
 * error UX live here so the user can Retry / Skip / Stop on each
 * failure without bouncing through Tauri events.
 *
 * Argos never proxies — the log + the user's API key go straight from
 * the desktop process to the configured provider. The privacy notice
 * above the textarea shows where the paste is about to land.
 */

import { createEffect, createMemo, createSignal, For, onCleanup, Show } from 'solid-js';
import { Dialog } from '@kobalte/core/dialog';

import {
  AI_CHUNK_BYTES,
  AI_MAX_LOG_BYTES,
  aiExtractCancel,
  aiExtractChunk,
  aiExtractSplit,
  aiImportExtracted,
  workspaceReload,
  type AiExtractInput,
  type AiExtractedRequest,
  type AiImportTarget,
} from '../lib/api';
import { notify, notifyError } from '../lib/toast';
import { settings } from '../stores/settings';
import { setWorkspace, workspace } from '../stores/workspace';
import { openSettings } from '../stores/settings-panel';
import { walkFolders } from '../types/workspace';

/** When chunk count crosses this, the Extract button is gated behind a
 *  confirm checkbox — at ~30s/chunk on Ollama, anything past 10 chunks
 *  starts becoming a "please leave the laptop on for 5+ minutes" run. */
const HEAVY_CHUNK_THRESHOLD = 10;

/** Decision the user picks from the per-chunk error dialog. `skip-all`
 *  is the "apply to all remaining failures" variant of `skip`. */
type ErrorDecision = 'retry' | 'skip' | 'skip-all' | 'stop';

type Phase =
  | { kind: 'paste' }
  | {
      kind: 'extracting';
      chunkIdx: number;
      chunksTotal: number;
      requestsSoFar: number;
      /** Cancel was clicked but the loop hasn't returned yet —
       *  drives the "Cancelling…" UI state. */
      cancelling: boolean;
    }
  | {
      kind: 'review';
      results: AiExtractedRequest[];
      selected: boolean[];
      raw: string;
      cancelled: boolean;
      chunksDone: number;
      chunksTotal: number;
    }
  | { kind: 'importing' };

type ErrorPromptState = {
  chunkIdx: number;
  chunksTotal: number;
  error: string;
};

type TargetMode = 'new' | 'existing';

function defaultFolderName(): string {
  const d = new Date();
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  return `AI import ${hh}:${mm}`;
}

/** Per-chunk wall-clock guesstimate (low, high) in seconds. Rough —
 *  the bands are honest enough for "is this 30 seconds or 30 minutes". */
function perChunkSeconds(provider: string): { low: number; high: number } {
  if (provider === 'ollama') return { low: 20, high: 90 };
  return { low: 4, high: 15 };
}

function formatSeconds(s: number): string {
  if (s < 60) return `${Math.round(s)} s`;
  const m = Math.round(s / 60);
  return `${m} min`;
}

/** Normalize a URL for dedup: lowercase scheme + host, drop a single
 *  trailing slash on the path. Path case is preserved — some backends
 *  are case-sensitive there. Mirrors the Rust `normalize_url_for_dedup`
 *  that lived in `ai.rs` before the loop moved to JS. */
function normalizeUrlForDedup(url: string): string {
  const schemeEnd = url.indexOf('://');
  if (schemeEnd === -1) return url;
  const scheme = url.substring(0, schemeEnd + 3);
  const rest = url.substring(schemeEnd + 3);
  const hostEndIdx = rest.search(/[/?#]/);
  const hostEnd = hostEndIdx === -1 ? rest.length : hostEndIdx;
  const host = rest.substring(0, hostEnd);
  const tail = rest.substring(hostEnd);
  const pathEndIdx = tail.search(/[?#]/);
  const pathEnd = pathEndIdx === -1 ? tail.length : pathEndIdx;
  let path = tail.substring(0, pathEnd);
  const qf = tail.substring(pathEnd);
  if (path.length > 1 && path.endsWith('/')) path = path.slice(0, -1);
  return scheme.toLowerCase() + host.toLowerCase() + path + qf;
}

/** Dedup key intentionally excludes headers — two log lines for the
 *  same endpoint with rotated auth tokens collapse into one entry, which
 *  matches user intent for replay. */
function dedupKey(r: AiExtractedRequest): string {
  let bodySig = '';
  if (r.body) {
    if (r.body.kind === 'json') {
      bodySig = JSON.stringify(r.body.value);
    } else if (r.body.kind === 'text') {
      bodySig = `text:${r.body.content}`;
    } else if (r.body.kind === 'form') {
      bodySig = 'form:' + r.body.fields.map((f) => `${f.name}=${f.value};`).join('');
    }
  }
  return `${r.method.toUpperCase()}|${normalizeUrlForDedup(r.url)}|${bodySig}`;
}

export default function LogImportModal(props: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
}) {
  const [logText, setLogText] = createSignal('');
  const [phase, setPhase] = createSignal<Phase>({ kind: 'paste' });
  const [error, setError] = createSignal<string | null>(null);
  const [targetMode, setTargetMode] = createSignal<TargetMode>('new');
  const [newFolderName, setNewFolderName] = createSignal(defaultFolderName());
  const [existingFolderPath, setExistingFolderPath] = createSignal<string>('');
  const [confirmHeavy, setConfirmHeavy] = createSignal(false);

  // Error-dialog plumbing. The loop awaits a Promise; clicking a
  // button resolves it and the loop continues with the user's choice.
  const [errorPrompt, setErrorPrompt] = createSignal<ErrorPromptState | null>(null);
  const [applyToAll, setApplyToAll] = createSignal(false);
  let errorResolver: ((d: ErrorDecision) => void) | null = null;

  // Set when Cancel is clicked. Outlives the phase so the loop's
  // try/catch can branch on "user requested stop" vs "Rust returned
  // 'cancelled' for some other reason".
  let cancelRequested = false;

  const folderOptions = createMemo(() => {
    const ws = workspace();
    if (!ws) return [];
    return walkFolders(ws.tree);
  });

  const ai = () => settings().ai;
  const enabled = () => ai().provider !== 'none' && ai().model.trim() !== '';

  const destinationLabel = createMemo(() => {
    const { provider, baseUrl, model } = ai();
    const m = model || '?';
    switch (provider) {
      case 'ollama':
        return `your local Ollama (${m})`;
      case 'anthropic':
        return `Anthropic (${m})`;
      case 'openrouter':
        return `OpenRouter (${m})`;
      case 'openai-compatible': {
        let host = baseUrl;
        try {
          host = new URL(baseUrl).host || baseUrl;
        } catch {
          /* keep raw baseUrl */
        }
        return `${host} (${m})`;
      }
      default:
        return baseUrl;
    }
  });

  const byteSize = createMemo(() => new Blob([logText()]).size);
  const tooBig = () => byteSize() > AI_MAX_LOG_BYTES;
  const chunkCount = createMemo(() => Math.max(1, Math.ceil(byteSize() / AI_CHUNK_BYTES)));
  const approxTokens = createMemo(() => Math.round(byteSize() / 4));
  const etaLabel = createMemo(() => {
    const { low, high } = perChunkSeconds(ai().provider);
    const n = chunkCount();
    return `~${formatSeconds(low * n)} – ${formatSeconds(high * n)}`;
  });
  const isHeavy = () => chunkCount() > HEAVY_CHUNK_THRESHOLD;
  const extractDisabled = () =>
    !enabled() || tooBig() || logText().trim() === '' || (isHeavy() && !confirmHeavy());

  // Drop the heavy-confirm when the log shrinks below threshold so the
  // warning re-shows on the next big paste.
  createEffect(() => {
    if (!isHeavy()) setConfirmHeavy(false);
  });

  function reset() {
    setLogText('');
    setPhase({ kind: 'paste' });
    setError(null);
    setTargetMode('new');
    setNewFolderName(defaultFolderName());
    setExistingFolderPath('');
    setConfirmHeavy(false);
    setErrorPrompt(null);
    setApplyToAll(false);
    errorResolver = null;
    // Critical: a `doExtract` may still be awaiting Rust when reset
    // fires (e.g. user closed the modal). Leaving `cancelRequested` as
    // false would let the loop fall through to its final `setPhase
    // 'review'` call, clobbering this paste-phase reset and surfacing
    // stale results the next time the modal opens. Flipping the bit
    // here makes the loop break on its next await + the phase guard
    // below skip the stale write.
    cancelRequested = true;
  }

  onCleanup(() => {
    // If the component goes away mid-run, make sure Rust drops the
    // in-flight HTTP future too.
    if (phase().kind === 'extracting') void aiExtractCancel();
  });

  function askErrorDecision(p: ErrorPromptState): Promise<ErrorDecision> {
    setApplyToAll(false);
    return new Promise<ErrorDecision>((resolve) => {
      errorResolver = resolve;
      setErrorPrompt(p);
    });
  }

  function resolveError(base: 'retry' | 'skip' | 'stop') {
    let decision: ErrorDecision = base;
    if (base === 'skip' && applyToAll()) decision = 'skip-all';
    setErrorPrompt(null);
    const r = errorResolver;
    errorResolver = null;
    if (r) r(decision);
  }

  /** Single source of truth for "modal is going away". The in-modal
   *  Close button bypasses Kobalte's onOpenChange (external prop
   *  changes don't trigger it), so this function makes both paths
   *  funnel through the same teardown — cancel any in-flight run,
   *  wipe signals, then tell the parent to flip `open`. Without it
   *  the signals stay alive between open/close cycles and the modal
   *  re-opens in the state it was last closed in. */
  function closeAndReset() {
    if (phase().kind === 'extracting') requestCancel();
    reset();
    props.onOpenChange(false);
  }

  function requestCancel() {
    cancelRequested = true;
    // Immediate UI feedback — flip the phase to its `cancelling` flavor.
    const p = phase();
    if (p.kind === 'extracting') {
      setPhase({ ...p, cancelling: true });
    }
    // Resolve any pending error dialog so the loop unblocks.
    if (errorResolver) resolveError('stop');
    // Tell Rust to drop the in-flight reqwest future (~50 ms latency).
    void aiExtractCancel();
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
    cancelRequested = false;
    setPhase({
      kind: 'extracting',
      chunkIdx: 0,
      chunksTotal: chunkCount(),
      requestsSoFar: 0,
      cancelling: false,
    });

    const a = ai();
    const input: AiExtractInput = {
      provider: a.provider,
      apiKey: a.apiKey,
      baseUrl: a.baseUrl,
      model: a.model,
    };

    const accumulated: AiExtractedRequest[] = [];
    const seen = new Set<string>();
    const raws: string[] = [];
    let skipAll = false;
    let chunksDone = 0;

    let chunks: string[];
    try {
      chunks = await aiExtractSplit(logText());
    } catch (e) {
      // Same race as the review-phase guard below: don't clobber a
      // fresh paste state with an error from an abandoned run.
      if (cancelRequested) return;
      setError(e instanceof Error ? e.message : String(e));
      setPhase({ kind: 'paste' });
      return;
    }

    if (cancelRequested) return;

    setPhase({
      kind: 'extracting',
      chunkIdx: 0,
      chunksTotal: chunks.length,
      requestsSoFar: 0,
      cancelling: false,
    });

    for (let i = 0; i < chunks.length; i++) {
      if (cancelRequested) break;
      setPhase({
        kind: 'extracting',
        chunkIdx: i,
        chunksTotal: chunks.length,
        requestsSoFar: accumulated.length,
        cancelling: false,
      });

      let chunkSettled = false;
      while (!chunkSettled) {
        if (cancelRequested) break;
        try {
          const out = await aiExtractChunk(input, chunks[i]!);
          raws.push(`---chunk ${i + 1}/${chunks.length}---\n${out.raw}`);
          for (const r of out.requests) {
            const key = dedupKey(r);
            if (!seen.has(key)) {
              seen.add(key);
              accumulated.push(r);
            }
          }
          const p = phase();
          if (p.kind === 'extracting') {
            setPhase({ ...p, requestsSoFar: accumulated.length });
          }
          chunksDone++;
          chunkSettled = true;
        } catch (e) {
          const errMsg = e instanceof Error ? e.message : String(e);
          // The Rust side returns this literal when `tokio::select!`
          // loses to the cancel watcher.
          if (errMsg === 'cancelled' || cancelRequested) {
            raws.push(`---chunk ${i + 1}/${chunks.length} CANCELLED---`);
            cancelRequested = true;
            break;
          }
          raws.push(`---chunk ${i + 1}/${chunks.length} ERROR---\n${errMsg}`);
          if (skipAll) {
            chunksDone++;
            chunkSettled = true;
            break;
          }
          const decision = await askErrorDecision({
            chunkIdx: i,
            chunksTotal: chunks.length,
            error: errMsg,
          });
          if (decision === 'retry') {
            // Loop again on the same chunk.
            continue;
          }
          if (decision === 'skip') {
            chunksDone++;
            chunkSettled = true;
          } else if (decision === 'skip-all') {
            skipAll = true;
            chunksDone++;
            chunkSettled = true;
          } else {
            // 'stop'
            cancelRequested = true;
            break;
          }
        }
      }
    }

    // If `reset()` ran while we were awaiting (modal closed,
    // user re-opened with a fresh paste), phase has already moved
    // away from 'extracting'. Don't clobber that fresh state with
    // partial review data from a job the user already abandoned.
    if (phase().kind !== 'extracting') return;

    setPhase({
      kind: 'review',
      results: accumulated,
      selected: accumulated.map(() => true),
      raw: raws.join('\n\n'),
      cancelled: cancelRequested,
      chunksDone,
      chunksTotal: chunks.length,
    });
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
    let target: AiImportTarget;
    if (targetMode() === 'existing') {
      const fp = existingFolderPath();
      if (!fp) {
        setError('Pick an existing folder, or switch to "New folder".');
        return;
      }
      target = { kind: 'existing', folderPath: fp };
    } else {
      target = {
        kind: 'new',
        name: newFolderName().trim() || defaultFolderName(),
      };
    }
    setError(null);
    setPhase({ kind: 'importing' });
    try {
      const report = await aiImportExtracted(ws.root, picked, target);
      notify.success('AI log import', `${report.requests_created} requests imported.`);
      const refreshed = await workspaceReload(ws.root);
      setWorkspace(refreshed);
      reset();
      props.onOpenChange(false);
    } catch (e) {
      notifyError('AI import failed', e);
      setPhase({
        kind: 'review',
        results: p.results,
        selected: p.selected,
        raw: p.raw,
        cancelled: p.cancelled,
        chunksDone: p.chunksDone,
        chunksTotal: p.chunksTotal,
      });
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
    <Dialog
      open={props.open}
      onOpenChange={(v) => {
        if (!v) closeAndReset();
        else props.onOpenChange(v);
      }}
    >
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
              <span class="font-mono text-fg-primary">{destinationLabel()}</span>.
              Argos doesn't see or store the log.
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

            <Show when={chunkCount() > 1 && !tooBig()}>
              <div
                class="mt-2 rounded border border-border bg-bg-secondary/40 px-3 py-2 text-[11px] text-fg-secondary"
                classList={{ 'border-warning bg-warning/10': isHeavy() }}
              >
                <div class="text-fg-primary">
                  This log will be split into{' '}
                  <span class="font-mono">{chunkCount()}</span> chunks of ~
                  {AI_CHUNK_BYTES / 1024} KB.
                </div>
                <div class="mt-0.5">
                  ≈ <span class="font-mono">{approxTokens().toLocaleString()}</span>{' '}
                  input tokens · est. wall-clock{' '}
                  <span class="font-mono">{etaLabel()}</span>. Your provider's
                  pricing applies.
                </div>
                <Show when={isHeavy()}>
                  <label class="mt-2 flex cursor-pointer items-center gap-1.5 text-fg-primary">
                    <input
                      type="checkbox"
                      class="accent-[var(--color-primary)]"
                      checked={confirmHeavy()}
                      onChange={(e) => setConfirmHeavy(e.currentTarget.checked)}
                    />
                    I understand this will make {chunkCount()} provider calls and
                    may take a while.
                  </label>
                </Show>
              </div>
            </Show>
          </Show>

          <Show when={phase().kind === 'extracting'}>
            {(() => {
              const p = phase() as Extract<Phase, { kind: 'extracting' }>;
              const pct =
                p.chunksTotal === 0
                  ? 0
                  : Math.min(100, Math.round((p.chunkIdx / p.chunksTotal) * 100));
              const ep = errorPrompt();
              return (
                <div class="mt-4 flex flex-1 flex-col items-center justify-center gap-3 text-[12px] text-fg-secondary">
                  <Show when={!ep}>
                    <div>
                      <Show when={p.cancelling} fallback={`Asking ${destinationLabel()}…`}>
                        Cancelling — current chunk is finishing…
                      </Show>
                    </div>
                  </Show>
                  <div class="w-2/3">
                    <div class="h-1.5 w-full overflow-hidden rounded bg-bg-secondary">
                      <div
                        class="h-full bg-primary transition-[width] duration-200"
                        style={{ width: `${pct}%` }}
                      />
                    </div>
                    <div class="mt-1 flex justify-between font-mono text-[10px]">
                      <span>
                        chunk {Math.min(p.chunkIdx + 1, p.chunksTotal)} / {p.chunksTotal}
                      </span>
                      <span>{p.requestsSoFar} requests so far</span>
                    </div>
                  </div>

                  <Show when={ep}>
                    {(() => {
                      const e = ep as ErrorPromptState;
                      const isOllama = ai().provider === 'ollama';
                      return (
                        <div class="mt-2 w-full max-w-[560px] rounded border border-error bg-error/10 p-3 text-left text-[12px]">
                          <div class="text-fg-primary">
                            Chunk{' '}
                            <span class="font-mono">
                              {e.chunkIdx + 1}/{e.chunksTotal}
                            </span>{' '}
                            failed.
                          </div>
                          <pre class="mt-1 max-h-24 overflow-auto rounded bg-bg-card p-2 font-mono text-[10px] text-error scrollbar-thin">
                            {e.error}
                          </pre>
                          <Show when={isOllama}>
                            <div class="mt-2 text-[11px] text-fg-secondary">
                              Local Ollama is struggling with this log. Consider
                              switching to a cloud provider (Anthropic /
                              OpenRouter / OpenAI-compatible) for big logs —
                              they handle longer prompts in seconds rather than
                              minutes.
                            </div>
                          </Show>
                          <label class="mt-2 flex cursor-pointer items-center gap-1.5 text-[11px] text-fg-primary">
                            <input
                              type="checkbox"
                              class="accent-[var(--color-primary)]"
                              checked={applyToAll()}
                              onChange={(ev) => setApplyToAll(ev.currentTarget.checked)}
                            />
                            Apply Skip to all remaining failures (don't ask again)
                          </label>
                          <div class="mt-2 flex justify-end gap-2">
                            <button
                              type="button"
                              class="rounded border border-border px-2.5 py-1 text-[11px] hover:bg-bg-secondary"
                              onClick={() => resolveError('retry')}
                            >
                              Retry chunk
                            </button>
                            <button
                              type="button"
                              class="rounded border border-border px-2.5 py-1 text-[11px] hover:bg-bg-secondary"
                              onClick={() => resolveError('skip')}
                            >
                              Skip chunk
                            </button>
                            <button
                              type="button"
                              class="rounded border border-error px-2.5 py-1 text-[11px] text-error hover:bg-error/10"
                              onClick={() => resolveError('stop')}
                            >
                              Stop
                            </button>
                          </div>
                        </div>
                      );
                    })()}
                  </Show>

                  <Show when={!ep}>
                    <button
                      type="button"
                      class="mt-2 rounded border border-border px-3 py-1 text-[12px] hover:bg-bg-secondary disabled:opacity-50"
                      disabled={p.cancelling}
                      onClick={() => requestCancel()}
                    >
                      <Show when={p.cancelling} fallback={"Cancel — keep what's found so far"}>
                        Cancelling…
                      </Show>
                    </button>
                  </Show>
                </div>
              );
            })()}
          </Show>

          <Show when={phase().kind === 'review'}>
            {(() => {
              const p = phase() as Extract<Phase, { kind: 'review' }>;
              return (
                <>
                  <div class="mt-2 text-[11px] text-fg-secondary">
                    Found <strong class="text-fg-primary">{p.results.length}</strong>{' '}
                    requests
                    <Show when={p.chunksTotal > 1}>
                      {' '}
                      across{' '}
                      <span class="font-mono">
                        {p.chunksDone}/{p.chunksTotal}
                      </span>{' '}
                      chunks
                    </Show>
                    <Show when={p.cancelled}>
                      <span class="ml-1 text-warning">(stopped — partial)</span>
                    </Show>
                    . Uncheck anything you don't want; the rest will land where you
                    pick below.
                  </div>

                  <Show when={p.results.length > 0}>
                    <div class="mt-3 rounded border border-border bg-bg-secondary/40 p-2">
                      <div class="flex gap-4 text-[11px]">
                        <label class="flex cursor-pointer items-center gap-1.5">
                          <input
                            type="radio"
                            class="accent-[var(--color-primary)]"
                            checked={targetMode() === 'new'}
                            onChange={() => setTargetMode('new')}
                          />
                          New folder
                        </label>
                        <label
                          class="flex cursor-pointer items-center gap-1.5"
                          classList={{
                            'opacity-50 cursor-not-allowed': folderOptions().length === 0,
                          }}
                        >
                          <input
                            type="radio"
                            class="accent-[var(--color-primary)]"
                            checked={targetMode() === 'existing'}
                            disabled={folderOptions().length === 0}
                            onChange={() => setTargetMode('existing')}
                          />
                          Add to existing folder
                          <Show when={folderOptions().length === 0}>
                            <span class="ml-1 text-fg-secondary">(no folders yet)</span>
                          </Show>
                        </label>
                      </div>

                      <Show when={targetMode() === 'new'}>
                        <div class="mt-2 flex items-center gap-2">
                          <label class="w-[110px] text-[11px] text-fg-secondary">Folder name</label>
                          <input
                            type="text"
                            spellcheck={false}
                            autocomplete="off"
                            class="flex-1 rounded border border-border bg-bg-secondary px-2 py-1 font-mono text-[11px]"
                            value={newFolderName()}
                            onInput={(e) => setNewFolderName(e.currentTarget.value)}
                          />
                        </div>
                        <p class="mt-1 pl-[118px] text-[10px] text-fg-secondary">
                          Created under{' '}
                          <span class="font-mono">collections/</span> (or the workspace root if
                          there is no <span class="font-mono">collections/</span> dir).
                        </p>
                      </Show>

                      <Show when={targetMode() === 'existing'}>
                        <div class="mt-2 flex items-center gap-2">
                          <label class="w-[110px] text-[11px] text-fg-secondary">Folder</label>
                          <select
                            class="flex-1 rounded border border-border bg-bg-secondary px-2 py-1 font-mono text-[11px]"
                            value={existingFolderPath()}
                            onChange={(e) => setExistingFolderPath(e.currentTarget.value)}
                          >
                            <option value="">— pick a folder —</option>
                            <For each={folderOptions()}>
                              {(f) => <option value={f.path}>{f.label}</option>}
                            </For>
                          </select>
                        </div>
                        <p class="mt-1 pl-[118px] text-[10px] text-fg-secondary">
                          Files are appended into this folder — existing requests are not touched.
                        </p>
                      </Show>
                    </div>
                  </Show>

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
              onClick={() => closeAndReset()}
            >
              Close
            </button>
            <Show when={phase().kind === 'paste'}>
              <button
                type="button"
                class="rounded bg-primary px-3 py-1 text-[12px] text-primary-foreground hover:opacity-90 disabled:opacity-50"
                disabled={extractDisabled()}
                onClick={() => void doExtract()}
              >
                <Show when={chunkCount() > 1} fallback={'Extract requests'}>
                  Extract from {chunkCount()} chunks
                </Show>
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
