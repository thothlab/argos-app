/**
 * Sidebar — workspace tree placeholder.
 *
 * Full collection / folder / request CRUD lands in E2 (workspace + format).
 * For T1.2 we render a static skeleton that matches the layout density and
 * spacing called out in `docs/06_designer_specification.md`.
 */

import { For } from 'solid-js';

import { Folder, FolderOpen, Plus } from 'lucide-solid';

const PLACEHOLDER_TREE = [
  { name: 'Users', open: true, count: 4 },
  { name: 'Orders', open: false, count: 7 },
  { name: 'Auth', open: false, count: 3 },
  { name: 'Billing', open: false, count: 2 },
];

export default function Sidebar() {
  return (
    <div class="flex h-full flex-col">
      <div class="flex items-center justify-between px-3 py-2 text-fg-secondary">
        <span class="font-mono text-[10px] tracking-widest">COLLECTIONS</span>
        <button
          type="button"
          class="rounded p-1 hover:bg-bg-secondary hover:text-fg-primary"
          title="New collection (TBD in E2)"
        >
          <Plus size={14} />
        </button>
      </div>

      <ul class="flex-1 overflow-auto scrollbar-thin px-2 pb-2 text-[13px]">
        <For each={PLACEHOLDER_TREE}>
          {(node) => (
            <li>
              <button
                type="button"
                class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-bg-secondary"
              >
                {node.open ? (
                  <FolderOpen size={14} class="text-fg-secondary" />
                ) : (
                  <Folder size={14} class="text-fg-secondary" />
                )}
                <span class="flex-1 truncate">{node.name}</span>
                <span class="font-mono text-[10px] text-fg-secondary">{node.count}</span>
              </button>
            </li>
          )}
        </For>
      </ul>

      <div class="border-t border-border px-3 py-2 font-mono text-[10px] text-fg-secondary">
        my-project · workspace
      </div>
    </div>
  );
}
