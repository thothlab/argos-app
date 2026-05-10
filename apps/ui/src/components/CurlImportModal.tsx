/**
 * Modal that takes a pasted `curl` command and opens a fresh tab
 * pre-filled from the parsed request. Powered by the
 * `curl_to_request` Tauri command (which delegates to
 * `argos_core::codegen::curl::from_curl`).
 */

import { createSignal, Show } from 'solid-js';
import { Dialog } from '@kobalte/core/dialog';

import { curlToRequest } from '../lib/api';
import { openTabFromHttpRequest } from '../stores/tabs';

export default function CurlImportModal(props: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
}) {
  const [text, setText] = createSignal('');
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);

  async function doImport() {
    const input = text().trim();
    if (!input) {
      setError('Paste a curl command first.');
      return;
    }
    setError(null);
    setBusy(true);
    try {
      const req = await curlToRequest(input);
      const title = guessTitle(req.url) ?? 'Imported';
      openTabFromHttpRequest(req, title);
      setText('');
      props.onOpenChange(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay class="fixed inset-0 z-50 bg-black/50" />
        <Dialog.Content class="fixed left-1/2 top-1/2 z-50 w-[640px] max-w-[90vw] -translate-x-1/2 -translate-y-1/2 rounded-md border border-border bg-bg-card p-4 shadow-xl">
          <Dialog.Title class="text-[14px] font-medium text-fg-primary">
            Import from cURL
          </Dialog.Title>
          <Dialog.Description class="mt-1 text-[11px] text-fg-secondary">
            Paste a `curl` command. Multi-line snippets with `\` continuations
            are accepted.
          </Dialog.Description>

          <textarea
            class="mt-3 h-40 w-full resize-none rounded border border-border bg-bg-secondary p-3 font-mono text-[12px] outline-none focus:border-primary scrollbar-thin"
            placeholder={`curl https://api.example.com/users \\\n  -H 'Accept: application/json'`}
            value={text()}
            spellcheck={false}
            autocomplete="off"
            autocorrect="off"
            onInput={(e) => setText(e.currentTarget.value)}
          />

          <Show when={error()}>
            <div class="mt-2 rounded border border-error bg-error/10 px-2 py-1 text-[12px] text-error">
              {error()}
            </div>
          </Show>

          <div class="mt-3 flex justify-end gap-2">
            <button
              type="button"
              class="rounded border border-border px-3 py-1 text-[12px] hover:bg-bg-secondary"
              onClick={() => props.onOpenChange(false)}
            >
              Cancel
            </button>
            <button
              type="button"
              class="rounded bg-primary px-3 py-1 text-[12px] text-white hover:opacity-90 disabled:opacity-50"
              disabled={busy()}
              onClick={doImport}
            >
              {busy() ? 'Importing…' : 'Import'}
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog>
  );
}

function guessTitle(url: string): string | null {
  try {
    const u = new URL(url);
    const last = u.pathname.split('/').filter(Boolean).pop();
    if (last) return last;
    return u.host || null;
  } catch {
    return null;
  }
}
