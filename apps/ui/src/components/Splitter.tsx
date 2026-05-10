/**
 * Horizontal splitter that divides the active-tab area between request and
 * response panes (T1.2.3).
 *
 * The split is stored as a percentage and clamped at both ends. We also
 * apply pixel `min-width` to each pane so neither side collapses far enough
 * to overlap its own controls (the URL bar / status row).
 */

import { createSignal, type JSX } from 'solid-js';

import { loadJSON, saveJSON } from '../lib/persist';

export type SplitterProps = {
  left: () => JSX.Element;
  right: () => JSX.Element;
};

const STORAGE_KEY = 'argos:splitter:request';
const MIN_PCT = 30;
const MAX_PCT = 70;
const MIN_PANE_PX = 360;

const [splitPct, setSplitPctRaw] = createSignal(loadJSON<number>(STORAGE_KEY, 50));

function setSplitPct(pct: number): void {
  const clamped = Math.max(MIN_PCT, Math.min(MAX_PCT, pct));
  setSplitPctRaw(clamped);
  saveJSON(STORAGE_KEY, clamped);
}

export default function Splitter(props: SplitterProps) {
  let containerRef: HTMLDivElement | undefined;

  function onPointerDown(e: PointerEvent) {
    if (!containerRef) return;
    e.preventDefault();
    const rect = containerRef.getBoundingClientRect();
    const onMove = (mv: PointerEvent) => {
      const x = mv.clientX - rect.left;
      const pct = (x / rect.width) * 100;
      setSplitPct(pct);
    };
    const onUp = () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
  }

  return (
    <div ref={containerRef} class="flex h-full w-full overflow-hidden">
      <div
        class="flex h-full overflow-hidden"
        style={{ width: `${splitPct()}%`, 'min-width': `${MIN_PANE_PX}px` }}
      >
        {props.left()}
      </div>
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize request / response"
        class="relative w-px shrink-0 cursor-col-resize bg-border hover:bg-primary"
        onPointerDown={onPointerDown}
      >
        <span class="absolute inset-y-0 -left-1 -right-1" />
      </div>
      <div
        class="flex h-full flex-1 overflow-hidden"
        style={{ 'min-width': `${MIN_PANE_PX}px` }}
      >
        {props.right()}
      </div>
    </div>
  );
}
