/**
 * Renders the inline text-prompt modal driven by `lib/prompt.ts`.
 * Mounted once at App root.
 */

import { createEffect, createSignal, Show } from 'solid-js';
import { X } from 'lucide-solid';

import { promptState, resolvePrompt } from '../lib/prompt';

export default function PromptModal() {
  const [value, setValue] = createSignal('');

  // Reset value when a fresh prompt opens. Focus is handled via the
  // input's ref callback (fires when the element mounts) — that's
  // more reliable than queueMicrotask, which can race the Show
  // block's content rendering.
  createEffect(() => {
    const s = promptState();
    if (s.open) {
      setValue(s.opts.defaultValue ?? '');
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
        class="pointer-events-auto fixed inset-0 z-[100] flex items-center justify-center bg-bg-primary/70"
        role="dialog"
        aria-modal="true"
        // Marks this overlay as a Kobalte top layer so any underlying
        // Kobalte Dialog (e.g. EnvironmentEditor) does not treat clicks
        // inside the prompt as outside-interaction and auto-close.
        data-kb-top-layer
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
            ref={(el) => {
              // Ref callbacks fire after the element is in the DOM.
              // Focus + select-all so the user can immediately type
              // over a default value (e.g. for Rename).
              requestAnimationFrame(() => {
                el?.focus();
                el?.select();
              });
            }}
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
