/**
 * Top bar — workspace identity + global controls + theme + hotkey hints.
 *
 * Real workspace-switcher / env-switcher / command palette wire up in E2/E3.
 * For T1.2 the elements are visible-but-inert so the layout reads correctly.
 */

import { Show } from 'solid-js';

import {
  ChevronDown,
  Command,
  Monitor,
  Moon,
  PanelBottom,
  PanelLeft,
  Search,
  Sun,
} from 'lucide-solid';

import { dockVisible, sidebarVisible, toggleDock, toggleSidebar } from '~/stores/layout';
import { cycleTheme, effectiveTheme, theme } from '~/stores/theme';
import { label } from '~/lib/hotkeys';

export default function TopBar() {
  return (
    <header class="flex h-12 shrink-0 items-center gap-2 border-b border-border bg-bg-card px-3">
      <button
        type="button"
        class="rounded-md p-1.5 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
        title={`Toggle sidebar (${label({ key: 'B', meta: true })})`}
        aria-pressed={sidebarVisible()}
        onClick={toggleSidebar}
      >
        <PanelLeft size={16} />
      </button>

      <button
        type="button"
        class="rounded-md p-1.5 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
        title={`Toggle lower dock (${label({ key: 'J', meta: true })})`}
        aria-pressed={dockVisible()}
        onClick={toggleDock}
      >
        <PanelBottom size={16} />
      </button>

      <div class="mx-1 h-5 w-px bg-border" />

      <WorkspacePicker />
      <EnvironmentPicker />

      <div class="flex-1" />

      <CommandPaletteTrigger />
      <ThemeToggle />
    </header>
  );
}

function WorkspacePicker() {
  return (
    <button
      type="button"
      class="flex items-center gap-1.5 rounded-md px-2 py-1 text-fg-primary hover:bg-bg-secondary"
      title="Switch workspace (TBD in E2)"
    >
      <span class="font-mono text-[13px]">my-project</span>
      <ChevronDown size={14} class="text-fg-secondary" />
    </button>
  );
}

function EnvironmentPicker() {
  return (
    <button
      type="button"
      class="flex items-center gap-1.5 rounded-full bg-bg-secondary px-3 py-1 text-fg-primary hover:bg-border"
      title="Switch environment (TBD in E3)"
    >
      <span class="h-1.5 w-1.5 rounded-full bg-[var(--color-success-foreground)]" aria-hidden />
      <span class="font-mono text-[12px]">production</span>
      <ChevronDown size={12} class="text-fg-secondary" />
    </button>
  );
}

function CommandPaletteTrigger() {
  return (
    <button
      type="button"
      class="flex w-72 items-center gap-2 rounded-full bg-bg-secondary px-3 py-1 text-fg-secondary hover:bg-border"
      title={`Search (${label({ key: 'K', meta: true })})`}
      // Real palette is part of T8 / E8; this is just the affordance.
    >
      <Search size={14} />
      <span class="flex-1 text-left text-[12px]">Search or jump to…</span>
      <span class="flex items-center gap-0.5 font-mono text-[11px] opacity-70">
        <Command size={11} />K
      </span>
    </button>
  );
}

function ThemeToggle() {
  return (
    <button
      type="button"
      class="flex items-center gap-1.5 rounded-md px-2 py-1.5 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary"
      title={`Cycle theme (${label({ key: 'T', meta: true, shift: true })})`}
      onClick={cycleTheme}
    >
      <Show when={theme() === 'system'} fallback={<ThemeIcon mode={effectiveTheme()} />}>
        <Monitor size={14} />
      </Show>
      <span class="font-mono text-[11px] capitalize">{theme()}</span>
    </button>
  );
}

function ThemeIcon(props: { mode: 'light' | 'dark' }) {
  return props.mode === 'dark' ? <Moon size={14} /> : <Sun size={14} />;
}
