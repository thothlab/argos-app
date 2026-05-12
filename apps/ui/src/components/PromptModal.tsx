/**
 * Renders the inline text-prompt modal driven by `lib/prompt.ts`.
 * Mounted once at App root.
 */

import { createEffect, createSignal, Show } from 'solid-js';
import { X } from 'lucide-solid';

import { promptState, resolvePrompt } from '../lib/prompt';

export default function PromptModal() {
  const [value, setValue] = createSignal('');
  let inputEl: HTMLInputElement | undefined;

  // Reset value + focus when a fresh prompt opens.
  createEffect(() => {
    const s = promptState();
    if (s.open) {
      setValue(s.opts.defaultValue ?? '');
      // Defer focus to next tick so the input is in the DOM.
      queueMicrotask(() => inputEl?.focus());
    }
  });

  function submit() {
    const v = value().trim();
    resolvePrompt(v.length > 0 ? v : null);
  }

  function cancel() {
    resolvePrompt(null);
  }

  return (
    <Show when={promptState().open}>
      <div
        class="fixed inset-0 z-50 flex items-center justify-center bg-bg-primary/70"
        role="dialog"
        aria-modal="true"
        onClick={(e) => {
          // Click on backdrop = cancel; clicks inside the dialog
          // bubble up but our stopPropagation below catches them.
          if (e.target === e.currentTarget) cancel();
        }}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            cancel();
          }
        }}
      >
        <div class="flex w-[400px] flex-col gap-3 rounded-xl border border-border bg-bg-card p-5 shadow-xl">
          <header class="flex items-start justify-between gap-3">
            <div class="flex-1">
              <h2 class="text-[14px] font-semibold">{promptState().opts.title}</h2>
              <Show when={promptState().opts.description}>
                <p class="mt-1 text-[12px] text-fg-secondary">
                  {promptState().opts.description}
                </p>
              </Show>
            </div>
            <button
              type="button"
              class="rounded p-1 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
              onClick={cancel}
              aria-label="Cancel"
            >
              <X size={14} />
            </button>
          </header>

          <input
            ref={inputEl}
            type="text"
            spellcheck={false}
            autocomplete="off"
            class="h-9 rounded border border-border bg-bg-primary px-3 font-mono text-[13px] outline-none focus:border-primary"
            placeholder={promptState().opts.placeholder ?? ''}
            value={value()}
            onInput={(e) => setValue(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                submit();
              }
            }}
          />

          <div class="flex justify-end gap-2 pt-2">
            <button
              type="button"
              class="rounded px-3 py-1.5 text-[12px] hover:bg-bg-secondary"
              onClick={cancel}
            >
              Cancel
            </button>
            <button
              type="button"
              class="rounded bg-primary px-3 py-1.5 text-[12px] font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
              disabled={value().trim().length === 0}
              onClick={submit}
            >
              {promptState().opts.submitLabel ?? 'OK'}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}
