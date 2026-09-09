---
title: Git panel
description: The built-in Git client — staging, commits, branches, stashes and a console.
---

An Argos workspace is a folder of plain files, so the natural way to
share and version it is the one the team already uses: git. The desktop
app has a Git client built in — the branch name in the top bar opens it.

The panel is a modal with five tabs. It operates on the repository that
contains the open workspace; when the workspace sits in a subfolder of a
larger repository, the whole repository is what you see (paths are shown
exactly as git prints them, relative to the repository root).

## How it talks to git

By shelling out to the `git` on your `PATH` — not a bundled library.
That is deliberate: your git already has your SSH keys, your credential
helper, your hooks, your commit signing and your `includeIf` config, and
a reimplementation gets those wrong long before it gets the plumbing
wrong.

Two consequences worth knowing:

- **git must be installed.** If it is not, every action reports that
  plainly instead of failing obscurely.
- **No interactive prompts.** Argos runs git with
  `GIT_TERMINAL_PROMPT=0` and no editor, because a prompt inside a
  webview would hang the app with nothing on screen. A command that
  needs a password (an HTTPS remote with no credential helper) fails
  with git's own message; configure the helper, or use SSH.

Errors are shown as git's own stderr, verbatim. When a push or a switch
is refused, the refusal is the useful text — not a paraphrase.

## Changes

Staged and unstaged lists, with the diff of the selected file beside
them. A file that is half-staged appears in both lists, because that is
what git means: the two sides have different content.

- Stage / unstage / discard per file, or all at once.
- **Hunk-level staging** — each hunk in the diff has its own button.
  Argos applies the exact patch text git printed for that hunk, so what
  gets staged is what you read.
- Commit box with an **Amend** checkbox, which pre-fills HEAD's message
  rather than silently replacing it. `Cmd`/`Ctrl`+`Enter` commits.

Discarding is destructive and asks first. Staged changes are what gets
committed — the commit button acts on the index, exactly as the list
above it shows.

## History

Commits newest first, with refs decorated, optionally across all
branches. Selecting one shows its message, its touched files and their
diffs against the first parent.

Per-commit actions: checkout (detached), branch from here, cherry-pick,
revert, and reset `soft` / `mixed` / `hard`. `hard` names what it
destroys in its confirmation.

## Branches

Local and remote-tracking branches with ahead/behind counts, filterable.
Create, checkout, merge, rebase, rename, delete.

Switching with a dirty working tree is offered twice: plainly first, and
only after git refuses does Argos offer to stash and retry. Stashing
behind your back is how changes get lost in other clients.

Deleting a remote-tracking branch deletes it **on the remote**, for
everyone — the confirmation says so.

## Stashes

Shelve the current changes (optionally including untracked files), then
apply, pop or drop an entry. `stash@{n}` names shift as entries are
dropped, so the list is reloaded after every action.

## Console

Type any git command and read its output — `status --short`,
`log --oneline -10`, `bisect start`, `worktree list`. The leading `git`
is optional; `↑` / `↓` walk the session's history.

This is the escape hatch that makes the panel a client rather than a set
of buttons: anything not modelled by a button is still one line away, in
the repository already open. There is no shell involved — the line is
split into arguments and handed to `git` directly, so pipes, globbing
and variable expansion are not available. A non-zero exit is printed,
not treated as a failure: it is the answer the command was typed for.

Commands that want an editor or a prompt fail immediately rather than
hanging — `rebase -i`, `add -i`, `commit` with no `-m`. There is no
terminal behind the panel to type into, so Argos tells git never to open
one.

## Push and pull

The header carries Fetch, Pull and Push plus the tracking counts.

Push tries the plain push first. If git refuses because the branch has
no upstream, Argos offers `-u`; if it refuses because the remote moved,
Argos offers `--force-with-lease` — each named explicitly and confirmed,
with git's refusal quoted. It never escalates on its own.

## Unfinished operations

A half-finished merge, rebase, cherry-pick or revert is detected and
shown as a banner above the tabs, with Continue / Skip / Abort — because
it changes what every other button means. Resolve the conflicting files
in your editor, stage them, then continue.

## Not in the repository yet

If the workspace is not inside a git repository, the panel offers to run
`git init` at the workspace root, and refuses to nest a second
repository inside an existing one.
