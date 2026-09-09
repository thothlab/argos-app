/**
 * History: commit list on the left, the selected commit's message and
 * touched files on the right, a file's diff below.
 *
 * Actions on a commit are the ones that need a commit as their subject —
 * checkout (detached), branch-from-here, cherry-pick, revert, reset. Each
 * destructive one goes through a confirmation naming what it destroys.
 */

import { createResource, createSignal, For, Show } from 'solid-js';

import { confirmAction } from '../../lib/confirm';
import { promptText } from '../../lib/prompt';
import {
  gitCherryPick,
  gitCheckout,
  gitCommitDetails,
  gitCreateBranch,
  gitReset,
  gitRevert,
  type CommitRow,
  type RefLabel,
} from '../../lib/git';
import { commits, loadLog, logAllRefs, logHasMore, run, toggleLogAllRefs } from '../../stores/git';
import { StatusBadge } from './ChangesTab';
import DiffPane from './DiffPane';

export default function HistoryTab(props: { workspace: string }) {
  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [selectedFile, setSelectedFile] = createSignal<string | null>(null);

  const [details] = createResource(
    () => (selectedId() ? ([props.workspace, selectedId()!] as const) : null),
    ([ws, id]) => gitCommitDetails(ws, id),
  );

  function select(row: CommitRow) {
    setSelectedId(row.id);
    setSelectedFile(null);
  }

  async function resetTo(id: string, mode: 'soft' | 'mixed' | 'hard') {
    const ok = await confirmAction({
      title: `Reset ${mode} to ${id.slice(0, 8)}?`,
      description:
        mode === 'hard'
          ? 'Moves the branch and throws away every working-tree change. This cannot be undone.'
          : 'Moves the branch; working-tree files are kept.',
      confirmLabel: `Reset ${mode}`,
      danger: mode === 'hard',
    });
    if (!ok) return;
    await run((ws) => gitReset(ws, id, mode));
  }

  async function branchFrom(id: string) {
    const name = await promptText({
      title: 'New branch from this commit',
      placeholder: 'feature/x',
      submitLabel: 'Create',
    });
    if (!name) return;
    await run((ws) => gitCreateBranch(ws, name, id));
  }

  return (
    <div class="flex h-full min-h-0">
      <div class="flex w-[45%] min-w-[320px] flex-col border-r border-border">
        <header class="flex shrink-0 items-center gap-2 border-b border-border px-3 py-1.5">
          <label class="flex items-center gap-1.5 text-[11px] text-fg-secondary">
            <input
              type="checkbox"
              class="h-3.5 w-3.5 accent-primary"
              checked={logAllRefs()}
              onChange={toggleLogAllRefs}
            />
            All branches
          </label>
        </header>

        <div class="flex-1 overflow-auto scrollbar-thin">
          <For each={commits()}>
            {(row) => (
              <div
                class="cursor-pointer border-b border-border/50 px-3 py-1.5"
                classList={{
                  'bg-bg-secondary': selectedId() === row.id,
                  'hover:bg-bg-secondary/60': selectedId() !== row.id,
                }}
                onClick={() => select(row)}
              >
                <div class="flex items-baseline gap-2">
                  <span class="shrink-0 font-mono text-[11px] text-fg-secondary">
                    {row.shortId}
                  </span>
                  <span class="min-w-0 flex-1 truncate text-[12px]">{row.summary}</span>
                </div>
                <div class="mt-0.5 flex items-center gap-2 text-[11px] text-fg-secondary">
                  <span class="truncate">{row.author}</span>
                  <span>{formatDate(row.timestamp)}</span>
                  <For each={row.refs}>{(r) => <RefChip label={r} />}</For>
                </div>
              </div>
            )}
          </For>

          <Show when={logHasMore()}>
            <button
              type="button"
              class="w-full py-2 text-[11px] text-fg-secondary hover:bg-bg-secondary"
              onClick={() => void loadLog(true)}
            >
              Load more
            </button>
          </Show>

          <Show when={commits().length === 0}>
            <p class="p-4 text-[12px] text-fg-secondary">No commits yet.</p>
          </Show>
        </div>
      </div>

      <div class="flex min-w-0 flex-1 flex-col">
        <Show
          when={details()}
          fallback={<p class="p-4 text-[12px] text-fg-secondary">Select a commit.</p>}
        >
          <div class="shrink-0 border-b border-border p-3">
            <p class="text-[13px] font-medium">{details()!.row.summary}</p>
            <p class="mt-1 text-[11px] text-fg-secondary">
              {details()!.row.author} &lt;{details()!.row.email}&gt; ·{' '}
              {formatDate(details()!.row.timestamp)} ·{' '}
              <span class="font-mono">{details()!.row.shortId}</span>
            </p>
            <Show when={details()!.body}>
              <pre class="mt-2 whitespace-pre-wrap font-mono text-[11px] text-fg-secondary">
                {details()!.body}
              </pre>
            </Show>

            <div class="mt-2 flex flex-wrap gap-1">
              <Action
                label="Checkout"
                onClick={() => void run((ws) => gitCheckout(ws, details()!.row.id, false))}
              />
              <Action label="Branch from here" onClick={() => void branchFrom(details()!.row.id)} />
              <Action
                label="Cherry-pick"
                onClick={() => void run((ws) => gitCherryPick(ws, details()!.row.id))}
              />
              <Action
                label="Revert"
                onClick={() => void run((ws) => gitRevert(ws, details()!.row.id))}
              />
              <Action label="Reset soft" onClick={() => void resetTo(details()!.row.id, 'soft')} />
              <Action
                label="Reset mixed"
                onClick={() => void resetTo(details()!.row.id, 'mixed')}
              />
              <Action
                label="Reset hard"
                danger
                onClick={() => void resetTo(details()!.row.id, 'hard')}
              />
            </div>
          </div>

          <div class="max-h-[30%] shrink-0 overflow-auto border-b border-border scrollbar-thin">
            <For each={details()!.files}>
              {(f) => (
                <div
                  class="flex cursor-pointer items-center gap-2 px-3 py-1 font-mono text-[12px]"
                  classList={{
                    'bg-bg-secondary': selectedFile() === f.path,
                    'hover:bg-bg-secondary/60': selectedFile() !== f.path,
                  }}
                  onClick={() => setSelectedFile(f.path)}
                >
                  <StatusBadge status={f.status} />
                  <span class="truncate">{f.path}</span>
                </div>
              )}
            </For>
          </div>

          <div class="min-h-0 flex-1">
            <DiffPane
              workspace={props.workspace}
              path={selectedFile()}
              base={selectedId() ?? 'worktree'}
              revision={0}
            />
          </div>
        </Show>
      </div>
    </div>
  );
}

function RefChip(props: { label: RefLabel }) {
  return (
    <span
      class="shrink-0 rounded px-1 font-mono text-[10px]"
      classList={{
        'bg-primary/20 text-primary': props.label.kind === 'head',
        'bg-bg-secondary text-fg-secondary':
          props.label.kind === 'local' || props.label.kind === 'remote',
        'bg-[var(--color-success)]/20 text-[var(--color-success-foreground)]':
          props.label.kind === 'tag',
      }}
    >
      {props.label.name}
    </span>
  );
}

function Action(props: { label: string; danger?: boolean; onClick: () => void }) {
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

/** Local time, minute precision — a git UI never needs seconds. */
function formatDate(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleString(undefined, {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}
