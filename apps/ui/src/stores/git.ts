/**
 * State for the built-in Git panel.
 *
 * One store rather than per-tab state because there is exactly one
 * repository in play: the one holding the open workspace. Every mutation
 * goes through [`run`], which serialises calls, surfaces git's own error
 * text and refreshes afterwards — a git UI that lets two writes overlap
 * shows a status from before the first one finished.
 */

import { createSignal } from 'solid-js';

import {
  gitBranches,
  gitIsRepo,
  gitLog,
  gitRemoteList,
  gitStashList,
  gitStatus,
  type BranchInfo,
  type CommitRow,
  type RemoteInfo,
  type RepoStatus,
  type StashEntry,
} from '../lib/git';
import { workspace } from './workspace';

export type GitTab = 'changes' | 'history' | 'branches' | 'stashes' | 'console';

const PAGE = 100;

const [gitOpen, setGitOpen] = createSignal(false);
const [gitTab, setGitTab] = createSignal<GitTab>('changes');
const [status, setStatus] = createSignal<RepoStatus | null>(null);
/**
 * Whether the workspace is a git repository at all — `null` while that
 * is still being determined. Kept separate from `status`: a `git status`
 * call can fail for reasons that have nothing to do with there being a
 * repository (a transient shell hiccup, a half-finished operation), and
 * collapsing both into one signal used to make the panel show "not a
 * git repository" — with an offer to re-init on top of an existing one —
 * whenever status merely failed to load.
 */
const [isRepo, setIsRepo] = createSignal<boolean | null>(null);
const [branches, setBranches] = createSignal<BranchInfo[]>([]);
const [remotes, setRemotes] = createSignal<RemoteInfo[]>([]);
const [stashes, setStashes] = createSignal<StashEntry[]>([]);
const [commits, setCommits] = createSignal<CommitRow[]>([]);
const [logHasMore, setLogHasMore] = createSignal(false);
const [logAllRefs, setLogAllRefs] = createSignal(false);
const [busy, setBusy] = createSignal(false);
const [error, setError] = createSignal<string | null>(null);

export {
  branches,
  busy,
  commits,
  error,
  gitOpen,
  gitTab,
  isRepo,
  logAllRefs,
  logHasMore,
  remotes,
  setError,
  setGitTab,
  stashes,
  status,
};

/** Workspace root, or `null` when the welcome screen is showing. */
export function repoWorkspace(): string | null {
  return workspace()?.root ?? null;
}

export function openGit(tab: GitTab = 'changes'): void {
  setGitTab(tab);
  setGitOpen(true);
  void refreshAll();
}

export function closeGit(): void {
  setGitOpen(false);
  setError(null);
}

/**
 * Run one git action, then refresh.
 *
 * Errors are kept in the store rather than thrown on: git's stderr is
 * the most useful thing on screen when a push is rejected, and a toast
 * that vanishes is the wrong place for a merge conflict.
 */
export async function run<T>(action: (ws: string) => Promise<T>): Promise<T | null> {
  const ws = repoWorkspace();
  if (!ws) return null;
  if (busy()) return null;
  setBusy(true);
  setError(null);
  try {
    return await action(ws);
  } catch (e) {
    setError(e instanceof Error ? e.message : String(e));
    return null;
  } finally {
    setBusy(false);
    await refreshAll();
  }
}

/** Reload everything the panel shows. Never throws. */
export async function refreshAll(): Promise<void> {
  const ws = repoWorkspace();
  if (!ws) return;

  const repoExists = await gitIsRepo(ws).catch(() => false);
  setIsRepo(repoExists);
  if (!repoExists) {
    setStatus(null);
    return;
  }

  // Independent of `git status` below: a repo with a broken HEAD or a
  // status parse edge case shouldn't also blank out branches/log/stashes.
  try {
    setStatus(await gitStatus(ws));
  } catch (e) {
    setError(e instanceof Error ? e.message : String(e));
  }
  const [b, s, r] = await Promise.all([
    gitBranches(ws).catch(() => [] as BranchInfo[]),
    gitStashList(ws).catch(() => [] as StashEntry[]),
    gitRemoteList(ws).catch(() => [] as RemoteInfo[]),
  ]);
  setBranches(b);
  setStashes(s);
  setRemotes(r);
  await loadLog(false);
}

/** Load (or extend) the history page. */
export async function loadLog(append: boolean): Promise<void> {
  const ws = repoWorkspace();
  if (!ws) return;
  const skip = append ? commits().length : 0;
  try {
    const rows = await gitLog(ws, {
      rev: logAllRefs() ? '--all' : null,
      skip,
      limit: PAGE,
    });
    setCommits(append ? [...commits(), ...rows] : rows);
    setLogHasMore(rows.length === PAGE);
  } catch {
    // A repository with no commits yet has no log; that is a state, not
    // an error worth taking over the panel.
    if (!append) {
      setCommits([]);
      setLogHasMore(false);
    }
  }
}

export function toggleLogAllRefs(): void {
  setLogAllRefs(!logAllRefs());
  void loadLog(false);
}

/** Files with staged changes / with working-tree changes. A file that is
 *  half-staged appears in both, which is what git means. */
export function stagedFiles() {
  return (status()?.files ?? []).filter((f) => f.staged);
}

export function unstagedFiles() {
  return (status()?.files ?? []).filter((f) => f.unstaged);
}
