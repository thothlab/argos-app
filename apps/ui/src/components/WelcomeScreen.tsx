/**
 * Welcome screen — shown when no workspace is open.
 *
 * Mirrors the Pencil mockup in `06_designer_specification.md` §3.3.1:
 *   - Open existing workspace folder
 *   - Create new workspace
 *   - Recent workspaces (sorted by last-opened, files-on-disk filtered out)
 *   - Import (placeholder until E6 lands)
 */

import { createResource, For, Show } from 'solid-js';

import { FilePlus, FolderOpen, Sparkles } from 'lucide-solid';
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';

import { workspaceCreate, workspaceListRecent, workspaceOpen } from '../lib/api';
import { isTauri } from '../lib/tauri';
import { setWorkspace } from '../stores/workspace';
import type { RecentEntry } from '../types/workspace';

export default function WelcomeScreen() {
  const [recents, { refetch }] = createResource<RecentEntry[]>(async () => {
    if (!isTauri()) return [];
    try {
      return await workspaceListRecent();
    } catch {
      return [];
    }
  });

  async function pickAndOpen() {
    if (!isTauri()) return;
    const picked = await openDialog({ directory: true, multiple: false });
    if (typeof picked !== 'string') return;
    try {
      const ws = await workspaceOpen(picked);
      setWorkspace(ws);
    } catch (e) {
      alert(`Could not open workspace:\n\n${String(e)}`);
    }
  }

  async function pickAndCreate() {
    if (!isTauri()) return;
    const picked = await saveDialog({
      title: 'Choose a folder for the new workspace',
      defaultPath: 'my-argos-workspace',
    });
    if (typeof picked !== 'string') return;

    // Use the directory name as the human workspace name.
    const segments = picked.split(/[\\/]/);
    const name = segments[segments.length - 1] || 'untitled';
    try {
      const ws = await workspaceCreate(picked, name);
      setWorkspace(ws);
    } catch (e) {
      alert(`Could not create workspace:\n\n${String(e)}`);
    }
  }

  async function openRecent(entry: RecentEntry) {
    try {
      const ws = await workspaceOpen(entry.path);
      setWorkspace(ws);
      refetch();
    } catch (e) {
      alert(`Could not open workspace at ${entry.path}:\n\n${String(e)}`);
    }
  }

  return (
    <div class="flex h-full w-full flex-col items-center justify-center gap-8 px-8 py-12 text-center">
      <header class="flex flex-col items-center gap-2">
        <h1 class="font-mono text-5xl font-bold tracking-tight">Argos</h1>
        <p class="text-fg-secondary">A fast, git-native API client</p>
      </header>

      <div class="flex w-full max-w-md flex-col overflow-hidden rounded-2xl border border-border bg-bg-card shadow-sm">
        <CtaRow
          icon={<FolderOpen size={20} />}
          label="Open workspace folder"
          hint="⌘O"
          onClick={pickAndOpen}
        />
        <Divider />
        <CtaRow
          icon={<FilePlus size={20} />}
          label="Create new workspace"
          hint="⌘N"
          onClick={pickAndCreate}
        />
        <Divider />
        <CtaRow
          icon={<Sparkles size={20} class="text-primary" />}
          label="Try sample workspace"
          hint=""
          onClick={() => {
            alert('Sample workspaces ship after E2 alpha — bundling a tiny Postman-flavoured demo into the binary.');
          }}
        />
      </div>

      <Show when={(recents()?.length ?? 0) > 0}>
        <section class="flex w-full max-w-md flex-col gap-2 text-left">
          <h2 class="font-mono text-[10px] uppercase tracking-widest text-fg-secondary">Recent</h2>
          <ul class="flex flex-col gap-1">
            <For each={recents() ?? []}>
              {(entry) => (
                <li>
                  <button
                    type="button"
                    class="flex w-full items-center gap-3 rounded px-3 py-2 text-left text-[13px] hover:bg-bg-secondary"
                    onClick={() => openRecent(entry)}
                  >
                    <FolderOpen size={14} class="text-fg-secondary" />
                    <span class="flex-1 truncate font-mono">{entry.path}</span>
                    <span class="text-[11px] text-fg-secondary">{formatRelative(entry.last_opened_ms)}</span>
                  </button>
                </li>
              )}
            </For>
          </ul>
        </section>
      </Show>

      <Show when={!isTauri()}>
        <p class="max-w-md text-[12px] text-fg-secondary">
          Running in the browser without the Tauri shell — workspace
          operations need the desktop binary. Use{' '}
          <code class="font-mono">make tauri-dev</code> to launch.
        </p>
      </Show>
    </div>
  );
}

function CtaRow(props: {
  icon: ReturnType<typeof FolderOpen>;
  label: string;
  hint: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      class="flex w-full items-center gap-4 px-4 py-3 text-left hover:bg-bg-secondary"
      onClick={props.onClick}
    >
      {props.icon}
      <span class="flex-1 text-[15px] font-medium text-fg-primary">{props.label}</span>
      <Show when={props.hint}>
        <span class="font-mono text-[11px] text-fg-secondary">{props.hint}</span>
      </Show>
    </button>
  );
}

function Divider() {
  return <div class="h-px w-full bg-border" aria-hidden />;
}

function formatRelative(ts: number): string {
  const diffMs = Date.now() - ts;
  const min = Math.floor(diffMs / 60_000);
  if (min < 1) return 'just now';
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const d = Math.floor(hr / 24);
  if (d < 30) return `${d}d ago`;
  return new Date(ts).toLocaleDateString();
}
