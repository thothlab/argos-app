/**
 * Opt-in modal shown on startup when there are pending crash reports
 * and the user hasn't yet committed to a preference (`consent: 'ask'`).
 *
 * Three buttons — "Submit", "Submit always", "Never" — map to the
 * choices documented in `lib/crashes.ts::applyConsent`.
 */

import { Bug, X } from 'lucide-solid';
import { Show } from 'solid-js';

import { applyConsent, closeCrashModal, crashModalPending } from '../lib/crashes';

export default function CrashReportConsentModal() {
  return (
    <Show when={crashModalPending() > 0}>
      <div
        class="fixed inset-0 z-50 flex items-center justify-center bg-bg-primary/70"
        role="dialog"
        aria-modal="true"
        aria-labelledby="crash-modal-title"
      >
        <div class="flex w-[440px] flex-col gap-4 rounded-xl border border-border bg-bg-card p-5 shadow-xl">
          <header class="flex items-center justify-between">
            <div class="flex items-center gap-2">
              <Bug size={16} class="text-fg-error" />
              <h2 id="crash-modal-title" class="text-[14px] font-semibold">
                Send crash report?
              </h2>
            </div>
            <button
              type="button"
              class="rounded p-1 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
              onClick={() => closeCrashModal()}
              aria-label="Close — decide later"
            >
              <X size={14} />
            </button>
          </header>

          <p class="text-[13px] leading-relaxed text-fg-secondary">
            Argos crashed during the last session. We have{' '}
            <strong class="text-fg-primary">{crashModalPending()}</strong> pending report
            {crashModalPending() === 1 ? '' : 's'}.
          </p>

          <p class="text-[12px] leading-relaxed text-fg-secondary">
            What gets sent: the panic message, source file + line, OS and Argos version,
            and (only if you choose "Submit always") an anonymous session id so we can
            tell repeat crashes from the same install. <strong>No URLs, no headers,
            no body content</strong>, no system identifiers beyond what's listed.
          </p>

          <div class="flex flex-col gap-2 pt-2">
            <button
              type="button"
              class="rounded bg-primary px-3 py-2 text-[13px] font-medium text-primary-foreground hover:opacity-90"
              onClick={() => void applyConsent('just-once')}
            >
              Submit (just this once)
            </button>
            <button
              type="button"
              class="rounded border border-border px-3 py-2 text-[13px] font-medium hover:bg-bg-secondary"
              onClick={() => void applyConsent('always')}
            >
              Submit always (anonymous session id)
            </button>
            <button
              type="button"
              class="rounded px-3 py-2 text-[12px] text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
              onClick={() => void applyConsent('never')}
            >
              Never — discard pending reports
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}
