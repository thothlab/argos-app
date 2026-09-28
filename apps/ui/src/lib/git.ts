/**
 * Typed wrappers over the `git_*` Tauri commands.
 *
 * Mirrors `crates/desktop/src-tauri/src/git/` one-to-one. Every call
 * carries the workspace path: the backend is stateless and rediscovers
 * the repository root each time, so nothing here has to be invalidated
 * when the user opens a different workspace.
 *
 * Paths in both directions are **repo-relative**, exactly as git prints
 * them — which is not necessarily workspace-relative when the workspace
 * sits in a subfolder of a larger repository (`workspacePrefix` says
 * how far down).
 */

import { invokeCommand } from './tauri';

export type FileState = 'modified' | 'added' | 'deleted' | 'renamed' | 'untracked' | 'conflicted';

export type FileStatus = {
  path: string;
  status: FileState;
  oldPath?: string;
  staged: boolean;
  unstaged: boolean;
};

/** An unfinished merge / rebase / cherry-pick / revert. */
export type Operation = 'none' | 'merge' | 'rebase' | 'cherryPick' | 'revert';

export type RepoStatus = {
  repoPath: string;
  /** Workspace path relative to the repo root; `''` when they coincide. */
  workspacePrefix: string;
  branch: string;
  upstream: string | null;
  ahead: number;
  behind: number;
  detached: boolean;
  operation: Operation;
  userEmail: string | null;
  files: FileStatus[];
};

export type BranchInfo = {
  name: string;
  isRemote: boolean;
  isCurrent: boolean;
  upstream: string | null;
  ahead: number;
  behind: number;
};

export type RefKind = 'head' | 'local' | 'remote' | 'tag';
export type RefLabel = { name: string; kind: RefKind };

export type CommitRow = {
  id: string;
  shortId: string;
  summary: string;
  author: string;
  email: string;
  /** Author date, Unix seconds. */
  timestamp: number;
  parents: string[];
  refs: RefLabel[];
};

export type CommitDetails = { row: CommitRow; body: string; files: FileStatus[] };

export type DiffLine = {
  origin: ' ' | '+' | '-';
  content: string;
  oldNo: number | null;
  newNo: number | null;
};

export type Hunk = { header: string; lines: DiffLine[]; patch: string };

export type FileDiff = { path: string; binary: boolean; hunks: Hunk[] };

export type StashEntry = { name: string; message: string; timestamp: number };

export type RemoteInfo = { name: string; url: string };

export type RawOutput = { stdout: string; stderr: string; exitCode: number };

/** `worktree` = unstaged changes, `index` = staged, or a commit id. */
export type DiffBase = 'worktree' | 'index' | (string & {});

export async function gitIsRepo(workspace: string): Promise<boolean> {
  return invokeCommand<boolean>('git_is_repo', { workspace });
}

export async function gitInit(workspace: string): Promise<RepoStatus> {
  return invokeCommand<RepoStatus>('git_init', { workspace });
}

export async function gitStatus(workspace: string): Promise<RepoStatus> {
  return invokeCommand<RepoStatus>('git_status', { workspace });
}

export async function gitStage(workspace: string, paths: string[]): Promise<void> {
  return invokeCommand<void>('git_stage', { workspace, paths });
}

export async function gitUnstage(workspace: string, paths: string[]): Promise<void> {
  return invokeCommand<void>('git_unstage', { workspace, paths });
}

/** Destructive — throws away working-tree changes. Confirm first. */
export async function gitDiscard(workspace: string, paths: string[]): Promise<void> {
  return invokeCommand<void>('git_discard', { workspace, paths });
}

export async function gitApplyPatch(
  workspace: string,
  patch: string,
  cached: boolean,
  reverse: boolean,
): Promise<void> {
  return invokeCommand<void>('git_apply_patch', { workspace, patch, cached, reverse });
}

export async function gitCommit(
  workspace: string,
  paths: string[],
  message: string,
  amend: boolean,
): Promise<void> {
  return invokeCommand<void>('git_commit', { workspace, paths, message, amend });
}

export async function gitHeadMessage(workspace: string): Promise<string> {
  return invokeCommand<string>('git_head_message', { workspace });
}

export async function gitBranches(workspace: string): Promise<BranchInfo[]> {
  return invokeCommand<BranchInfo[]>('git_branches', { workspace });
}

export async function gitCreateBranch(
  workspace: string,
  name: string,
  from: string | null = null,
): Promise<void> {
  return invokeCommand<void>('git_create_branch', { workspace, name, from });
}

export async function gitCheckout(workspace: string, name: string, stash: boolean): Promise<void> {
  return invokeCommand<void>('git_checkout', { workspace, name, stash });
}

export async function gitDeleteBranch(
  workspace: string,
  name: string,
  force: boolean,
): Promise<void> {
  return invokeCommand<void>('git_delete_branch', { workspace, name, force });
}

export async function gitRenameBranch(workspace: string, from: string, to: string): Promise<void> {
  return invokeCommand<void>('git_rename_branch', { workspace, from, to });
}

export async function gitMerge(workspace: string, name: string, noFf: boolean): Promise<void> {
  return invokeCommand<void>('git_merge', { workspace, name, noFf });
}

export async function gitRebase(
  workspace: string,
  onto: string,
  autosquash = false,
): Promise<void> {
  return invokeCommand<void>('git_rebase', { workspace, onto, autosquash });
}

export async function gitCherryPick(workspace: string, commit: string): Promise<void> {
  return invokeCommand<void>('git_cherry_pick', { workspace, commit });
}

export async function gitRevert(workspace: string, commit: string): Promise<void> {
  return invokeCommand<void>('git_revert', { workspace, commit });
}

/** `hard` discards working-tree changes — name it in the confirmation. */
export async function gitReset(
  workspace: string,
  commit: string,
  mode: 'soft' | 'mixed' | 'hard',
): Promise<void> {
  return invokeCommand<void>('git_reset', { workspace, commit, mode });
}

export async function gitOperationStep(
  workspace: string,
  action: 'continue' | 'abort' | 'skip',
): Promise<void> {
  return invokeCommand<void>('git_operation_step', { workspace, action });
}

export type PushMode = 'normal' | 'upstream' | 'force' | 'force-hard';

export async function gitPush(workspace: string, mode: PushMode): Promise<void> {
  return invokeCommand<void>('git_push', { workspace, mode });
}

export async function gitPull(workspace: string, rebase: boolean): Promise<void> {
  return invokeCommand<void>('git_pull', { workspace, rebase });
}

export async function gitFetch(workspace: string): Promise<void> {
  return invokeCommand<void>('git_fetch', { workspace });
}

export async function gitRemoteList(workspace: string): Promise<RemoteInfo[]> {
  return invokeCommand<RemoteInfo[]>('git_remote_list', { workspace });
}

export async function gitRemoteAdd(workspace: string, name: string, url: string): Promise<void> {
  return invokeCommand<void>('git_remote_add', { workspace, name, url });
}

export async function gitRemoteSetUrl(
  workspace: string,
  name: string,
  url: string,
): Promise<void> {
  return invokeCommand<void>('git_remote_set_url', { workspace, name, url });
}

export async function gitRemoteRemove(workspace: string, name: string): Promise<void> {
  return invokeCommand<void>('git_remote_remove', { workspace, name });
}

export async function gitStashList(workspace: string): Promise<StashEntry[]> {
  return invokeCommand<StashEntry[]>('git_stash_list', { workspace });
}

export async function gitStashPush(
  workspace: string,
  message: string,
  includeUntracked: boolean,
): Promise<void> {
  return invokeCommand<void>('git_stash_push', { workspace, message, includeUntracked });
}

export async function gitStashAction(
  workspace: string,
  name: string,
  action: 'apply' | 'pop' | 'drop',
): Promise<void> {
  return invokeCommand<void>('git_stash_action', { workspace, name, action });
}

export async function gitLog(
  workspace: string,
  opts: { rev?: string | null; skip?: number; limit?: number; path?: string | null } = {},
): Promise<CommitRow[]> {
  return invokeCommand<CommitRow[]>('git_log', {
    workspace,
    rev: opts.rev ?? null,
    skip: opts.skip ?? 0,
    limit: opts.limit ?? 100,
    path: opts.path ?? null,
  });
}

export async function gitCommitDetails(workspace: string, id: string): Promise<CommitDetails> {
  return invokeCommand<CommitDetails>('git_commit_details', { workspace, id });
}

export async function gitDiff(workspace: string, path: string, base: DiffBase): Promise<FileDiff> {
  return invokeCommand<FileDiff>('git_diff', { workspace, path, base });
}

/** Arbitrary `git` argv — the console panel's escape hatch. */
export async function gitExec(workspace: string, args: string[]): Promise<RawOutput> {
  return invokeCommand<RawOutput>('git_exec', { workspace, args });
}
