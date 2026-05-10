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
  LogOut,
  Monitor,
  Moon,
  PanelBottom,
  PanelLeft,
  Search,
  Sun,
} from 'lucide-solid';
import { DropdownMenu } from '@kobalte/core/dropdown-menu';
import { Select } from '@kobalte/core/select';

import { dockVisible, sidebarVisible, toggleDock, toggleSidebar } from '../stores/layout';
import { cycleTheme, effectiveTheme, theme } from '../stores/theme';
import { activeEnvName, setActiveEnvName } from '../stores/active-env';
import { setWorkspace, workspace } from '../stores/workspace';
import { closeAllTabs } from '../stores/tabs';
import { workspaceClose } from '../lib/api';
import { label } from '../lib/hotkeys';

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
  const ws = workspace;

  async function close() {
    closeAllTabs();
    setWorkspace(null);
    try {
      await workspaceClose();
    } catch {
      // Best-effort — frontend has already left the workspace either way.
    }
  }

  return (
    <DropdownMenu>
      <DropdownMenu.Trigger
        class="flex items-center gap-1.5 rounded-md px-2 py-1 text-fg-primary hover:bg-bg-secondary"
        title={ws()?.root ?? 'No workspace'}
      >
        <span class="font-mono text-[13px]">{ws()?.manifest.name ?? '—'}</span>
        <ChevronDown size={14} class="text-fg-secondary" />
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content class="z-50 min-w-56 overflow-hidden rounded-md border border-border bg-bg-card shadow-lg">
          <div class="border-b border-border px-3 py-2 font-mono text-[10px] text-fg-secondary">
            <div class="truncate">{ws()?.root}</div>
          </div>
          <DropdownMenu.Item
            class="flex cursor-pointer items-center gap-2 px-3 py-1.5 text-[13px] hover:bg-bg-secondary data-[highlighted]:bg-bg-secondary"
            onSelect={close}
          >
            <LogOut size={14} class="text-fg-secondary" />
            <span>Close workspace</span>
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu>
  );
}

const NO_ENV = '__no_env__';

function EnvironmentPicker() {
  const ws = workspace;
  const options = (): string[] => {
    const list = ws()?.environments.map((e) => e.name) ?? [];
    return [NO_ENV, ...list];
  };

  const current = (): string => activeEnvName() ?? NO_ENV;

  return (
    <Show
      when={ws()}
      fallback={
        <span class="font-mono text-[12px] text-fg-secondary">no env</span>
      }
    >
      <Select<string>
        value={current()}
        onChange={(v) => setActiveEnvName(v && v !== NO_ENV ? v : null)}
        options={options()}
        itemComponent={(itemProps) => (
          <Select.Item
            item={itemProps.item}
            class="flex cursor-pointer items-center gap-2 px-3 py-1.5 text-[12px] hover:bg-bg-secondary data-[selected]:bg-bg-secondary"
          >
            <Select.ItemLabel class="font-mono">
              {itemProps.item.rawValue === NO_ENV ? 'no env' : itemProps.item.rawValue}
            </Select.ItemLabel>
          </Select.Item>
        )}
      >
        <Select.Trigger
          aria-label="Active environment"
          class="flex items-center gap-1.5 rounded-full bg-bg-secondary px-3 py-1 text-fg-primary hover:bg-border"
        >
          <span
            class="h-1.5 w-1.5 rounded-full"
            style={{
              background:
                activeEnvName() === null
                  ? 'var(--fg-secondary)'
                  : 'var(--color-success-foreground)',
            }}
            aria-hidden
          />
          <Select.Value<string>>
            {(s) => (
              <span class="font-mono text-[12px]">
                {s.selectedOption() === NO_ENV ? 'no env' : s.selectedOption()}
              </span>
            )}
          </Select.Value>
          <Select.Icon>
            <ChevronDown size={12} class="text-fg-secondary" />
          </Select.Icon>
        </Select.Trigger>
        <Select.Portal>
          <Select.Content class="z-50 overflow-hidden rounded-md border border-border bg-bg-card shadow-lg">
            <Select.Listbox class="max-h-72 overflow-auto py-1" />
          </Select.Content>
        </Select.Portal>
      </Select>
    </Show>
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
