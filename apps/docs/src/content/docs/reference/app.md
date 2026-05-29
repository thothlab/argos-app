---
title: Desktop app reference
description: Settings, keybindings, command palette, auto-updater and crash reporter in the Argos desktop app.
---

The desktop app is intentionally thin on chrome — most of the surface is
the request editor and the response pane. The rest of this page covers
the bits that don't live inside a tab: settings, keybindings, the
command palette, the auto-updater, and the crash reporter.

Everything here is desktop-only. The CLI (`argos run`) ignores user
settings and reads only the workspace files.

## Settings

Open with <kbd>⌘</kbd>+<kbd>,</kbd> (or **Argos** menu → **Settings…**
on macOS, the workspace menu in the top-right elsewhere).

Settings persist in `settings.json` under the platform config dir:

| OS | Path |
|---|---|
| macOS | `~/Library/Application Support/argos/settings.json` |
| Linux | `~/.config/argos/settings.json` |
| Windows | `%APPDATA%\argos\settings.json` |

The file is plain JSON and safe to hand-edit; Argos reloads it on next
launch. Four tabs group the options:

### Appearance

- **Application theme** — `Light` / `Dark` / `System`. `System` follows
  your OS appearance setting and flips in real time.

### Editor

Applies to every CodeMirror surface — URL bar, body editor, script
editor.

- **Font size** — 10–20 px.
- **Tab size** — 2 / 4 / 8 spaces.
- **Line wrapping** — when off, long lines get a horizontal scrollbar.
- **Editor theme** — `Follow app` keeps the editor in sync with the
  application theme. `One Dark` forces a dark editor regardless.

### Keybindings

See [Keybindings](#keybindings) below.

### Advanced

- **Updates** — manual "Check for updates" / "Install and restart". See
  [Auto-updater](#auto-updater).
- **Backup & restore** — export the current settings to a JSON file,
  import a previously exported file, or reset everything to shipped
  defaults.
- **Diagnostics** — open the crash report log. See
  [Crash reporter](#crash-reporter).

## Keybindings

Every keyboard shortcut goes through a named action. The defaults are:

| Action | Default |
|---|---|
| Toggle command palette | <kbd>⌘</kbd>+<kbd>K</kbd> |
| Save active tab | <kbd>⌘</kbd>+<kbd>S</kbd> |
| Open settings | <kbd>⌘</kbd>+<kbd>,</kbd> |
| Toggle sidebar | <kbd>⌘</kbd>+<kbd>B</kbd> |
| Toggle lower dock | <kbd>⌘</kbd>+<kbd>J</kbd> |
| Cycle theme (light / dark / system) | <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>T</kbd> |

On Linux / Windows the <kbd>⌘</kbd> (Cmd) modifier maps to <kbd>Ctrl</kbd>.

<kbd>⌘</kbd>+<kbd>Enter</kbd> always sends the active tab's request and
is not customisable.

### Rebinding

In **Settings → Keybindings**, click the combo next to an action and
press the new combination. The capture cancels on <kbd>Esc</kbd>.

- If the new combo is already bound elsewhere, the row turns red with
  an **Override anyway** link — clicking it disables the other binding
  and assigns the combo to the current action.
- The reset icon next to an overridden action restores its default.
- **Reset all to defaults** at the bottom of the tab clears every
  override.

Overrides are stored in `settings.json` under `keybindings.<actionId>`
as a string like `"meta+shift+t"`. Set a value to `null` in the file to
disable an action entirely.

Shortcuts that include <kbd>⌘</kbd> (or <kbd>Ctrl</kbd>) fire even
while you're typing in an input. Bare-letter shortcuts are suppressed
inside editable fields so they don't shadow normal typing.

## Command palette

<kbd>⌘</kbd>+<kbd>K</kbd> (or the search button in the top bar) opens a
fuzzy jump-to-request overlay.

- Type any substring of the request name or its folder path. Tokens are
  AND-matched, so `users post` finds `Create user` under `/users`.
- <kbd>↑</kbd> / <kbd>↓</kbd> navigate, <kbd>Enter</kbd> opens or
  focuses the matching tab, <kbd>Esc</kbd> closes.
- Up to 200 matches are shown — refine the query if you don't see what
  you want.

The palette only knows about requests in the currently open workspace.
Open one first via the welcome screen or **Open workspace folder…**.

## Auto-updater

Argos pings `https://argos.thothlab.tech/api/update/{target}-{arch}/{version}`
once on every launch.

- **Up to date** — silent. Nothing in the UI.
- **Update available** — a sticky toast appears with an **Install now**
  action. The same affordance shows up in
  **Settings → Advanced → Updates** until the install completes.
- **Network or signature failure** — logged to the dev console but not
  surfaced. Closed-alpha shouldn't nag users about flaky checks.

Clicking **Install now** (or **Install and restart** in Settings)
downloads the signed bundle, applies it, and relaunches Argos. Open
unsaved tabs are written through autosave before relaunch.

### Manual check

**Settings → Advanced → Updates → Check for updates** runs the same
check and toasts the result either way (success or failure).

The manifest is a single source of truth shared with the download links
on the landing page, so the "Check for updates" result and the website
download bundle are always the same version.

## Crash reporter

If the desktop app panics, the next launch shows an opt-in modal listing
the number of pending reports and exactly what would be sent.

What gets included:

- Panic message.
- Source file and line number from the panic location.
- OS name + version, CPU architecture, Argos version.
- (Only with **Submit always**) an anonymous session ID so we can tell
  repeat crashes from the same install.

What is **not** included:

- No request URLs, headers, query params or bodies.
- No environment values or variable contents.
- No file paths from the workspace.
- No system identifiers (hostname, username, MAC, hardware id).

Choices on the modal:

- **Submit (just this once)** — sends the pending reports and asks
  again next time.
- **Submit always (anonymous session id)** — persists consent; future
  crashes upload silently.
- **Never — discard pending reports** — drops the queue and never asks
  again.

You can review previously submitted reports under
**Settings → Advanced → Diagnostics → View log…**. The same panel can
clear the local log at any time.

To change consent later, re-open the modal by triggering a fresh crash,
or clear the stored choice from your browser-style local storage:

| OS | Storage |
|---|---|
| All | Argos's local storage keys `argos:crash:consent` and `argos:crash:session_id` |

Deleting `argos:crash:consent` resets you to **ask** on next launch.
Deleting `argos:crash:session_id` rotates the anonymous id used by
**Submit always** so future reports are no longer linked to past ones.

## Native menu (macOS)

On macOS, Argos installs a standard application menu — **Argos**,
**File**, **Edit**, **View**, **Window**, **Help** — with the usual
items (Hide, Quit, Cut / Copy / Paste, Minimize, Zoom). The only Argos-
specific entry is **Argos → Settings…** (<kbd>⌘</kbd>+<kbd>,</kbd>),
which opens the same Settings modal as the in-app affordance.

On Linux and Windows there is no native menu; use the in-app top bar
instead.
