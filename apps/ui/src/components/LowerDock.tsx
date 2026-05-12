/**
 * Lower dock — collapsible bottom panel with a draggable top edge.
 *
 * Hosts:
 * - Runs — per-tab run history (T1.5).
 * - Logs / Console — placeholders, wired up alongside scripting in E4.
 */

import { createSignal, For, Match, Show, Switch } from 'solid-js';

import { X } from 'lucide-solid';

import { loadJSON, saveJSON } from '../lib/persist';
import { toggleDock } from '../stores/layout';
import { errorLogEntries } from '../stores/error-log';
import { runsFor } from '../stores/runs';
import { activeTabId } from '../stores/tabs';

import ErrorLogView from './ErrorLogView';
import RunHistoryView from './RunHistoryView';

type DockTab = 'runs' | 'logs' | 'console';

const HEIGHT_KEY = 'argos:dock:height';
const MIN_HEIGHT = 120;
const MAX_HEIGHT = 600;
const DEFAULT_HEIGHT = 224;

export default function LowerDock() {
  const [activeDockTab, setActiveDockTab] = createSignal<DockTab>('runs');
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

  const runCount = (): number => {
    const id = activeTabId();
    return id ? runsFor(id).length : 0;
  };

  const TABS: Array<{ id: DockTab; label: string; badge?: () => number; disabled?: boolean }> = [
    { id: 'runs', label: 'Runs', badge: runCount },
    { id: 'logs', label: 'Logs', badge: () => errorLogEntries().length },
    { id: 'console', label: 'Console', disabled: true },
  ];

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
            {(t) => (
              <li>
                <button
                  type="button"
                  class="flex items-center gap-1.5 px-3 py-1.5"
                  classList={{
                    'text-fg-primary border-b-2 border-primary': activeDockTab() === t.id,
                    'text-fg-secondary hover:text-fg-primary':
                      activeDockTab() !== t.id && !t.disabled,
                    'cursor-not-allowed text-fg-secondary opacity-50': !!t.disabled,
                  }}
                  disabled={t.disabled}
                  onClick={() => !t.disabled && setActiveDockTab(t.id)}
                  title={t.disabled ? 'Wired up alongside scripting (E4)' : undefined}
                >
                  <span>{t.label}</span>
                  <Show when={t.badge && t.badge()! > 0}>
                    <span class="rounded-full bg-bg-secondary px-1.5 font-mono text-[10px] text-fg-secondary">
                      {t.badge!()}
                    </span>
                  </Show>
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

      <div class="flex-1 overflow-auto scrollbar-thin">
        <Switch>
          <Match when={activeDockTab() === 'runs'}>
            <RunHistoryView />
          </Match>
          <Match when={activeDockTab() === 'logs'}>
            <ErrorLogView />
          </Match>
          <Match when={activeDockTab() === 'console'}>
            <Placeholder>console.log() / bru.log() output appears here. Wired up in E4.</Placeholder>
          </Match>
        </Switch>
      </div>
    </div>
  );
}

function Placeholder(props: { children: string }) {
  return (
    <div class="p-3 font-mono text-[12px] text-fg-secondary">{props.children}</div>
  );
}

function clamp(h: number): number {
  if (Number.isNaN(h)) return DEFAULT_HEIGHT;
  return Math.max(MIN_HEIGHT, Math.min(MAX_HEIGHT, Math.round(h)));
}
