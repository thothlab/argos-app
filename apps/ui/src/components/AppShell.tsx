/**
 * AppShell — root layout container.
 *
 *   ┌─ TopBar ──────────────────────────────────────────────┐
 *   ├─ Sidebar │ TabBar                                     │
 *   │  (resiz- ├─ Active tab content                        │
 *   │   able)  ├─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─│
 *   │          └─ LowerDock (collapsible)                   │
 *   └────────────────────────────────────────────────────────┘
 *
 * The shell only knows about the *layout*. The active-tab area is rendered
 * via the `tabContent` prop so T1.3/T1.4 can drop a real request editor in
 * without touching this component.
 */

import { Show, type JSX } from 'solid-js';

import { sidebarVisible, sidebarWidth, dockVisible } from '../stores/layout';

import LowerDock from './LowerDock';
import Sidebar from './Sidebar';
import SidebarResizer from './SidebarResizer';
import TabBar from './TabBar';
import TopBar from './TopBar';

export type AppShellProps = {
  /** Content rendered inside the active tab. */
  tabContent: () => JSX.Element;
};

export default function AppShell(props: AppShellProps) {
  // Global shortcuts live in App.tsx so they fire on the Welcome screen too.

  return (
    <div class="flex h-screen w-screen flex-col bg-bg-primary text-fg-primary">
      <TopBar />

      <div class="flex flex-1 overflow-hidden">
        <Show when={sidebarVisible()}>
          <aside
            class="flex shrink-0 flex-col border-r border-border bg-bg-card"
            style={{ width: `${sidebarWidth()}px` }}
            aria-label="Workspace sidebar"
          >
            <Sidebar />
          </aside>
          <SidebarResizer />
        </Show>

        <main class="flex flex-1 flex-col overflow-hidden">
          <TabBar />

          <section class="min-h-0 flex-1 overflow-hidden">{props.tabContent()}</section>

          <Show when={dockVisible()}>
            <LowerDock />
          </Show>
        </main>
      </div>
    </div>
  );
}
