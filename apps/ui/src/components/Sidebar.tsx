/**
 * Sidebar — workspace tree with CRUD context menu and drag-n-drop reorder.
 *
 * - Click a request → open / focus tab.
 * - Right-click on a folder → New folder / New request / Rename / Delete.
 * - Right-click on a request → Rename / Delete.
 * - Drag a node onto a folder → moves the file/folder there.
 *
 * After every mutation we kick a workspace reload so the sidebar reflects
 * disk state (the file watcher would catch it too, but the explicit reload
 * removes the visible lag).
 */

import { createSignal, For, Match, Show, Switch, type JSX } from 'solid-js';

import {
  ChevronDown,
  ChevronRight,
  FilePlus,
  FileText,
  Folder,
  FolderOpen,
  FolderPlus,
  Pencil,
  Trash2,
} from 'lucide-solid';
import { ContextMenu } from '@kobalte/core/context-menu';

import {
  treeCreateFolder,
  treeCreateRequest,
  treeDelete,
  treeMove,
  treeRename,
  workspaceReload,
} from '../lib/api';
import { setWorkspace, workspace } from '../stores/workspace';
import { activeTab, openOrFocusTabForRequest } from '../stores/tabs';
import type { TreeNode } from '../types/workspace';

export default function Sidebar() {
  return (
    <div class="flex h-full flex-col">
      <Show
        when={workspace()}
        fallback={
          <div class="p-4 font-mono text-[11px] text-fg-secondary">
            No workspace loaded.
          </div>
        }
      >
        {(ws) => (
          <>
            <div class="flex items-center justify-between px-3 py-2 text-fg-secondary">
              <span class="font-mono text-[10px] tracking-widest" title={ws().root}>
                {ws().manifest.name.toUpperCase()}
              </span>
              <div class="flex items-center gap-1">
                <button
                  type="button"
                  class="rounded p-1 hover:bg-bg-secondary hover:text-fg-primary"
                  title="New folder at workspace root"
                  onClick={async () => {
                    const ws = workspace();
                    if (!ws) return;
                    const name = window.prompt('Folder name:');
                    if (!name) return;
                    try {
                      await treeCreateFolder(collectionsRoot(ws), name);
                      await reloadWorkspace();
                    } catch (e) {
                      alert(`Could not create folder:\n\n${String(e)}`);
                    }
                  }}
                >
                  <FolderPlus size={14} />
                </button>
                <button
                  type="button"
                  class="rounded p-1 hover:bg-bg-secondary hover:text-fg-primary"
                  title="New request at workspace root"
                  onClick={async () => {
                    const ws = workspace();
                    if (!ws) return;
                    const name = window.prompt('Request name:');
                    if (!name) return;
                    try {
                      const path = await treeCreateRequest(collectionsRoot(ws), name);
                      await reloadWorkspace();
                      // Open the freshly-created request in a tab.
                      const fresh = workspace();
                      if (fresh) {
                        const node = findRequest(fresh.tree, path);
                        if (node) openOrFocusTabForRequest(node.path, node.draft);
                      }
                    } catch (e) {
                      alert(`Could not create request:\n\n${String(e)}`);
                    }
                  }}
                >
                  <FilePlus size={14} />
                </button>
              </div>
            </div>

            <ul class="flex-1 overflow-auto scrollbar-thin px-1 pb-2 text-[13px]">
              {(() => {
                const root = ws().tree;
                const children = root.kind === 'folder' ? root.children : [];
                return (
                  <Show
                    when={children.length > 0}
                    fallback={
                      <li class="px-3 py-2 font-mono text-[11px] text-fg-secondary">
                        Empty workspace. Use the buttons above to add your first folder or request.
                      </li>
                    }
                  >
                    <For each={children}>{(node) => <NodeView node={node} depth={0} />}</For>
                  </Show>
                );
              })()}
            </ul>

            <div class="border-t border-border px-3 py-2 font-mono text-[10px] text-fg-secondary">
              <span title={ws().root}>{lastSegment(ws().root)}</span> · workspace
            </div>
          </>
        )}
      </Show>
    </div>
  );
}

function NodeView(props: { node: TreeNode; depth: number }) {
  return (
    <Switch>
      <Match when={props.node.kind === 'folder'}>
        <FolderRow
          node={props.node as Extract<TreeNode, { kind: 'folder' }>}
          depth={props.depth}
        />
      </Match>
      <Match when={props.node.kind === 'request'}>
        <RequestRow
          node={props.node as Extract<TreeNode, { kind: 'request' }>}
          depth={props.depth}
        />
      </Match>
    </Switch>
  );
}

function FolderRow(props: { node: Extract<TreeNode, { kind: 'folder' }>; depth: number }) {
  const [open, setOpen] = createSignal(props.depth < 1);
  const [dragOver, setDragOver] = createSignal(false);
  const indent = () => `${0.5 + props.depth * 0.75}rem`;

  function onDragStart(e: DragEvent) {
    if (!e.dataTransfer) return;
    e.dataTransfer.setData('text/argos-node-path', props.node.path);
    e.dataTransfer.effectAllowed = 'move';
  }
  function onDragOver(e: DragEvent) {
    if (!e.dataTransfer) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    setDragOver(true);
  }
  function onDragLeave() {
    setDragOver(false);
  }
  async function onDrop(e: DragEvent) {
    e.preventDefault();
    setDragOver(false);
    const src = e.dataTransfer?.getData('text/argos-node-path');
    if (!src || src === props.node.path) return;
    try {
      await treeMove(src, props.node.path);
      await reloadWorkspace();
    } catch (err) {
      alert(`Move failed:\n\n${String(err)}`);
    }
  }

  return (
    <li>
      <ContextMenu>
        <ContextMenu.Trigger
          as="div"
          class="contents"
          draggable={true}
          onDragStart={onDragStart}
          onDragOver={onDragOver}
          onDragLeave={onDragLeave}
          onDrop={onDrop}
        >
          <button
            type="button"
            class="flex w-full items-center gap-1.5 rounded px-2 py-1 text-left hover:bg-bg-secondary"
            classList={{ 'bg-bg-secondary': dragOver() }}
            style={{ 'padding-left': indent() }}
            onClick={() => setOpen((v) => !v)}
          >
            <span class="flex h-3 w-3 shrink-0 items-center justify-center text-fg-secondary">
              {open() ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
            </span>
            {open() ? (
              <FolderOpen size={14} class="shrink-0 text-fg-secondary" />
            ) : (
              <Folder size={14} class="shrink-0 text-fg-secondary" />
            )}
            <span class="flex-1 truncate">{props.node.name}</span>
          </button>
        </ContextMenu.Trigger>
        <ContextMenu.Portal>
          <ContextMenu.Content class="z-50 min-w-48 overflow-hidden rounded-md border border-border bg-bg-card shadow-lg">
            <MenuItem
              icon={<FilePlus size={13} />}
              label="New request"
              onSelect={async () => {
                const name = window.prompt('Request name:');
                if (!name) return;
                try {
                  const path = await treeCreateRequest(props.node.path, name);
                  await reloadWorkspace();
                  setOpen(true);
                  const fresh = workspace();
                  if (fresh) {
                    const node = findRequest(fresh.tree, path);
                    if (node) openOrFocusTabForRequest(node.path, node.draft);
                  }
                } catch (e) {
                  alert(`Could not create request:\n\n${String(e)}`);
                }
              }}
            />
            <MenuItem
              icon={<FolderPlus size={13} />}
              label="New folder"
              onSelect={async () => {
                const name = window.prompt('Folder name:');
                if (!name) return;
                try {
                  await treeCreateFolder(props.node.path, name);
                  await reloadWorkspace();
                  setOpen(true);
                } catch (e) {
                  alert(`Could not create folder:\n\n${String(e)}`);
                }
              }}
            />
            <MenuSeparator />
            <MenuItem
              icon={<Pencil size={13} />}
              label="Rename"
              onSelect={async () => {
                const next = window.prompt('Folder name:', props.node.name);
                if (!next || next === props.node.name) return;
                try {
                  await treeRename(props.node.path, next);
                  await reloadWorkspace();
                } catch (e) {
                  alert(`Rename failed:\n\n${String(e)}`);
                }
              }}
            />
            <MenuItem
              icon={<Trash2 size={13} />}
              label="Delete"
              danger
              onSelect={async () => {
                if (!window.confirm(`Delete folder "${props.node.name}" and everything inside?`)) return;
                try {
                  await treeDelete(props.node.path);
                  await reloadWorkspace();
                } catch (e) {
                  alert(`Delete failed:\n\n${String(e)}`);
                }
              }}
            />
          </ContextMenu.Content>
        </ContextMenu.Portal>
      </ContextMenu>
      <Show when={open()}>
        <ul>
          <For each={props.node.children}>
            {(child) => <NodeView node={child} depth={props.depth + 1} />}
          </For>
        </ul>
      </Show>
    </li>
  );
}

const METHOD_VAR: Record<string, string> = {
  GET: 'var(--method-get)',
  POST: 'var(--method-post)',
  PUT: 'var(--method-put)',
  PATCH: 'var(--method-patch)',
  DELETE: 'var(--method-delete)',
  HEAD: 'var(--fg-secondary)',
  OPTIONS: 'var(--fg-secondary)',
};

function RequestRow(props: { node: Extract<TreeNode, { kind: 'request' }>; depth: number }) {
  const indent = () => `${1 + props.depth * 0.75}rem`;
  const isOpen = () => activeTab()?.path === props.node.path;
  const method = (): string => {
    const d = props.node.draft;
    return d.type === 'rest' ? d.method : 'GET';
  };

  function onDragStart(e: DragEvent) {
    if (!e.dataTransfer) return;
    e.dataTransfer.setData('text/argos-node-path', props.node.path);
    e.dataTransfer.effectAllowed = 'move';
  }

  return (
    <li>
      <ContextMenu>
        <ContextMenu.Trigger
          as="div"
          class="contents"
          draggable={true}
          onDragStart={onDragStart}
        >
          <button
            type="button"
            class="group flex w-full items-center gap-2 rounded px-2 py-1 text-left"
            classList={{
              'bg-bg-secondary text-fg-primary': isOpen(),
              'hover:bg-bg-secondary text-fg-secondary': !isOpen(),
            }}
            style={{ 'padding-left': indent() }}
            title={props.node.path}
            onClick={() => openOrFocusTabForRequest(props.node.path, props.node.draft)}
          >
            <FileText size={12} class="shrink-0 text-fg-secondary" />
            <span class="font-mono text-[10px] font-bold" style={{ color: METHOD_VAR[method()] }}>
              {method()}
            </span>
            <span class="flex-1 truncate">{props.node.draft.name}</span>
          </button>
        </ContextMenu.Trigger>
        <ContextMenu.Portal>
          <ContextMenu.Content class="z-50 min-w-48 overflow-hidden rounded-md border border-border bg-bg-card shadow-lg">
            <MenuItem
              icon={<Pencil size={13} />}
              label="Rename"
              onSelect={async () => {
                const next = window.prompt('Request name:', props.node.draft.name);
                if (!next || next === props.node.draft.name) return;
                try {
                  await treeRename(props.node.path, next);
                  await reloadWorkspace();
                } catch (e) {
                  alert(`Rename failed:\n\n${String(e)}`);
                }
              }}
            />
            <MenuItem
              icon={<Trash2 size={13} />}
              label="Delete"
              danger
              onSelect={async () => {
                if (!window.confirm(`Delete request "${props.node.draft.name}"?`)) return;
                try {
                  await treeDelete(props.node.path);
                  await reloadWorkspace();
                } catch (e) {
                  alert(`Delete failed:\n\n${String(e)}`);
                }
              }}
            />
          </ContextMenu.Content>
        </ContextMenu.Portal>
      </ContextMenu>
    </li>
  );
}

function MenuItem(props: {
  icon: JSX.Element;
  label: string;
  danger?: boolean;
  onSelect: () => void;
}) {
  return (
    <ContextMenu.Item
      class="flex cursor-pointer items-center gap-2 px-3 py-1.5 text-[13px] hover:bg-bg-secondary data-[highlighted]:bg-bg-secondary"
      classList={{ 'text-[var(--color-error-foreground)]': !!props.danger }}
      onSelect={props.onSelect}
    >
      <span class="text-fg-secondary">{props.icon}</span>
      <span>{props.label}</span>
    </ContextMenu.Item>
  );
}

function MenuSeparator() {
  return <ContextMenu.Separator class="my-1 h-px bg-border" />;
}

// ---- helpers --------------------------------------------------------------

function collectionsRoot(ws: NonNullable<ReturnType<typeof workspace>>): string {
  return joinPath(ws.root, ws.manifest.config.collections_dir);
}

function joinPath(...parts: string[]): string {
  return parts
    .map((p) => p.replace(/\/+$/, ''))
    .filter(Boolean)
    .join('/');
}

function lastSegment(p: string): string {
  const parts = p.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? p;
}

function findRequest(
  node: TreeNode,
  path: string,
): Extract<TreeNode, { kind: 'request' }> | null {
  if (node.kind === 'request' && node.path === path) return node;
  if (node.kind !== 'folder') return null;
  for (const child of node.children) {
    const found = findRequest(child, path);
    if (found) return found;
  }
  return null;
}

async function reloadWorkspace(): Promise<void> {
  const ws = workspace();
  if (!ws) return;
  try {
    const fresh = await workspaceReload(ws.root);
    setWorkspace(fresh);
  } catch {
    // Best-effort: file watcher will catch up shortly.
  }
}
