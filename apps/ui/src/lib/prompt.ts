/**
 * Inline text-prompt helper, since Tauri 2 WebViews don't implement
 * `window.prompt` (it silently returns `null`).
 *
 * Usage:
 *   const name = await promptText({ title: 'Folder name', defaultValue: 'new-folder' });
 *   if (!name) return;
 *
 * Returns `null` if the user cancels (Esc, close button, blank
 * submit). Single global queue — only one prompt visible at a time;
 * concurrent calls await each other.
 */

import { createSignal } from 'solid-js';

export type PromptOptions = {
  /** Heading shown in the modal. */
  title: string;
  /** Optional sub-text under the title. */
  description?: string;
  /** Pre-filled value. */
  defaultValue?: string;
  /** Placeholder for empty input. */
  placeholder?: string;
  /** Button label for the submit action. Default: "OK". */
  submitLabel?: string;
};

type PromptState = {
  open: boolean;
  opts: PromptOptions;
  resolve: ((value: string | null) => void) | null;
};

const [state, setState] = createSignal<PromptState>({
  open: false,
  opts: { title: '' },
  resolve: null,
});

export const promptState = state;

let queue: Promise<unknown> = Promise.resolve();

/**
 * Show an inline text-prompt modal. Resolves with the entered text
 * (trimmed) or `null` if cancelled / blank.
 */
export function promptText(opts: PromptOptions): Promise<string | null> {
  const run = new Promise<string | null>((resolve) => {
    setState({ open: true, opts, resolve });
  });
  // Serialise concurrent calls.
  queue = queue.then(() => run.catch(() => null));
  return run;
}

/** Called by the modal — submit with the entered value or null. */
export function resolvePrompt(value: string | null): void {
  const cur = state();
  if (!cur.open || !cur.resolve) return;
  cur.resolve(value);
  setState({ open: false, opts: { title: '' }, resolve: null });
}
