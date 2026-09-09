/**
 * Stashes: shelve the current changes, then apply, pop or drop an entry.
 *
 * `stash@{n}` names shift when an entry is dropped, so every action is
 * issued against the name from the list that was just refreshed — the
 * store reloads after each mutation for exactly this reason.
 */

import { createSignal, For, Show } from 'solid-js';

import { confirmAction } from '../../lib/confirm';
import { gitStashAction, gitStashPush, type StashEntry } from '../../lib/git';
import { busy, run, stashes, status } from '../../stores/git';

export default function StashesTab(_props: { workspace: string }) {
  const [message, setMessage] = createSignal('');
  const [untracked, setUntracked] = createSignal(true);

  const dirty = () => (status()?.files.length ?? 0) > 0;

  async function push() {
    const done = await run((ws) => gitStashPush(ws, message(), untracked()));
    if (done !== null) setMessage('');
  }

  async function drop(entry: StashEntry) {
    const ok = await confirmAction({
      title: `Drop ${entry.name}?`,
      description: entry.message,
      confirmLabel: 'Drop',
      danger: true,
    });
    if (!ok) return;
    await run((ws) => gitStashAction(ws, entry.name, 'drop'));
  }

  return (
    <div class="flex h-full min-h-0 flex-col">
      <header class="flex shrink-0 items-center gap-2 border-b border-border px-3 py-2">
        <input
          type="text"
          spellcheck={false}
          autocomplete="off"
          autocorrect="off"
          class="h-7 min-w-0 flex-1 rounded border border-border bg-bg-primary px-2 font-mono text-[12px] outline-none focus:border-primary"
          placeholder="Stash message (optional)"
          value={message()}
          onInput={(e) => setMessage(e.currentTarget.value)}
        />
        <label class="flex shrink-0 items-center gap-1.5 text-[11px] text-fg-secondary">
          <input
            type="checkbox"
            class="h-3.5 w-3.5 accent-primary"
            checked={untracked()}
            onChange={(e) => setUntracked(e.currentTarget.checked)}
          />
          Include untracked
        </label>
        <button
          type="button"
          class="rounded bg-primary px-2 py-1 text-[12px] font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
          disabled={!dirty() || busy()}
          title={dirty() ? undefined : 'Nothing to stash — the working tree is clean'}
          onClick={() => void push()}
        >
          Stash changes
        </button>
      </header>

      <div class="flex-1 overflow-auto scrollbar-thin">
        <For each={stashes()}>
          {(entry) => (
            <div class="group flex items-center gap-2 border-b border-border/50 px-3 py-1.5 text-[12px]">
              <span class="shrink-0 font-mono text-[11px] text-fg-secondary">{entry.name}</span>
              <span class="min-w-0 flex-1 truncate">{entry.message}</span>
              <span class="flex shrink-0 gap-1 opacity-0 group-hover:opacity-100">
                <Mini
                  label="Apply"
                  onClick={() => void run((ws) => gitStashAction(ws, entry.name, 'apply'))}
                />
                <Mini
                  label="Pop"
                  onClick={() => void run((ws) => gitStashAction(ws, entry.name, 'pop'))}
                />
                <Mini label="Drop" danger onClick={() => void drop(entry)} />
              </span>
            </div>
          )}
        </For>
        <Show when={stashes().length === 0}>
          <p class="p-4 text-[12px] text-fg-secondary">No stashes.</p>
        </Show>
      </div>
    </div>
  );
}

function Mini(props: { label: string; danger?: boolean; onClick: () => void }) {
  return (
    <button
      type="button"
      class="rounded border border-border px-1.5 py-0.5 text-[11px]"
      classList={{
        'text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary': !props.danger,
        'text-[var(--color-error-foreground)] hover:bg-bg-secondary': !!props.danger,
      }}
      onClick={props.onClick}
    >
      {props.label}
    </button>
  );
}
