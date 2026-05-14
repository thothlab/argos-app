import { Match, Show, Switch } from 'solid-js';

import AppShell from './components/AppShell';
import CommandPalette from './components/CommandPalette';
import CrashReportConsentModal from './components/CrashReportConsentModal';
import GraphqlEditor from './components/GraphqlEditor';
import PromptModal from './components/PromptModal';
import ProtocolPlaceholder from './components/ProtocolPlaceholder';
import RequestEditor from './components/RequestEditor';
import ResponsePane from './components/ResponsePane';
import Splitter from './components/Splitter';
import Toaster from './components/Toaster';
import WebsocketEditor from './components/WebsocketEditor';
import WelcomeScreen from './components/WelcomeScreen';
import { installAutosave } from './lib/autosave';
import { installCrashCapture, startupCrashFlow } from './lib/crashes';
import { bind } from './lib/hotkeys';
import { saveActiveTab } from './lib/save';
import { checkForUpdatesOnStartup } from './lib/updater';
import { togglePalette } from './stores/command-palette';
import { activeTab, activeTabId } from './stores/tabs';
import { workspace } from './stores/workspace';
import { installWsEventListener } from './stores/ws';

export default function App() {
  installAutosave();
  installCrashCapture();
  void installWsEventListener();
  void checkForUpdatesOnStartup();
  void startupCrashFlow();

  bind({ key: 'k', meta: true }, () => {
    togglePalette();
  });

  // ⌘S saves the active tab. Scratch tabs (no `path`) trigger a Save-As
  // dialog the first time, then save directly thereafter.
  bind({ key: 's', meta: true }, async () => {
    const outcome = await saveActiveTab();
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
      <Show when={workspace()} fallback={<WelcomeScreen />}>
        <AppShell tabContent={tabContent} />
      </Show>
    </>
  );
}
