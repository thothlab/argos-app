/**
 * Inline confirmation for destructive actions.
 *
 * Same shape and the same reason as `prompt.ts`: Tauri 2's WKWebView
 * makes `window.confirm` unreliable across platforms, and a discard or a
 * force-push is exactly the button that must not silently do nothing.
 *
 * Usage:
 *   if (!(await confirmAction({ title: 'Discard changes?', danger: true }))) return;
 */

import { createSignal } from 'solid-js';

export type ConfirmOptions = {
  title: string;
  description?: string;
  /** Button label for the confirming action. Default: "Confirm". */
  confirmLabel?: string;
  /** Style the confirm button as destructive. */
  danger?: boolean;
};

type ConfirmState = {
  open: boolean;
  opts: ConfirmOptions;
  resolve: ((value: boolean) => void) | null;
};

const [state, setState] = createSignal<ConfirmState>({
  open: false,
  opts: { title: '' },
  resolve: null,
});

export const confirmState = state;

/** Resolves `true` only on an explicit confirm; Esc / Cancel give `false`. */
export function confirmAction(opts: ConfirmOptions): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    // A second request while one is open answers the first with "no"
    // rather than stranding its caller forever.
    const cur = state();
    if (cur.open && cur.resolve) cur.resolve(false);
    setState({ open: true, opts, resolve });
  });
}

/** Called by the modal. */
export function resolveConfirm(value: boolean): void {
  const cur = state();
  if (!cur.open || !cur.resolve) return;
  cur.resolve(value);
  setState({ open: false, opts: { title: '' }, resolve: null });
}
