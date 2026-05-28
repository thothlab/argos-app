/**
 * Open/close state for the Crash reports inspector modal.
 * Lives separately from the crash flow logic in [[crashes.ts]].
 */

import { createSignal } from 'solid-js';

const [open, setOpen] = createSignal(false);

export { open as crashLogOpen };

export function openCrashLog(): void {
  setOpen(true);
}

export function closeCrashLog(): void {
  setOpen(false);
}
