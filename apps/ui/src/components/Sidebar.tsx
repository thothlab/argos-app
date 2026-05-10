/**
 * Sidebar — workspace tree.
 *
 * Renders the loaded workspace's collection tree. Folders expand/collapse
 * via the chevron; clicking a request opens (or focuses) a tab for it.
 *
 * For T2.3 v0.1 the tree is read-only — folder/request CRUD ships in the
 * next slice (context menu, drag-n-drop). Save still works for whatever
 * the user opens.
 */

import { createSignal, For, Match, Show, Switch } from 'solid-js';

import { ChevronDown, ChevronRight, FileText, Folder, FolderOpen } from 'lucide-solid';

import { workspace } from '../stores/workspace';
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
                        Empty workspace. Add requests on disk and reload.
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
  const indent = () => `${0.5 + props.depth * 0.75}rem`;

  return (
    <li>
      <button
        type="button"
        class="flex w-full items-center gap-1.5 rounded px-2 py-1 text-left hover:bg-bg-secondary"
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

  return (
    <li>
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
    </li>
  );
}

function lastSegment(p: string): string {
  const parts = p.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? p;
}
