/**
 * Lower dock — collapsible bottom panel.
 *
 * Hosts logs, run-history, and (in 2.0) the AI assistant chat. Until the
 * concrete tabs land we render an empty placeholder with the tab strip and a
 * close button.
 */

import { For } from 'solid-js';

import { X } from 'lucide-solid';

import { toggleDock } from '~/stores/layout';

const TABS = ['Logs', 'Runs', 'Console'];

export default function LowerDock() {
  return (
    <div class="flex h-56 shrink-0 flex-col border-t border-border bg-bg-card">
      <div class="flex h-8 items-center justify-between border-b border-border pl-2 pr-1">
        <ul class="flex items-stretch text-[12px]">
          <For each={TABS}>
            {(label, i) => (
              <li>
                <button
                  type="button"
                  class="px-3 py-1.5 text-fg-secondary hover:text-fg-primary"
                  classList={{
                    'border-b-2 border-primary text-fg-primary': i() === 0,
                  }}
                >
                  {label}
                </button>
              </li>
            )}
          </For>
        </ul>
        <button
          type="button"
          class="rounded p-1 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
          title="Hide dock"
          onClick={toggleDock}
        >
          <X size={14} />
        </button>
      </div>

      <div class="flex-1 overflow-auto scrollbar-thin p-3 font-mono text-[12px] text-fg-secondary">
        Logs / runs / console output appears here. Wired up alongside the
        request editor in T1.3.
      </div>
    </div>
  );
}
