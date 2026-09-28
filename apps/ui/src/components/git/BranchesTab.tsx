/**
 * Branches: local and remote-tracking, with the operations that take a
 * branch as their subject.
 *
 * Switching with a dirty tree is offered twice: plain first, and — only
 * after git refuses — with a stash. Stashing behind the user's back is
 * how changes get lost in other clients.
 */

import { createSignal, For, Show, type JSX } from 'solid-js';

import { Check, X } from 'lucide-solid';

import { confirmAction } from '../../lib/confirm';
import { promptText } from '../../lib/prompt';
import {
  gitCheckout,
  gitCreateBranch,
  gitDeleteBranch,
  gitMerge,
  gitRebase,
  gitRemoteAdd,
  gitRemoteRemove,
  gitRemoteSetUrl,
  gitRenameBranch,
  type BranchInfo,
  type RemoteInfo,
} from '../../lib/git';
import { branches, error, remotes, run, setError, status } from '../../stores/git';

export default function BranchesTab(_props: { workspace: string }) {
  const [filter, setFilter] = createSignal('');

  const local = () => branches().filter((b) => !b.isRemote && matches(b));
  const remote = () => branches().filter((b) => b.isRemote && matches(b));

  function matches(b: BranchInfo): boolean {
    const q = filter().trim().toLowerCase();
    return q === '' || b.name.toLowerCase().includes(q);
  }

  async function create() {
    const name = await promptText({
      title: 'New branch',
      description: `Created from ${status()?.branch ?? 'HEAD'} and checked out.`,
      placeholder: 'feature/x',
      submitLabel: 'Create',
    });
    if (!name) return;
    await run((ws) => gitCreateBranch(ws, name, null));
  }

  /** Plain checkout; on refusal offer to stash and retry. */
  async function checkout(name: string) {
    const done = await run((ws) => gitCheckout(ws, name, false));
    if (done !== null) return;
    const message = error() ?? '';
    if (!/local changes|would be overwritten|Please commit/i.test(message)) return;
    const ok = await confirmAction({
      title: `Stash changes and switch to ${name}?`,
      description: `git refused the switch:\n\n${message}`,
      confirmLabel: 'Stash and switch',
    });
    if (!ok) return;
    setError(null);
    await run((ws) => gitCheckout(ws, name, true));
  }

  async function remove(b: BranchInfo) {
    const ok = await confirmAction({
      title: b.isRemote ? `Delete ${b.name} on the remote?` : `Delete branch ${b.name}?`,
      description: b.isRemote
        ? 'This deletes the branch on the remote for everyone.'
        : 'Unmerged commits on it become unreachable.',
      confirmLabel: 'Delete',
      danger: true,
    });
    if (!ok) return;
    await run((ws) => gitDeleteBranch(ws, b.name, !b.isRemote));
  }

  async function rename(b: BranchInfo) {
    const to = await promptText({
      title: `Rename ${b.name}`,
      defaultValue: b.name,
      submitLabel: 'Rename',
    });
    if (!to || to === b.name) return;
    await run((ws) => gitRenameBranch(ws, b.name, to));
  }

  async function merge(b: BranchInfo) {
    const ok = await confirmAction({
      title: `Merge ${b.name} into ${status()?.branch ?? 'HEAD'}?`,
      confirmLabel: 'Merge',
    });
    if (!ok) return;
    await run((ws) => gitMerge(ws, b.name, false));
  }

  const [addRemoteOpen, setAddRemoteOpen] = createSignal(false);
  const [addName, setAddName] = createSignal('origin');
  const [addUrl, setAddUrl] = createSignal('');

  function openAddRemote() {
    setAddName('origin');
    setAddUrl('');
    setAddRemoteOpen(true);
  }

  async function submitAddRemote() {
    const name = addName().trim();
    const url = addUrl().trim();
    if (!name || !url) return;
    setAddRemoteOpen(false);
    await run((ws) => gitRemoteAdd(ws, name, url));
  }

  async function editRemote(r: RemoteInfo) {
    const url = await promptText({
      title: `Edit remote "${r.name}"`,
      defaultValue: r.url,
      submitLabel: 'Save',
    });
    if (!url || url === r.url) return;
    await run((ws) => gitRemoteSetUrl(ws, r.name, url));
  }

  async function removeRemote(r: RemoteInfo) {
    const ok = await confirmAction({
      title: `Remove remote "${r.name}"?`,
      description: 'Local branches tracking it keep their history but lose their upstream link.',
      confirmLabel: 'Remove',
      danger: true,
    });
    if (!ok) return;
    await run((ws) => gitRemoteRemove(ws, r.name));
  }

  async function rebase(b: BranchInfo) {
    const ok = await confirmAction({
      title: `Rebase ${status()?.branch ?? 'HEAD'} onto ${b.name}?`,
      description: 'Rewrites the current branch’s commits. Force-push may be needed afterwards.',
      confirmLabel: 'Rebase',
      danger: true,
    });
    if (!ok) return;
    await run((ws) => gitRebase(ws, b.name));
  }

  return (
    <div class="flex h-full min-h-0 flex-col">
      <header class="flex shrink-0 items-center gap-2 border-b border-border px-3 py-2">
        <input
          type="text"
          spellcheck={false}
          autocomplete="off"
          class="h-7 min-w-0 flex-1 rounded border border-border bg-bg-primary px-2 font-mono text-[12px] outline-none focus:border-primary"
          placeholder="Filter branches"
          value={filter()}
          onInput={(e) => setFilter(e.currentTarget.value)}
        />
        <button
          type="button"
          class="rounded bg-primary px-2 py-1 text-[12px] font-medium text-primary-foreground hover:opacity-90"
          onClick={() => void create()}
        >
          New branch
        </button>
      </header>

      <div class="flex-1 overflow-auto scrollbar-thin">
        <Group
          title="Remotes"
          action={<Mini label="Add remote" onClick={openAddRemote} />}
        >
          <Show
            when={remotes().length > 0}
            fallback={
              <p class="px-3 py-2 text-[12px] text-fg-secondary">
                No remote configured — add one to push and pull.
              </p>
            }
          >
            <For each={remotes()}>
              {(r) => (
                <div class="group flex items-center gap-2 px-3 py-1 font-mono text-[12px] hover:bg-bg-secondary/60">
                  <span class="min-w-0 flex-1 truncate" title={r.url}>
                    {r.name} <span class="text-fg-secondary">{r.url}</span>
                  </span>
                  <span class="flex shrink-0 gap-1 opacity-0 group-hover:opacity-100">
                    <Mini label="Edit" onClick={() => void editRemote(r)} />
                    <Mini label="Remove" danger onClick={() => void removeRemote(r)} />
                  </span>
                </div>
              )}
            </For>
          </Show>
        </Group>

        <Group title="Local">
          <For each={local()}>
            {(b) => (
              <Row
                branch={b}
                onCheckout={() => void checkout(b.name)}
                onMerge={() => void merge(b)}
                onRebase={() => void rebase(b)}
                onRename={() => void rename(b)}
                onDelete={() => void remove(b)}
              />
            )}
          </For>
        </Group>

        <Group title="Remote">
          <For each={remote()}>
            {(b) => (
              <Row
                branch={b}
                onCheckout={() => void checkout(b.name)}
                onMerge={() => void merge(b)}
                onRebase={() => void rebase(b)}
                onDelete={() => void remove(b)}
              />
            )}
          </For>
        </Group>
      </div>

      <Show when={addRemoteOpen()}>
        <div
          class="pointer-events-auto fixed inset-0 z-[100] flex items-center justify-center bg-bg-primary/70"
          role="dialog"
          aria-modal="true"
          data-kb-top-layer
          onClick={(e) => {
            if (e.target === e.currentTarget) setAddRemoteOpen(false);
          }}
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              e.preventDefault();
              setAddRemoteOpen(false);
            }
          }}
        >
          <div class="flex w-[420px] flex-col gap-3 rounded-xl border border-border bg-bg-card p-5 shadow-xl">
            <header class="flex items-start justify-between gap-3">
              <h2 class="text-[14px] font-semibold">Add remote</h2>
              <button
                type="button"
                class="rounded p-1 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
                onClick={() => setAddRemoteOpen(false)}
                aria-label="Cancel"
              >
                <X size={14} />
              </button>
            </header>

            <label class="flex flex-col gap-1">
              <span class="text-[11px] text-fg-secondary">Name</span>
              <input
                type="text"
                spellcheck={false}
                autocomplete="off"
                class="h-9 rounded border border-border bg-bg-primary px-3 font-mono text-[13px] outline-none focus:border-primary"
                value={addName()}
                onInput={(e) => setAddName(e.currentTarget.value)}
              />
            </label>

            <label class="flex flex-col gap-1">
              <span class="text-[11px] text-fg-secondary">URL</span>
              <input
                ref={(el) => {
                  requestAnimationFrame(() => el?.focus());
                }}
                type="text"
                spellcheck={false}
                autocomplete="off"
                class="h-9 rounded border border-border bg-bg-primary px-3 font-mono text-[13px] outline-none focus:border-primary"
                placeholder="git@github.com:org/repo.git"
                value={addUrl()}
                onInput={(e) => setAddUrl(e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault();
                    void submitAddRemote();
                  }
                }}
              />
            </label>

            <div class="flex justify-end gap-2 pt-2">
              <button
                type="button"
                class="rounded px-3 py-1.5 text-[12px] hover:bg-bg-secondary"
                onClick={() => setAddRemoteOpen(false)}
              >
                Cancel
              </button>
              <button
                type="button"
                class="rounded bg-primary px-3 py-1.5 text-[12px] font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
                disabled={!addName().trim() || !addUrl().trim()}
                onClick={() => void submitAddRemote()}
              >
                Add
              </button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
}

function Group(props: { title: string; action?: JSX.Element; children: JSX.Element }) {
  return (
    <section>
      <h3 class="sticky top-0 z-10 flex items-center justify-between border-b border-border bg-bg-card px-3 py-1.5 text-[11px] font-medium uppercase tracking-widest text-fg-secondary">
        <span>{props.title}</span>
        <Show when={props.action}>
          <span class="normal-case tracking-normal">{props.action}</span>
        </Show>
      </h3>
      {props.children}
    </section>
  );
}

function Row(props: {
  branch: BranchInfo;
  onCheckout: () => void;
  onMerge: () => void;
  onRebase: () => void;
  onRename?: () => void;
  onDelete: () => void;
}) {
  return (
    <div class="group flex items-center gap-2 px-3 py-1 font-mono text-[12px] hover:bg-bg-secondary/60">
      <span class="w-3 shrink-0 text-primary">
        <Show when={props.branch.isCurrent}>
          <Check size={12} />
        </Show>
      </span>
      <span class="min-w-0 flex-1 truncate" title={props.branch.upstream ?? undefined}>
        {props.branch.name}
      </span>
      <Show when={props.branch.ahead > 0 || props.branch.behind > 0}>
        <span class="shrink-0 text-[11px] text-fg-secondary">
          {props.branch.ahead > 0 ? `↑${props.branch.ahead}` : ''}
          {props.branch.behind > 0 ? `↓${props.branch.behind}` : ''}
        </span>
      </Show>
      <span class="flex shrink-0 gap-1 opacity-0 group-hover:opacity-100">
        <Show when={!props.branch.isCurrent}>
          <Mini label="Checkout" onClick={props.onCheckout} />
          <Mini label="Merge" onClick={props.onMerge} />
          <Mini label="Rebase" onClick={props.onRebase} />
        </Show>
        <Show when={props.onRename}>
          <Mini label="Rename" onClick={props.onRename!} />
        </Show>
        <Show when={!props.branch.isCurrent}>
          <Mini label="Delete" danger onClick={props.onDelete} />
        </Show>
      </span>
    </div>
  );
}

function Mini(props: { label: string; danger?: boolean; onClick: () => void }) {
  return (
    <button
      type="button"
      class="rounded border border-border px-1.5 py-0.5 text-[11px]"
      classList={{
        'text-fg-secondary hover:bg-bg-card hover:text-fg-primary': !props.danger,
        'text-[var(--color-error-foreground)] hover:bg-bg-card': !!props.danger,
      }}
      onClick={props.onClick}
    >
      {props.label}
    </button>
  );
}
