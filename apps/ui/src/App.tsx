import { Match, Show, Switch } from 'solid-js';

import AppShell from './components/AppShell';
import CommandPalette from './components/CommandPalette';
import CrashReportConsentModal from './components/CrashReportConsentModal';
import SettingsPanel from './components/SettingsPanel';
import GraphqlEditor from './components/GraphqlEditor';
import PromptModal from './components/PromptModal';
import ProtocolPlaceholder from './components/ProtocolPlaceholder';
import RequestEditor from './components/RequestEditor';
import ResponsePane from './components/ResponsePane';
import Splitter from './components/Splitter';
import Toaster from './components/Toaster';
import WebsocketEditor from './components/WebsocketEditor';
import WelcomeScreen from './components/WelcomeScreen';
import { defineAction, installActionRouter } from './lib/actions';
import { installAutosave } from './lib/autosave';
import { installCrashCapture, startupCrashFlow } from './lib/crashes';
import { saveActiveTab } from './lib/save';
import { checkForUpdatesOnStartup } from './lib/updater';
import { togglePalette } from './stores/command-palette';
import { toggleDock, toggleSidebar } from './stores/layout';
import { openSettings } from './stores/settings-panel';
import { initSettings } from './stores/settings';
import { activeTab, activeTabId } from './stores/tabs';
import { cycleTheme } from './stores/theme';
import { workspace } from './stores/workspace';
import { installWsEventListener } from './stores/ws';

export default function App() {
  // Settings before UI side-effects — theme rendering reads from here.
  void initSettings();
  installAutosave();
  installCrashCapture();
  void installWsEventListener();
  void checkForUpdatesOnStartup();
  void startupCrashFlow();

  defineAction({
    id: 'palette.toggle',
    label: 'Toggle command palette',
    defaultCombo: { key: 'k', meta: true },
    handler: () => togglePalette(),
  });

  // ⌘S saves the active tab. Scratch tabs (no `path`) trigger a Save-As
  // dialog the first time, then save directly thereafter.
  defineAction({
    id: 'request.save',
    label: 'Save active tab',
    defaultCombo: { key: 's', meta: true },
    handler: () => {
      void saveActiveTab().then((outcome) => {
        switch (outcome.kind) {
          case 'saved':
          case 'cancelled':
          case 'no-tab':
          case 'no-workspace':
            return;
          case 'error':
            alert(`Save failed:\n\n${outcome.message}`);
        }
      });
    },
  });

  defineAction({
    id: 'settings.open',
    label: 'Open settings',
    defaultCombo: { key: ',', meta: true },
    handler: () => openSettings(),
  });

  // Layout + theme actions live at the App level (not AppShell) so the
  // shortcuts work on the Welcome screen too and appear in the Settings
  // keybindings panel before any workspace is opened.
  defineAction({
    id: 'sidebar.toggle',
    label: 'Toggle sidebar',
    defaultCombo: { key: 'b', meta: true },
    handler: toggleSidebar,
  });
  defineAction({
    id: 'dock.toggle',
    label: 'Toggle lower dock',
    defaultCombo: { key: 'j', meta: true },
    handler: toggleDock,
  });
  defineAction({
    id: 'theme.cycle',
    label: 'Cycle theme (light / dark / system)',
    defaultCombo: { key: 't', meta: true, shift: true },
    handler: cycleTheme,
  });

  installActionRouter();

  const tabContent = () => (
    <Show
      when={activeTabId()}
      fallback={
        <div class="flex h-full items-center justify-center">
          <p class="text-[13px] text-fg-secondary">
            No tab open. Click a request in the sidebar, or press{' '}
            <kbd class="rounded bg-bg-secondary px-1.5 py-0.5 font-mono text-[11px]">+</kbd> in the
            tab bar for an unsaved scratch tab. Save with{' '}
            <kbd class="rounded bg-bg-secondary px-1.5 py-0.5 font-mono text-[11px]">⌘S</kbd>.
          </p>
        </div>
      }
    >
      {/* Switch/Match keeps each branch mounted within its lifetime —
          an IIFE here re-ran on every tabs() signal change (e.g. when
          autosave flipped a tab's `dirty` bit) and the inner
          RequestEditor was unmounted mid-typing. */}
      <Switch fallback={<ProtocolPlaceholder protocol={activeTab()?.protocol ?? 'rest'} />}>
        <Match when={(activeTab()?.protocol ?? 'rest') === 'rest'}>
          <Splitter left={() => <RequestEditor />} right={() => <ResponsePane />} />
        </Match>
        <Match when={activeTab()?.protocol === 'graphql'}>
          <Splitter left={() => <GraphqlEditor />} right={() => <ResponsePane />} />
        </Match>
        <Match when={activeTab()?.protocol === 'websocket'}>
          <WebsocketEditor />
        </Match>
      </Switch>
    </Show>
  );

  return (
    <>
      <Toaster />
      <PromptModal />
      <CommandPalette />
      <CrashReportConsentModal />
      <SettingsPanel />
      <Show when={workspace()} fallback={<WelcomeScreen />}>
        <AppShell tabContent={tabContent} />
      </Show>
    </>
  );
}
