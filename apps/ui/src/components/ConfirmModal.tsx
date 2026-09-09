/**
 * Renders the confirmation modal driven by `lib/confirm.ts`.
 * Mounted once at App root, next to `PromptModal`.
 */

import { Show } from 'solid-js';
import { X } from 'lucide-solid';

import { confirmState, resolveConfirm } from '../lib/confirm';

export default function ConfirmModal() {
  return (
    <Show when={confirmState().open}>
      <div
        class="pointer-events-auto fixed inset-0 z-[100] flex items-center justify-center bg-bg-primary/70"
        role="dialog"
        aria-modal="true"
        // Same top-layer marker as PromptModal: without it an underlying
        // Kobalte Dialog (the Git panel) treats a click in here as an
        // outside interaction and closes itself.
        data-kb-top-layer
        onClick={(e) => {
          if (e.target === e.currentTarget) resolveConfirm(false);
        }}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            resolveConfirm(false);
          }
        }}
      >
        <div class="flex w-[420px] flex-col gap-3 rounded-xl border border-border bg-bg-card p-5 shadow-xl">
          <header class="flex items-start justify-between gap-3">
            <div class="flex-1">
              <h2 class="text-[14px] font-semibold">{confirmState().opts.title}</h2>
              <Show when={confirmState().opts.description}>
                <p class="mt-1 whitespace-pre-line text-[12px] text-fg-secondary">
                  {confirmState().opts.description}
                </p>
              </Show>
            </div>
            <button
              type="button"
              class="rounded p-1 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
              onClick={() => resolveConfirm(false)}
              aria-label="Cancel"
            >
              <X size={14} />
            </button>
          </header>

          <div class="flex justify-end gap-2 pt-2">
            <button
              type="button"
              class="rounded px-3 py-1.5 text-[12px] hover:bg-bg-secondary"
              onClick={() => resolveConfirm(false)}
            >
              Cancel
            </button>
            <button
              type="button"
              // Focus lands on the confirming button, but the modal is
              // opened by an explicit click — never by a keystroke that
              // could then be repeated into a confirmation.
              ref={(el) => requestAnimationFrame(() => el?.focus())}
              class="rounded px-3 py-1.5 text-[12px] font-medium text-primary-foreground hover:opacity-90"
              classList={{
                'bg-[var(--color-error)]': !!confirmState().opts.danger,
                'bg-primary': !confirmState().opts.danger,
              }}
              onClick={() => resolveConfirm(true)}
            >
              {confirmState().opts.confirmLabel ?? 'Confirm'}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}
