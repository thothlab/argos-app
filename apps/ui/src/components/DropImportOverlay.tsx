/**
 * Global drag-drop import overlay.
 *
 * Listens to the Tauri webview's native drag-drop events (HTML5 drop on
 * a `<div>` doesn't receive the filesystem path in Tauri 2). On drop
 * we ask the Rust side to sniff the file, then dispatch to the matching
 * importer command. Reuses the same per-format `*_import` paths the
 * File → Import menu calls — no shared wrapper, two callers isn't
 * enough to abstract.
 */

import { createSignal, onCleanup, onMount, Show } from 'solid-js';
import { Download, FileText, FolderOpen, Loader2, X } from 'lucide-solid';

import {
  brunoImport,
  importDetect,
  insomniaImport,
  openapiImport,
  postmanImport,
  workspaceReload,
  type ImportFormat,
  type PostmanImportReport,
} from '../lib/api';
import { isTauri } from '../lib/tauri';
import { workspace, setWorkspace } from '../stores/workspace';

type Phase =
  | { kind: 'idle' }
  | { kind: 'hover' } // drag is over the window
  | { kind: 'detecting'; path: string }
  | { kind: 'confirm'; path: string; format: ImportFormat; name: string }
  | { kind: 'running'; path: string; format: ImportFormat }
  | { kind: 'error'; message: string }
  | { kind: 'done'; report: PostmanImportReport; format: ImportFormat };

export default function DropImportOverlay() {
  const [phase, setPhase] = createSignal<Phase>({ kind: 'idle' });

  onMount(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    void (async () => {
      const { getCurrentWebview } = await import('@tauri-apps/api/webview');
      const wv = getCurrentWebview();
      const off = await wv.onDragDropEvent((event) => {
        const p = event.payload;
        // Tauri 2 emits enter | over | drop | leave. We only care about
        // start/end of the hover for the overlay and `drop` for the
        // actual import.
        if (p.type === 'enter' || p.type === 'over') {
          // Only show the hover overlay while we're actually idle —
          // don't stomp on an in-flight import.
          setPhase((cur) => (cur.kind === 'idle' ? { kind: 'hover' } : cur));
          return;
        }
        if (p.type === 'leave') {
          setPhase((cur) => (cur.kind === 'hover' ? { kind: 'idle' } : cur));
          return;
        }
        if (p.type === 'drop') {
          const paths = p.paths;
          if (!paths || paths.length === 0) {
            setPhase({ kind: 'idle' });
            return;
          }
          // Multi-file drop: take the first, warn about the rest. The
          // wizard is intentionally one-thing-at-a-time in v1.
          if (paths.length > 1) {
            console.warn(
              `Argos import: ignoring ${paths.length - 1} additional dropped file(s); only the first is imported.`,
            );
          }
          const first = paths[0];
          if (first) void handleDrop(first, setPhase);
        }
      });
      unlisten = off;
    })();
    onCleanup(() => {
      if (unlisten) unlisten();
    });
  });

  return (
    <>
      <Show when={phase().kind === 'hover'}>
        <HoverOverlay />
      </Show>
      <Show when={phase().kind !== 'idle' && phase().kind !== 'hover'}>
        <WizardModal phase={phase} setPhase={setPhase} />
      </Show>
    </>
  );
}

async function handleDrop(path: string, setPhase: (p: Phase) => void): Promise<void> {
  const ws = workspace();
  if (!ws) {
    setPhase({
      kind: 'error',
      message: 'Open or create a workspace before importing.',
    });
    return;
  }
  setPhase({ kind: 'detecting', path });
  try {
    const { format, name } = await importDetect(path);
    if (format === 'unknown') {
      setPhase({
        kind: 'error',
        message: `Couldn't recognise ${name} as a supported collection (Postman v2.1, Insomnia v4, OpenAPI 3.x, or Bruno).`,
      });
      return;
    }
    setPhase({ kind: 'confirm', path, format, name });
  } catch (e) {
    setPhase({ kind: 'error', message: messageOf(e) });
  }
}

async function runImport(
  path: string,
  format: ImportFormat,
  setPhase: (p: Phase) => void,
): Promise<void> {
  const ws = workspace();
  if (!ws) {
    setPhase({ kind: 'error', message: 'Workspace closed.' });
    return;
  }
  setPhase({ kind: 'running', path, format });
  try {
    let report: PostmanImportReport;
    switch (format) {
      case 'postman':
        report = await postmanImport(ws.root, path, false);
        break;
      case 'insomnia':
        report = await insomniaImport(ws.root, path, false);
        break;
      case 'openapi':
        report = await openapiImport(ws.root, path, false);
        break;
      case 'bruno':
        report = await brunoImport(ws.root, path);
        break;
      default:
        setPhase({ kind: 'error', message: `Unsupported format: ${format}` });
        return;
    }
    const refreshed = await workspaceReload(ws.root);
    setWorkspace(refreshed);
    setPhase({ kind: 'done', report, format });
  } catch (e) {
    setPhase({ kind: 'error', message: messageOf(e) });
  }
}

function messageOf(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === 'string') return e;
  return String(e);
}

function HoverOverlay() {
  return (
    <div
      class="pointer-events-none fixed inset-0 z-50 flex items-center justify-center bg-bg-primary/85 backdrop-blur-sm"
      aria-hidden="true"
    >
      <div class="flex flex-col items-center gap-3 rounded-2xl border-2 border-dashed border-primary px-12 py-8 text-center">
        <Download size={32} class="text-primary" />
        <p class="text-[15px] font-medium">Drop to import</p>
        <p class="max-w-xs text-[12px] text-fg-secondary">
          Postman v2.1, Insomnia v4, OpenAPI 3.x or Bruno collections
        </p>
      </div>
    </div>
  );
}

function WizardModal(props: {
  phase: () => Phase;
  setPhase: (p: Phase) => void;
}) {
  return (
    <div
      class="fixed inset-0 z-50 flex items-center justify-center bg-bg-primary/70"
      role="dialog"
      aria-modal="true"
    >
      <div class="flex w-[420px] flex-col gap-4 rounded-xl border border-border bg-bg-card p-5 shadow-xl">
        <header class="flex items-center justify-between">
          <h2 class="text-[14px] font-semibold">Import collection</h2>
          <button
            type="button"
            class="rounded p-1 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
            onClick={() => props.setPhase({ kind: 'idle' })}
            aria-label="Close"
          >
            <X size={14} />
          </button>
        </header>

        <Show when={props.phase().kind === 'detecting'}>
          <Row icon={<Loader2 size={16} class="animate-spin text-fg-secondary" />}>
            Detecting format…
          </Row>
        </Show>

        <Show when={(() => props.phase().kind === 'confirm')()}>
          {(() => {
            const p = props.phase();
            if (p.kind !== 'confirm') return null;
            return (
              <>
                <Row icon={iconFor(p.format)}>
                  <div class="flex flex-col">
                    <span class="font-medium">{p.name}</span>
                    <span class="text-[12px] text-fg-secondary">{labelFor(p.format)}</span>
                  </div>
                </Row>
                <div class="flex justify-end gap-2 pt-2">
                  <button
                    type="button"
                    class="rounded px-3 py-1.5 text-[12px] hover:bg-bg-secondary"
                    onClick={() => props.setPhase({ kind: 'idle' })}
                  >
                    Cancel
                  </button>
                  <button
                    type="button"
                    class="rounded bg-primary px-3 py-1.5 text-[12px] font-medium text-bg-primary hover:opacity-90"
                    onClick={() => void runImport(p.path, p.format, props.setPhase)}
                  >
                    Import
                  </button>
                </div>
              </>
            );
          })()}
        </Show>

        <Show when={(() => props.phase().kind === 'running')()}>
          {(() => {
            const p = props.phase();
            if (p.kind !== 'running') return null;
            return (
              <Row icon={<Loader2 size={16} class="animate-spin text-primary" />}>
                Importing as {labelFor(p.format)}…
              </Row>
            );
          })()}
        </Show>

        <Show when={(() => props.phase().kind === 'done')()}>
          {(() => {
            const p = props.phase();
            if (p.kind !== 'done') return null;
            const envHint = p.report.env_path
              ? `, ${p.report.variables_count} variable${p.report.variables_count === 1 ? '' : 's'} → env file`
              : '';
            return (
              <>
                <Row icon={iconFor(p.format)}>
                  <span>
                    Imported {p.report.requests_created} request
                    {p.report.requests_created === 1 ? '' : 's'} in {p.report.folders_created}{' '}
                    folder{p.report.folders_created === 1 ? '' : 's'}
                    {envHint}.
                  </span>
                </Row>
                <div class="flex justify-end pt-2">
                  <button
                    type="button"
                    class="rounded px-3 py-1.5 text-[12px] font-medium hover:bg-bg-secondary"
                    onClick={() => props.setPhase({ kind: 'idle' })}
                  >
                    Done
                  </button>
                </div>
              </>
            );
          })()}
        </Show>

        <Show when={(() => props.phase().kind === 'error')()}>
          {(() => {
            const p = props.phase();
            if (p.kind !== 'error') return null;
            return (
              <>
                <Row icon={<X size={16} class="text-fg-error" />}>
                  <span class="text-[12px]">{p.message}</span>
                </Row>
                <div class="flex justify-end pt-2">
                  <button
                    type="button"
                    class="rounded px-3 py-1.5 text-[12px] font-medium hover:bg-bg-secondary"
                    onClick={() => props.setPhase({ kind: 'idle' })}
                  >
                    Close
                  </button>
                </div>
              </>
            );
          })()}
        </Show>
      </div>
    </div>
  );
}

function Row(props: { icon: ReturnType<typeof FileText>; children: any }) {
  return (
    <div class="flex items-start gap-3 text-[13px]">
      <span class="mt-0.5">{props.icon}</span>
      <div class="flex-1">{props.children}</div>
    </div>
  );
}

function iconFor(format: ImportFormat) {
  switch (format) {
    case 'bruno':
      return <FolderOpen size={16} class="text-primary" />;
    default:
      return <FileText size={16} class="text-primary" />;
  }
}

function labelFor(format: ImportFormat): string {
  switch (format) {
    case 'postman':
      return 'Postman v2.1 collection';
    case 'insomnia':
      return 'Insomnia v4 export';
    case 'openapi':
      return 'OpenAPI 3.x document';
    case 'bruno':
      return 'Bruno collection';
    default:
      return 'Unknown format';
  }
}
