import { Show } from 'solid-js';

import AppShell from './components/AppShell';
import DropImportOverlay from './components/DropImportOverlay';
import GraphqlEditor from './components/GraphqlEditor';
import ProtocolPlaceholder from './components/ProtocolPlaceholder';
import RequestEditor from './components/RequestEditor';
import ResponsePane from './components/ResponsePane';
import Splitter from './components/Splitter';
import WelcomeScreen from './components/WelcomeScreen';
import { installAutosave } from './lib/autosave';
import { bind } from './lib/hotkeys';
import { saveActiveTab } from './lib/save';
import { activeTab, activeTabId } from './stores/tabs';
import { workspace } from './stores/workspace';

export default function App() {
  installAutosave();

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
      {(() => {
        const protocol = activeTab()?.protocol ?? 'rest';
        if (protocol === 'rest') {
          return <Splitter left={() => <RequestEditor />} right={() => <ResponsePane />} />;
        }
        if (protocol === 'graphql') {
          return <Splitter left={() => <GraphqlEditor />} right={() => <ResponsePane />} />;
        }
        return <ProtocolPlaceholder protocol={protocol} />;
      })()}
    </Show>
  );

  return (
    <>
      <DropImportOverlay />
      <Show when={workspace()} fallback={<WelcomeScreen />}>
        <AppShell tabContent={tabContent} />
      </Show>
    </>
  );
}
