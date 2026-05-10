import { Show } from 'solid-js';

import AppShell from './components/AppShell';
import RequestEditor from './components/RequestEditor';
import ResponsePane from './components/ResponsePane';
import Splitter from './components/Splitter';
import WelcomeScreen from './components/WelcomeScreen';
import { bind } from './lib/hotkeys';
import { requestSave } from './lib/api';
import { activeTab, activeTabId, markDirty, tabAsDraft } from './stores/tabs';
import { workspace } from './stores/workspace';

export default function App() {
  // ⌘S saves the active tab to its backing file.
  bind({ key: 's', meta: true }, async () => {
    const tabId = activeTabId();
    const t = activeTab();
    if (!tabId || !t || !t.path) return;
    const draft = tabAsDraft(tabId);
    if (!draft) return;
    try {
      await requestSave(t.path, draft);
      markDirty(tabId, false);
    } catch (e) {
      alert(`Save failed:\n\n${String(e)}`);
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
            tab bar for an unsaved scratch tab.
          </p>
        </div>
      }
    >
      <Splitter left={() => <RequestEditor />} right={() => <ResponsePane />} />
    </Show>
  );

  return (
    <Show when={workspace()} fallback={<WelcomeScreen />}>
      <AppShell tabContent={tabContent} />
    </Show>
  );
}
