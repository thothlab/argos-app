/**
 * Tab bar — open requests with active highlight, close, pin, new.
 *
 * Drag-and-drop reorder is intentionally minimal for T1.2: a single
 * "move to position" via primary-button drag with the tab being dragged
 * showing visual feedback. Polished interaction (cards-overflow, drag preview,
 * pin/unpin icons) lands in E8.
 */

import { For, Show } from 'solid-js';

import { Pin, Plus, X } from 'lucide-solid';

import {
  activeTabId,
  closeTab,
  moveTab,
  openNewTab,
  selectTab,
  tabs,
  togglePin,
  type HttpMethod,
} from '../stores/tabs';

const METHOD_VAR: Record<HttpMethod, string> = {
  GET: 'var(--method-get)',
  POST: 'var(--method-post)',
  PUT: 'var(--method-put)',
  PATCH: 'var(--method-patch)',
  DELETE: 'var(--method-delete)',
  HEAD: 'var(--fg-secondary)',
  OPTIONS: 'var(--fg-secondary)',
};

export default function TabBar() {
  function onDragStart(e: DragEvent, id: string) {
    if (!e.dataTransfer) return;
    e.dataTransfer.setData('text/argos-tab-id', id);
    e.dataTransfer.effectAllowed = 'move';
  }

  function onDrop(e: DragEvent, targetIndex: number) {
    e.preventDefault();
    const id = e.dataTransfer?.getData('text/argos-tab-id');
    if (!id) return;
    moveTab(id, targetIndex);
  }

  return (
    <div class="flex h-9 shrink-0 items-stretch border-b border-border bg-bg-card">
      <ul class="flex flex-1 items-stretch overflow-x-auto scrollbar-thin">
        <For each={tabs()}>
          {(tab, i) => {
            const isActive = () => tab.id === activeTabId();
            return (
              <li
                draggable={true}
                onDragStart={(e) => onDragStart(e, tab.id)}
                onDragOver={(e) => e.preventDefault()}
                onDrop={(e) => onDrop(e, i())}
                class="group relative flex shrink-0 items-center gap-2 border-r border-border px-3 text-[13px]"
                classList={{
                  'bg-bg-primary text-fg-primary': isActive(),
                  'text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary': !isActive(),
                }}
              >
                <button
                  type="button"
                  class="flex items-center gap-2 py-2"
                  onClick={() => selectTab(tab.id)}
                  onDblClick={() => togglePin(tab.id)}
                  title={tab.pinned ? 'Pinned tab — double-click to unpin' : 'Double-click to pin'}
                >
                  <span
                    class="font-mono text-[10px] font-bold"
                    style={{ color: METHOD_VAR[tab.method] }}
                  >
                    {tab.method}
                  </span>
                  <span class="max-w-[180px] truncate">{tab.title}</span>
                  <Show when={tab.dirty}>
                    <span class="h-1.5 w-1.5 rounded-full bg-primary" title="Unsaved changes" />
                  </Show>
                  <Show when={tab.pinned}>
                    <Pin size={11} class="text-fg-secondary" />
                  </Show>
                </button>

                <Show when={!tab.pinned}>
                  <button
                    type="button"
                    class="rounded p-0.5 text-fg-secondary opacity-0 hover:bg-bg-secondary hover:text-fg-primary group-hover:opacity-100"
                    classList={{ 'opacity-100': isActive() }}
                    title="Close tab"
                    onClick={(e) => {
                      e.stopPropagation();
                      closeTab(tab.id);
                    }}
                  >
                    <X size={12} />
                  </button>
                </Show>

                <Show when={isActive()}>
                  <span
                    class="absolute inset-x-0 -bottom-px h-px bg-primary"
                    aria-hidden
                  />
                </Show>
              </li>
            );
          }}
        </For>
      </ul>

      <button
        type="button"
        class="flex shrink-0 items-center justify-center px-3 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
        title="New request"
        onClick={() => openNewTab()}
      >
        <Plus size={14} />
      </button>
    </div>
  );
}
