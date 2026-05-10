/**
 * Response viewer.
 *
 * Three modes via the `getResponse(tabId)` state:
 * - `idle` — placeholder before any send.
 * - `loading` — animated indicator + elapsed counter.
 * - `ok` — full status / timing / size, then a tabbed body / headers view.
 * - `error` — friendly error card with the engine message.
 *
 * For T1.4 v0.1 the body view supports JSON pretty-print and raw text.
 * Hex / preview / virtualisation lands in a follow-up — note the `Show` for
 * `bytes.length > 200_000` that bails out for very large bodies until then.
 */

import { createMemo, createSignal, For, Match, Show, Switch } from 'solid-js';

import { CheckCircle2, Loader2, ShieldCheck, XCircle } from 'lucide-solid';

import { bytesToDataUrl } from '../lib/bytes';
import { getResponse } from '../stores/request';
import { activeTabId } from '../stores/tabs';
import {
  bytesToString,
  isJsonContentType,
  isTextContentType,
  type HttpResponse,
} from '../types/http';

import HexViewer from './HexViewer';

const LARGE_BODY_BYTES = 200_000;

type BodyTab = 'body' | 'headers' | 'timing';

export default function ResponsePane() {
  const tabId = activeTabId;
  const state = () => {
    const id = tabId();
    return id ? getResponse(id) : { status: 'idle' as const };
  };

  return (
    <div class="flex h-full w-full flex-col bg-bg-card">
      <Switch>
        <Match when={state().status === 'idle'}>
          <IdlePane />
        </Match>
        <Match when={state().status === 'loading'}>
          <LoadingPane />
        </Match>
        <Match when={state().status === 'error'}>
          {(_) => {
            const s = state() as Extract<ReturnType<typeof state>, { status: 'error' }>;
            return <ErrorPane message={s.message} />;
          }}
        </Match>
        <Match when={state().status === 'ok'}>
          {(_) => {
            const s = state() as Extract<ReturnType<typeof state>, { status: 'ok' }>;
            return <OkPane response={s.response} />;
          }}
        </Match>
      </Switch>
    </div>
  );
}

// ---- panes ----------------------------------------------------------------

function IdlePane() {
  return (
    <div class="flex flex-1 items-center justify-center text-center">
      <p class="max-w-sm text-[12px] text-fg-secondary">
        Press <kbd class="rounded bg-bg-secondary px-1.5 py-0.5 font-mono text-[11px]">⌘ Enter</kbd>{' '}
        or click <span class="font-medium">Send</span> to fire the request.
      </p>
    </div>
  );
}

function LoadingPane() {
  return (
    <div class="flex flex-1 flex-col items-center justify-center gap-2 text-fg-secondary">
      <Loader2 size={20} class="animate-spin text-primary" />
      <p class="text-[12px]">Sending…</p>
    </div>
  );
}

function ErrorPane(props: { message: string }) {
  return (
    <div class="flex flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
      <XCircle size={20} class="text-[var(--color-error-foreground)]" />
      <p class="font-mono text-[13px] text-[var(--color-error-foreground)]">Request failed</p>
      <p class="max-w-md font-mono text-[11px] text-fg-secondary">{props.message}</p>
    </div>
  );
}

function OkPane(props: { response: HttpResponse }) {
  const [activePane, setActivePane] = createSignal<BodyTab>('body');

  const r = () => props.response;
  const statusColor = () => {
    const s = r().status;
    if (s >= 500) return 'var(--color-error-foreground)';
    if (s >= 400) return 'var(--color-warning-foreground)';
    if (s >= 300) return 'var(--color-info-foreground)';
    return 'var(--color-success-foreground)';
  };
  const StatusIcon = () => {
    const s = r().status;
    return s >= 400 ? (
      <XCircle size={14} style={{ color: statusColor() }} />
    ) : (
      <CheckCircle2 size={14} style={{ color: statusColor() }} />
    );
  };

  return (
    <>
      <div class="flex h-12 shrink-0 items-center gap-3 border-b border-border px-4">
        <div class="flex items-center gap-1.5">
          <StatusIcon />
          <span class="font-mono text-[13px] font-bold" style={{ color: statusColor() }}>
            {r().status} {r().status_text}
          </span>
        </div>
        <span class="text-fg-secondary">·</span>
        <span class="font-mono text-[12px] text-fg-secondary">
          {r().timing.total_ms} ms
        </span>
        <span class="text-fg-secondary">·</span>
        <span class="font-mono text-[12px] text-fg-secondary">{formatBytes(r().body.size_bytes)}</span>
        <Show when={r().body.content_type}>
          <span class="text-fg-secondary">·</span>
          <span class="flex items-center gap-1 font-mono text-[11px] text-fg-secondary">
            <ShieldCheck size={12} />
            {r().body.content_type}
          </span>
        </Show>
        <span class="ml-auto truncate font-mono text-[11px] text-fg-secondary" title={r().final_url}>
          {r().final_url}
        </span>
      </div>

      <div class="flex h-9 shrink-0 items-end border-b border-border px-3">
        <For each={[
          { id: 'body' as BodyTab, label: 'Body' },
          { id: 'headers' as BodyTab, label: 'Headers' },
          { id: 'timing' as BodyTab, label: 'Timing' },
        ]}>
          {(t) => (
            <button
              type="button"
              class="relative px-3 py-2 text-[13px]"
              classList={{
                'text-fg-primary': activePane() === t.id,
                'text-fg-secondary hover:text-fg-primary': activePane() !== t.id,
              }}
              onClick={() => setActivePane(t.id)}
            >
              <span class="flex items-center gap-1.5">
                {t.label}
                <Show when={t.id === 'headers'}>
                  <span class="rounded-full bg-bg-secondary px-1.5 font-mono text-[10px] text-fg-secondary">
                    {r().headers.length}
                  </span>
                </Show>
              </span>
              <Show when={activePane() === t.id}>
                <span class="absolute inset-x-3 -bottom-px h-0.5 bg-primary" aria-hidden />
              </Show>
            </button>
          )}
        </For>
      </div>

      <div class="flex-1 overflow-auto scrollbar-thin">
        <Switch>
          <Match when={activePane() === 'body'}>
            <BodyView response={r()} />
          </Match>
          <Match when={activePane() === 'headers'}>
            <HeadersView headers={r().headers} />
          </Match>
          <Match when={activePane() === 'timing'}>
            <TimingView timing={r().timing} />
          </Match>
        </Switch>
      </div>
    </>
  );
}

// ---- body / headers / timing views ----------------------------------------

function BodyView(props: { response: HttpResponse }) {
  const ct = () => props.response.body.content_type;
  const isImage = () => (ct() ?? '').startsWith('image/');
  const isText = () => isTextContentType(ct());
  const isLarge = () => props.response.body.size_bytes >= LARGE_BODY_BYTES;

  return (
    <Switch>
      <Match when={isImage()}>
        <ImageView response={props.response} />
      </Match>
      <Match when={isLarge() && isText()}>
        <p class="p-4 font-mono text-[12px] text-fg-secondary">
          Body is {formatBytes(props.response.body.size_bytes)} — text preview disabled until
          streaming render lands. (Tracked under T1.4 follow-up.)
        </p>
      </Match>
      <Match when={isText()}>
        <TextView response={props.response} />
      </Match>
      <Match when={true}>
        <HexViewer bytes={props.response.body.bytes} />
      </Match>
    </Switch>
  );
}

function TextView(props: { response: HttpResponse }) {
  const text = createMemo(() => bytesToString(props.response.body.bytes));
  const ct = () => props.response.body.content_type;
  const pretty = createMemo(() => {
    if (!isJsonContentType(ct())) return null;
    try {
      return JSON.stringify(JSON.parse(text()), null, 2);
    } catch {
      return null;
    }
  });
  return (
    <pre class="m-0 whitespace-pre-wrap break-all p-4 font-mono text-[12px] leading-relaxed text-fg-primary">
      {pretty() ?? text()}
    </pre>
  );
}

function ImageView(props: { response: HttpResponse }) {
  const ct = () => props.response.body.content_type ?? 'image/*';
  const url = createMemo(() => bytesToDataUrl(props.response.body.bytes, ct()));
  return (
    <div class="flex flex-col items-start gap-2 p-4">
      <img
        src={url()}
        alt={`response ${ct()}`}
        class="max-w-full rounded border border-border bg-bg-secondary"
      />
      <p class="font-mono text-[11px] text-fg-secondary">
        {ct()} · {formatBytes(props.response.body.size_bytes)}
      </p>
    </div>
  );
}

function HeadersView(props: { headers: HttpResponse['headers'] }) {
  return (
    <table class="w-full font-mono text-[12px]">
      <tbody>
        <For each={props.headers}>
          {(h) => (
            <tr class="border-b border-border">
              <td class="w-1/3 max-w-xs px-4 py-2 align-top text-fg-secondary">{h.name}</td>
              <td class="break-all px-4 py-2 align-top">{h.value}</td>
            </tr>
          )}
        </For>
      </tbody>
    </table>
  );
}

function TimingView(props: { timing: HttpResponse['timing'] }) {
  const rows: Array<[string, number | null | undefined]> = [
    ['Total', props.timing.total_ms],
    ['Time to first byte', props.timing.ttfb_ms],
    ['Download', props.timing.download_ms],
    ['DNS lookup', props.timing.dns_ms],
    ['TCP connect', props.timing.connect_ms],
    ['TLS handshake', props.timing.tls_ms],
  ];
  return (
    <table class="w-full max-w-xl font-mono text-[12px]">
      <tbody>
        <For each={rows}>
          {([label, value]) => (
            <tr class="border-b border-border">
              <td class="w-1/2 px-4 py-2 text-fg-secondary">{label}</td>
              <td class="px-4 py-2 text-fg-primary">{value == null ? '—' : `${value} ms`}</td>
            </tr>
          )}
        </For>
      </tbody>
      <tfoot>
        <tr>
          <td colspan="2" class="px-4 py-3 text-[11px] text-fg-secondary">
            DNS / connect / TLS phases land alongside a custom reqwest connector
            in a T1.1 follow-up.
          </td>
        </tr>
      </tfoot>
    </table>
  );
}

// ---- helpers --------------------------------------------------------------

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}
