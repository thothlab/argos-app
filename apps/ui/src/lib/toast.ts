/**
 * Toast helpers — non-blocking notifications that replace
 * `window.alert` for fire-and-forget feedback.
 *
 * Use:
 *   notify.success('Imported 23 requests in 4 folders.');
 *   notify.error('Import failed', e.message);
 *
 * For confirmations and save-failure-must-not-disappear cases, keep
 * the existing `window.alert` — toasts auto-dismiss and shouldn't
 * carry messages that *require* an action.
 */

import { toaster } from '@kobalte/core/toast';

import { ToastView } from '../components/Toaster';

type Variant = 'info' | 'success' | 'error';

function show(variant: Variant, title: string, description?: string): void {
  toaster.show((props) =>
    ToastView({
      toastId: props.toastId,
      variant,
      title,
      description,
    }),
  );
}

export const notify = {
  info: (title: string, description?: string) => show('info', title, description),
  success: (title: string, description?: string) => show('success', title, description),
  error: (title: string, description?: string) => show('error', title, description),
};

/** Convenience for catch handlers — formats an unknown thrown value
 *  as `(title, message)` and fires an error toast. */
export function notifyError(title: string, e: unknown): void {
  const description = e instanceof Error ? e.message : String(e);
  notify.error(title, description);
}
