/**
 * The Git panel: one modal over the whole app, tabbed.
 *
 * A modal rather than a sidebar section because a git session is a mode —
 * you stage, commit and push, then go back to sending requests — and
 * because it keeps the request editor's layout untouched.
 *
 * The header is the part that is always true regardless of tab: which
 * branch, how far ahead/behind, whether an operation is half-finished,
 * and the three remote buttons.
 */

import { createSignal, For, Match, Show, Switch, type JSX } from 'solid-js';
import { Dialog } from '@kobalte/core/dialog';
import { ArrowDown, ArrowUp, GitBranch, Loader2, RefreshCw, X } from 'lucide-solid';

import { confirmAction } from '../../lib/confirm';
import {
  gitFetch,
  gitInit,
  gitOperationStep,
  gitPull,
  gitPush,
  type PushMode,
} from '../../lib/git';
import {
  busy,
  closeGit,
  error,
  gitOpen,
  gitTab,
  repoWorkspace,
  run,
  setError,
  setGitTab,
  status,
  type GitTab,
} from '../../stores/git';
import BranchesTab from './BranchesTab';
import ChangesTab from './ChangesTab';
import ConsoleTab from './ConsoleTab';
import HistoryTab from './HistoryTab';
import StashesTab from './StashesTab';

const TABS: Array<{ id: GitTab; label: string }> = [
  { id: 'changes', label: 'Changes' },
  { id: 'history', label: 'History' },
  { id: 'branches', label: 'Branches' },
  { id: 'stashes', label: 'Stashes' },
  { id: 'console', label: 'Console' },
];

export default function GitModal() {
  const [initBusy, setInitBusy] = createSignal(false);
  const ws = () => repoWorkspace();

  /** Plain push first; escalate only after git refuses, by name. */
  async function push() {
    const done = await run((w) => gitPush(w, 'normal'));
    if (done !== null) return;
    const message = error() ?? '';
    let mode: PushMode | null = null;
    let title = '';
    if (/no upstream branch|set-upstream/i.test(message)) {
      mode = 'upstream';
      title = 'Push and set the upstream branch?';
    } else if (/rejected|non-fast-forward|fetch first/i.test(message)) {
      mode = 'force';
      title = 'Force-push with lease?';
    }
    if (!mode) return;
    const ok = await confirmAction({
      title,
      description: `git refused the push:\n\n${message}`,
      confirmLabel: mode === 'force' ? 'Force-push' : 'Push',
      danger: mode === 'force',
    });
    if (!ok) return;
    setError(null);
    await run((w) => gitPush(w, mode));
  }

  async function init() {
    const target = ws();
    if (!target) return;
    setInitBusy(true);
    try {
      await gitInit(target);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setInitBusy(false);
    }
  }

  return (
    <Dialog open={gitOpen()} onOpenChange={(v) => (v ? undefined : closeGit())}>
      <Dialog.Portal>
        <Dialog.Overlay class="fixed inset-0 z-50 bg-black/50" />
        <Dialog.Content class="fixed left-1/2 top-1/2 z-50 flex h-[80vh] w-[1100px] max-w-[95vw] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-md border border-border bg-bg-card shadow-xl">
          <header class="flex shrink-0 items-center gap-2 border-b border-border px-3 py-2">
            <GitBranch size={14} class="shrink-0 text-fg-secondary" />
            <Dialog.Title class="flex min-w-0 items-baseline gap-2 text-[13px] font-medium">
              <span class="truncate font-mono">
                {status()?.branch ?? '—'}
                <Show when={status()?.detached}>
                  <span class="ml-1 text-[11px] text-fg-secondary">(detached)</span>
                </Show>
              </span>
              <Show when={status()?.upstream}>
                <span class="truncate text-[11px] font-normal text-fg-secondary">
                  → {status()!.upstream}
                </span>
              </Show>
              <Show when={(status()?.ahead ?? 0) > 0}>
                <span class="text-[11px] font-normal text-fg-secondary">↑{status()!.ahead}</span>
              </Show>
              <Show when={(status()?.behind ?? 0) > 0}>
                <span class="text-[11px] font-normal text-fg-secondary">↓{status()!.behind}</span>
              </Show>
            </Dialog.Title>

            <Show when={busy()}>
              <Loader2 size={13} class="shrink-0 animate-spin text-fg-secondary" />
            </Show>

            <div class="ml-auto flex shrink-0 items-center gap-1">
              <HeaderButton
                label="Fetch"
                icon={<RefreshCw size={13} />}
                onClick={() => void run((w) => gitFetch(w))}
              />
              <HeaderButton
                label="Pull"
                icon={<ArrowDown size={13} />}
                onClick={() => void run((w) => gitPull(w, false))}
              />
              <HeaderButton label="Push" icon={<ArrowUp size={13} />} onClick={() => void push()} />
              <button
                type="button"
                class="rounded p-1.5 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
                aria-label="Close"
                onClick={closeGit}
              >
                <X size={14} />
              </button>
            </div>
          </header>

          <Show when={status()?.operation && status()!.operation !== 'none'}>
            <OperationBanner />
          </Show>

          <Show when={error()}>
            <div class="flex shrink-0 items-start gap-2 border-b border-border bg-[var(--color-error)]/10 px-3 py-2">
              <pre class="min-w-0 flex-1 whitespace-pre-wrap font-mono text-[11px] text-[var(--color-error-foreground)]">
                {error()}
              </pre>
              <button
                type="button"
                class="shrink-0 rounded p-0.5 text-fg-secondary hover:text-fg-primary"
                aria-label="Dismiss"
                onClick={() => setError(null)}
              >
                <X size={12} />
              </button>
            </div>
          </Show>

          <Show
            when={status()}
            fallback={
              <div class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
                <p class="text-[13px] text-fg-secondary">
                  This workspace is not inside a git repository.
                </p>
                <button
                  type="button"
                  class="rounded bg-primary px-3 py-1.5 text-[12px] font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
                  disabled={initBusy() || !ws()}
                  onClick={() => void init()}
                >
                  Initialise a repository here
                </button>
              </div>
            }
          >
            <nav class="flex h-9 shrink-0 items-end border-b border-border px-3">
              <For each={TABS}>
                {(t) => (
                  <button
                    type="button"
                    class="relative px-3 py-2 text-[13px]"
                    classList={{
                      'text-fg-primary': gitTab() === t.id,
                      'text-fg-secondary hover:text-fg-primary': gitTab() !== t.id,
                    }}
                    onClick={() => setGitTab(t.id)}
                  >
                    {t.label}
                    <Show when={gitTab() === t.id}>
                      <span class="absolute inset-x-3 -bottom-px h-0.5 bg-primary" aria-hidden />
                    </Show>
                  </button>
                )}
              </For>
            </nav>

            <div class="min-h-0 flex-1">
              <Switch>
                <Match when={gitTab() === 'changes'}>
                  <ChangesTab workspace={ws()!} />
                </Match>
                <Match when={gitTab() === 'history'}>
                  <HistoryTab workspace={ws()!} />
                </Match>
                <Match when={gitTab() === 'branches'}>
                  <BranchesTab workspace={ws()!} />
                </Match>
                <Match when={gitTab() === 'stashes'}>
                  <StashesTab workspace={ws()!} />
                </Match>
                <Match when={gitTab() === 'console'}>
                  <ConsoleTab workspace={ws()!} />
                </Match>
              </Switch>
            </div>
          </Show>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog>
  );
}

/**
 * A half-finished merge / rebase / cherry-pick / revert. Shown above the
 * tabs because it changes what every other button means — committing
 * during a rebase is not the same act as committing normally.
 */
function OperationBanner() {
  const name = () =>
    ({ merge: 'Merge', rebase: 'Rebase', cherryPick: 'Cherry-pick', revert: 'Revert', none: '' })[
      status()!.operation
    ];
  return (
    <div class="flex shrink-0 items-center gap-2 border-b border-border bg-primary/10 px-3 py-2">
      <span class="text-[12px]">{name()} in progress — resolve conflicts, then continue.</span>
      <span class="ml-auto flex gap-1">
        <HeaderButton
          label="Continue"
          onClick={() => void run((w) => gitOperationStep(w, 'continue'))}
        />
        <HeaderButton label="Skip" onClick={() => void run((w) => gitOperationStep(w, 'skip'))} />
        <HeaderButton label="Abort" onClick={() => void run((w) => gitOperationStep(w, 'abort'))} />
      </span>
    </div>
  );
}

function HeaderButton(props: { label: string; icon?: JSX.Element; onClick: () => void }) {
  return (
    <button
      type="button"
      class="flex items-center gap-1 rounded border border-border px-2 py-1 text-[12px] text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary disabled:opacity-50"
      disabled={busy()}
      onClick={props.onClick}
    >
      {props.icon}
      {props.label}
    </button>
  );
}
