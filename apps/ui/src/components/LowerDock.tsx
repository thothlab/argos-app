/**
 * Lower dock — collapsible bottom panel with a draggable top edge.
 *
 * Hosts logs, run-history, and (in 2.0) the AI assistant chat. Until the
 * concrete tabs land we render an empty placeholder with the tab strip and
 * a close button.
 */

import { createSignal, For } from 'solid-js';

import { X } from 'lucide-solid';

import { loadJSON, saveJSON } from '../lib/persist';
import { toggleDock } from '../stores/layout';

const TABS = ['Logs', 'Runs', 'Console'];

const HEIGHT_KEY = 'argos:dock:height';
const MIN_HEIGHT = 120;
const MAX_HEIGHT = 600;
const DEFAULT_HEIGHT = 224;

export default function LowerDock() {
  const [height, setHeightRaw] = createSignal<number>(
    clamp(loadJSON<number>(HEIGHT_KEY, DEFAULT_HEIGHT)),
  );

  function setHeight(h: number): void {
    const clamped = clamp(h);
    setHeightRaw(clamped);
    saveJSON(HEIGHT_KEY, clamped);
  }

  function onPointerDown(downEvt: PointerEvent) {
    downEvt.preventDefault();
    const startY = downEvt.clientY;
    const startHeight = height();

    const onMove = (e: PointerEvent) => {
      const dy = e.clientY - startY;
      // Drag UP (negative dy) → grow the dock.
      setHeight(startHeight - dy);
    };
    const onUp = () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
    document.body.style.cursor = 'row-resize';
    document.body.style.userSelect = 'none';
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
  }

  return (
    <div
      class="flex shrink-0 flex-col border-t border-border bg-bg-card"
      style={{ height: `${height()}px` }}
    >
      <div
        role="separator"
        aria-orientation="horizontal"
        aria-label="Resize lower dock"
        class="relative h-px w-full cursor-row-resize bg-border hover:bg-primary"
        onPointerDown={onPointerDown}
      >
        <span class="absolute inset-x-0 -top-1 -bottom-1" />
      </div>

      <div class="flex h-8 shrink-0 items-center justify-between border-b border-border pl-2 pr-1">
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

function clamp(h: number): number {
  if (Number.isNaN(h)) return DEFAULT_HEIGHT;
  return Math.max(MIN_HEIGHT, Math.min(MAX_HEIGHT, Math.round(h)));
}
