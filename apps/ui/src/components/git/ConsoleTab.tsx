/**
 * Git console — type any `git` command and read its output.
 *
 * This is what makes the panel a client rather than a fixed set of
 * buttons: whatever the UI does not model (`bisect`, `worktree`,
 * `filter-branch`, a flag combination nobody anticipated) is still one
 * line away, in the repository already open.
 *
 * The input is split into argv by `splitShellArgs` — there is no shell
 * downstream, only `git`, so pipes and globbing were never available and
 * are not pretended at. A non-zero exit is printed, not thrown: it is
 * the answer the command was typed for.
 */

import { createSignal, For, Show } from 'solid-js';

import { gitExec } from '../../lib/git';
import { splitShellArgs } from '../../lib/git-console';
import { refreshAll, repoWorkspace } from '../../stores/git';

type Entry = { command: string; stdout: string; stderr: string; exitCode: number };

export default function ConsoleTab(_props: { workspace: string }) {
  const [input, setInput] = createSignal('');
  const [entries, setEntries] = createSignal<Entry[]>([]);
  const [history, setHistory] = createSignal<string[]>([]);
  const [historyIdx, setHistoryIdx] = createSignal(-1);
  const [running, setRunning] = createSignal(false);

  async function submit() {
    const line = input().trim();
    if (line === '' || running()) return;
    const split = splitShellArgs(line);
    if (!split.ok) {
      setEntries([...entries(), { command: line, stdout: '', stderr: split.error, exitCode: -1 }]);
      return;
    }
    if (split.args.length === 0) return;

    const ws = repoWorkspace();
    if (!ws) return;
    setRunning(true);
    try {
      const out = await gitExec(ws, split.args);
      setEntries([
        ...entries(),
        { command: line, stdout: out.stdout, stderr: out.stderr, exitCode: out.exitCode },
      ]);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setEntries([...entries(), { command: line, stdout: '', stderr: msg, exitCode: -1 }]);
    } finally {
      setRunning(false);
      setHistory([...history(), line]);
      setHistoryIdx(-1);
      setInput('');
      // The command may well have changed the repository.
      await refreshAll();
    }
  }

  /** ↑ / ↓ walk previously run commands, shell-style. */
  function recall(delta: number) {
    const h = history();
    if (h.length === 0) return;
    const cur = historyIdx() === -1 ? h.length : historyIdx();
    const next = Math.min(h.length, Math.max(0, cur + delta));
    setHistoryIdx(next === h.length ? -1 : next);
    setInput(next === h.length ? '' : h[next]!);
  }

  return (
    <div class="flex h-full min-h-0 flex-col">
      <div class="flex-1 overflow-auto scrollbar-thin bg-bg-primary p-3 font-mono text-[12px]">
        <Show when={entries().length === 0}>
          <p class="text-fg-secondary">
            Type a git command — `status --short`, `log --oneline -10`, `bisect start`. The leading
            `git` is optional. Runs in {repoWorkspace() ?? 'the workspace'}.
          </p>
        </Show>
        <For each={entries()}>
          {(e) => (
            <div class="mb-3">
              <div class="flex items-baseline gap-2">
                <span class="text-primary">$</span>
                <span class="min-w-0 flex-1 break-all">git {e.command.replace(/^git\s+/, '')}</span>
                <Show when={e.exitCode !== 0}>
                  <span class="shrink-0 text-[11px] text-[var(--color-error-foreground)]">
                    exit {e.exitCode}
                  </span>
                </Show>
              </div>
              <Show when={e.stdout}>
                <pre class="whitespace-pre-wrap break-all">{e.stdout}</pre>
              </Show>
              <Show when={e.stderr}>
                <pre class="whitespace-pre-wrap break-all text-[var(--color-error-foreground)]">
                  {e.stderr}
                </pre>
              </Show>
            </div>
          )}
        </For>
      </div>

      <div class="flex shrink-0 items-center gap-2 border-t border-border px-3 py-2">
        <span class="font-mono text-[12px] text-primary">$ git</span>
        <input
          type="text"
          spellcheck={false}
          autocomplete="off"
          autocorrect="off"
          autocapitalize="off"
          class="h-7 min-w-0 flex-1 rounded border border-border bg-bg-primary px-2 font-mono text-[12px] outline-none focus:border-primary"
          placeholder="status --short"
          value={input()}
          disabled={running()}
          onInput={(e) => setInput(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              void submit();
            } else if (e.key === 'ArrowUp') {
              e.preventDefault();
              recall(-1);
            } else if (e.key === 'ArrowDown') {
              e.preventDefault();
              recall(1);
            }
          }}
        />
        <button
          type="button"
          class="rounded border border-border px-2 py-1 text-[11px] text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
          onClick={() => setEntries([])}
        >
          Clear
        </button>
      </div>
    </div>
  );
}
