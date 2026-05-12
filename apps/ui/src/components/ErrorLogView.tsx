/**
 * Renderer-side error log panel — what `installCrashCapture` is
 * catching. Surfaced in the lower dock's Logs tab so the user
 * doesn't have to crack open the pending JSON files to know what
 * the app is choking on.
 *
 * Wiping the view only clears the in-memory list; the on-disk
 * pending reports stay and submit through the normal opt-in flow.
 */

import { For, Show } from 'solid-js';
import { Trash2 } from 'lucide-solid';

import { clearErrorLog, errorLogEntries } from '../stores/error-log';

export default function ErrorLogView() {
  return (
    <div class="flex h-full flex-col">
      <div class="flex shrink-0 items-center justify-between border-b border-border px-3 py-1.5 text-[11px] text-fg-secondary">
        <span>
          {errorLogEntries().length} caught runtime error
          {errorLogEntries().length === 1 ? '' : 's'} this session
        </span>
        <button
          type="button"
          class="flex items-center gap-1 rounded px-2 py-1 hover:bg-bg-secondary hover:text-fg-primary disabled:opacity-30"
          disabled={errorLogEntries().length === 0}
          onClick={() => clearErrorLog()}
          title="Clear log (doesn't drop pending crash reports)"
        >
          <Trash2 size={11} />
          Clear
        </button>
      </div>
      <Show
        when={errorLogEntries().length > 0}
        fallback={
          <p class="px-3 py-4 font-mono text-[12px] text-fg-secondary">
            No errors yet. Anything caught by the crash reporter
            (panic in Rust, JS error / unhandled rejection in the
            renderer) shows up here.
          </p>
        }
      >
        <ul class="min-h-0 flex-1 overflow-auto font-mono text-[11px]">
          <For each={errorLogEntries().slice().reverse()}>
            {(entry) => {
              const ts = new Date(entry.timestamp).toLocaleTimeString([], {
                hour12: false,
                hour: '2-digit',
                minute: '2-digit',
                second: '2-digit',
              });
              return (
                <li class="border-b border-border px-3 py-2">
                  <div class="flex items-baseline gap-2">
                    <span class="text-fg-secondary">{ts}</span>
                    <span class="rounded bg-bg-secondary px-1.5 py-0.5 text-[10px] text-fg-secondary">
                      {entry.source}
                    </span>
                  </div>
                  <div class="mt-1 text-fg-error break-all">{entry.message}</div>
                  <div class="mt-0.5 text-fg-secondary break-all">{entry.location}</div>
                </li>
              );
            }}
          </For>
        </ul>
      </Show>
    </div>
  );
}
