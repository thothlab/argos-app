/**
 * Unified diff view for one file, with per-hunk actions.
 *
 * The hunk's `patch` field is the exact text git printed for it, so
 * staging a single hunk is `git apply --cached` on that text rather than
 * a reconstruction — see the `Hunk` docs on the Rust side.
 */

import { createResource, For, Show } from 'solid-js';

import { gitApplyPatch, gitDiff, type DiffBase, type FileDiff } from '../../lib/git';
import { run } from '../../stores/git';

export type DiffPaneProps = {
  workspace: string;
  path: string | null;
  base: DiffBase;
  /** Bump to force a reload after the tree changed underneath. */
  revision: number;
  /** Hunk-level staging only makes sense for working-tree / index diffs. */
  hunkActions?: boolean;
};

export default function DiffPane(props: DiffPaneProps) {
  const [diff] = createResource(
    () =>
      props.path ? ([props.workspace, props.path, props.base, props.revision] as const) : null,
    async ([ws, path, base]): Promise<FileDiff> => gitDiff(ws, path, base),
  );

  async function applyHunk(patch: string, cached: boolean, reverse: boolean) {
    await run((ws) => gitApplyPatch(ws, patch, cached, reverse));
  }

  return (
    <div class="h-full overflow-auto scrollbar-thin bg-bg-primary font-mono text-[12px]">
      <Show
        when={props.path}
        fallback={<p class="p-4 text-[12px] text-fg-secondary">Select a file to see its diff.</p>}
      >
        <Show when={!diff.loading} fallback={<p class="p-4 text-fg-secondary">Loading…</p>}>
          <Show
            when={diff()}
            fallback={<p class="p-4 text-[var(--color-error-foreground)]">Diff unavailable.</p>}
          >
            <Show
              when={!diff()!.binary}
              fallback={<p class="p-4 text-fg-secondary">Binary file — no text diff.</p>}
            >
              <Show
                when={diff()!.hunks.length > 0}
                fallback={<p class="p-4 text-fg-secondary">No changes in this file.</p>}
              >
                <For each={diff()!.hunks}>
                  {(hunk) => (
                    <div class="border-b border-border">
                      <div class="flex items-center justify-between gap-2 bg-bg-secondary px-3 py-1">
                        <span class="truncate text-fg-secondary">{hunk.header}</span>
                        <Show when={props.hunkActions}>
                          <span class="flex shrink-0 gap-1">
                            <Show when={props.base === 'worktree'}>
                              <HunkButton
                                label="Stage hunk"
                                onClick={() => void applyHunk(hunk.patch, true, false)}
                              />
                            </Show>
                            <Show when={props.base === 'index'}>
                              <HunkButton
                                label="Unstage hunk"
                                onClick={() => void applyHunk(hunk.patch, true, true)}
                              />
                            </Show>
                          </span>
                        </Show>
                      </div>
                      <For each={hunk.lines}>
                        {(line) => (
                          <div
                            class="flex whitespace-pre"
                            classList={{
                              'bg-[var(--color-success)]/10': line.origin === '+',
                              'bg-[var(--color-error)]/10': line.origin === '-',
                            }}
                          >
                            <span class="w-12 shrink-0 select-none px-2 text-right text-fg-secondary">
                              {line.oldNo ?? ''}
                            </span>
                            <span class="w-12 shrink-0 select-none px-2 text-right text-fg-secondary">
                              {line.newNo ?? ''}
                            </span>
                            <span class="w-4 shrink-0 select-none text-fg-secondary">
                              {line.origin}
                            </span>
                            <span class="min-w-0 flex-1 pr-3">{line.content}</span>
                          </div>
                        )}
                      </For>
                    </div>
                  )}
                </For>
              </Show>
            </Show>
          </Show>
        </Show>
      </Show>
    </div>
  );
}

function HunkButton(props: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      class="rounded border border-border px-1.5 py-0.5 text-[11px] text-fg-secondary hover:bg-bg-card hover:text-fg-primary"
      onClick={props.onClick}
    >
      {props.label}
    </button>
  );
}
