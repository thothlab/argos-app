/**
 * Working-tree changes: staged / unstaged lists, a diff of the selected
 * file, and the commit box.
 *
 * Selection carries the *side* it came from, because the same path can
 * be in both lists with different content — a half-staged file's diff is
 * a different diff depending on which row you clicked.
 */

import { createSignal, For, Show, type JSX } from 'solid-js';

import { FilePlus, FileMinus, RotateCcw, Undo2 } from 'lucide-solid';

import { confirmAction } from '../../lib/confirm';
import {
  gitCommit,
  gitDiscard,
  gitHeadMessage,
  gitStage,
  gitUnstage,
  type FileStatus,
} from '../../lib/git';
import { busy, run, stagedFiles, status, unstagedFiles } from '../../stores/git';
import DiffPane from './DiffPane';

type Side = 'index' | 'worktree';
type Selection = { path: string; side: Side } | null;

export default function ChangesTab(props: { workspace: string }) {
  const [selected, setSelected] = createSignal<Selection>(null);
  const [message, setMessage] = createSignal('');
  const [amend, setAmend] = createSignal(false);
  const [revision, setRevision] = createSignal(0);

  /** Diffs are re-read after every mutation — the file did change. */
  async function act(action: (ws: string) => Promise<unknown>) {
    await run(action);
    setRevision(revision() + 1);
  }

  async function toggleAmend() {
    const next = !amend();
    setAmend(next);
    // Pre-fill HEAD's message so amending edits it instead of silently
    // replacing it with whatever is in the box.
    if (next && message().trim() === '') {
      try {
        setMessage(await gitHeadMessage(props.workspace));
      } catch {
        /* no commits yet — nothing to pre-fill */
      }
    }
  }

  async function discard(paths: string[]) {
    const ok = await confirmAction({
      title:
        paths.length === 1 ? `Discard changes in ${paths[0]}?` : `Discard ${paths.length} files?`,
      description: 'Working-tree changes are thrown away. This cannot be undone.',
      confirmLabel: 'Discard',
      danger: true,
    });
    if (!ok) return;
    await act((ws) => gitDiscard(ws, paths));
    setSelected(null);
  }

  async function commit() {
    const msg = message().trim();
    if (msg === '') return;
    // Empty `paths` commits the index as it stands — the staged list is
    // exactly what the user sees above the box.
    const done = await run((ws) => gitCommit(ws, [], msg, amend()));
    if (done !== null) {
      setMessage('');
      setAmend(false);
      setSelected(null);
      setRevision(revision() + 1);
    }
  }

  const canCommit = () =>
    message().trim().length > 0 && !busy() && (stagedFiles().length > 0 || amend());

  return (
    <div class="flex h-full min-h-0">
      <div class="flex w-[42%] min-w-[280px] flex-col border-r border-border">
        <div class="flex-1 overflow-auto scrollbar-thin">
          <Section
            title="Staged"
            count={stagedFiles().length}
            action={
              <SectionButton
                label="Unstage all"
                icon={<Undo2 size={12} />}
                disabled={stagedFiles().length === 0}
                onClick={() =>
                  void act((ws) =>
                    gitUnstage(
                      ws,
                      stagedFiles().map((f) => f.path),
                    ),
                  )
                }
              />
            }
          >
            <For each={stagedFiles()}>
              {(f) => (
                <FileRow
                  file={f}
                  active={selected()?.path === f.path && selected()?.side === 'index'}
                  onSelect={() => setSelected({ path: f.path, side: 'index' })}
                  actions={
                    <RowButton
                      title="Unstage"
                      onClick={() => void act((ws) => gitUnstage(ws, [f.path]))}
                    >
                      <FileMinus size={13} />
                    </RowButton>
                  }
                />
              )}
            </For>
          </Section>

          <Section
            title="Changes"
            count={unstagedFiles().length}
            action={
              <>
                <SectionButton
                  label="Stage all"
                  icon={<FilePlus size={12} />}
                  disabled={unstagedFiles().length === 0}
                  onClick={() =>
                    void act((ws) =>
                      gitStage(
                        ws,
                        unstagedFiles().map((f) => f.path),
                      ),
                    )
                  }
                />
                <SectionButton
                  label="Discard all"
                  icon={<RotateCcw size={12} />}
                  danger
                  disabled={unstagedFiles().length === 0}
                  onClick={() => void discard(unstagedFiles().map((f) => f.path))}
                />
              </>
            }
          >
            <For each={unstagedFiles()}>
              {(f) => (
                <FileRow
                  file={f}
                  active={selected()?.path === f.path && selected()?.side === 'worktree'}
                  onSelect={() => setSelected({ path: f.path, side: 'worktree' })}
                  actions={
                    <>
                      <RowButton
                        title="Stage"
                        onClick={() => void act((ws) => gitStage(ws, [f.path]))}
                      >
                        <FilePlus size={13} />
                      </RowButton>
                      <RowButton title="Discard" danger onClick={() => void discard([f.path])}>
                        <RotateCcw size={13} />
                      </RowButton>
                    </>
                  }
                />
              )}
            </For>
          </Section>

          <Show when={(status()?.files.length ?? 0) === 0}>
            <p class="p-4 text-[12px] text-fg-secondary">Working tree clean — nothing to commit.</p>
          </Show>
        </div>

        <div class="shrink-0 border-t border-border p-3">
          <textarea
            class="h-20 w-full resize-none rounded border border-border bg-bg-primary p-2 font-mono text-[12px] outline-none scrollbar-thin focus:border-primary"
            placeholder="Commit message"
            spellcheck={false}
            autocorrect="off"
            autocapitalize="off"
            value={message()}
            onInput={(e) => setMessage(e.currentTarget.value)}
            onKeyDown={(e) => {
              // Cmd/Ctrl+Enter commits, matching the send shortcut in
              // the request editor.
              if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                void commit();
              }
            }}
          />
          <div class="mt-2 flex items-center justify-between gap-2">
            <label class="flex items-center gap-1.5 text-[11px] text-fg-secondary">
              <input
                type="checkbox"
                class="h-3.5 w-3.5 accent-primary"
                checked={amend()}
                onChange={() => void toggleAmend()}
              />
              Amend last commit
            </label>
            <button
              type="button"
              class="rounded bg-primary px-3 py-1.5 text-[12px] font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
              disabled={!canCommit()}
              onClick={() => void commit()}
            >
              {amend() ? 'Amend' : 'Commit'}
              <span class="ml-1 opacity-70">{stagedFiles().length || ''}</span>
            </button>
          </div>
        </div>
      </div>

      <div class="min-w-0 flex-1">
        <DiffPane
          workspace={props.workspace}
          path={selected()?.path ?? null}
          base={selected()?.side === 'index' ? 'index' : 'worktree'}
          revision={revision()}
          hunkActions
        />
      </div>
    </div>
  );
}

function Section(props: {
  title: string;
  count: number;
  action?: JSX.Element;
  children?: JSX.Element;
}) {
  return (
    <section>
      <header class="sticky top-0 z-10 flex items-center gap-2 border-b border-border bg-bg-card px-3 py-1.5">
        <h3 class="text-[11px] font-medium uppercase tracking-widest text-fg-secondary">
          {props.title}
        </h3>
        <span class="rounded-full bg-bg-secondary px-1.5 font-mono text-[10px] text-fg-secondary">
          {props.count}
        </span>
        <span class="ml-auto flex gap-1">{props.action}</span>
      </header>
      {props.children}
    </section>
  );
}

function SectionButton(props: {
  label: string;
  icon: JSX.Element;
  danger?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      class="flex items-center gap-1 rounded border border-border px-1.5 py-0.5 text-[11px] disabled:opacity-40"
      classList={{
        'text-fg-secondary hover:text-fg-primary hover:bg-bg-secondary': !props.danger,
        'text-[var(--color-error-foreground)] hover:bg-bg-secondary': !!props.danger,
      }}
      disabled={props.disabled}
      onClick={props.onClick}
    >
      {props.icon}
      {props.label}
    </button>
  );
}

function FileRow(props: {
  file: FileStatus;
  active: boolean;
  onSelect: () => void;
  actions?: JSX.Element;
}) {
  return (
    <div
      class="group flex cursor-pointer items-center gap-2 px-3 py-1 font-mono text-[12px]"
      classList={{
        'bg-bg-secondary': props.active,
        'hover:bg-bg-secondary/60': !props.active,
      }}
      onClick={props.onSelect}
    >
      <StatusBadge status={props.file.status} />
      <span class="min-w-0 flex-1 truncate" title={props.file.path}>
        <Show when={props.file.oldPath}>
          <span class="text-fg-secondary">{props.file.oldPath} → </span>
        </Show>
        {props.file.path}
      </span>
      <span
        class="flex shrink-0 gap-0.5 opacity-0 group-hover:opacity-100"
        onClick={(e) => e.stopPropagation()}
      >
        {props.actions}
      </span>
    </div>
  );
}

export function StatusBadge(props: { status: FileStatus['status'] }) {
  const letter = () =>
    ({
      modified: 'M',
      added: 'A',
      deleted: 'D',
      renamed: 'R',
      untracked: '?',
      conflicted: '!',
    })[props.status];
  return (
    <span
      class="w-3 shrink-0 text-center font-mono text-[11px]"
      classList={{
        'text-[var(--color-success-foreground)]':
          props.status === 'added' || props.status === 'untracked',
        'text-[var(--color-error-foreground)]':
          props.status === 'deleted' || props.status === 'conflicted',
        'text-fg-secondary': props.status === 'modified' || props.status === 'renamed',
      }}
      title={props.status}
    >
      {letter()}
    </span>
  );
}

function RowButton(props: {
  title: string;
  danger?: boolean;
  onClick: () => void;
  children: JSX.Element;
}) {
  return (
    <button
      type="button"
      class="rounded p-1 hover:bg-bg-card"
      classList={{
        'text-fg-secondary hover:text-fg-primary': !props.danger,
        'text-fg-secondary hover:text-[var(--color-error-foreground)]': !!props.danger,
      }}
      title={props.title}
      onClick={props.onClick}
    >
      {props.children}
    </button>
  );
}
