import { Show } from 'solid-js';

import AppShell from './components/AppShell';
import RequestEditor from './components/RequestEditor';
import ResponsePane from './components/ResponsePane';
import Splitter from './components/Splitter';
import { activeTabId } from './stores/tabs';

export default function App() {
  const tabContent = () => (
    <Show
      when={activeTabId()}
      fallback={
        <div class="flex h-full items-center justify-center">
          <p class="text-[13px] text-fg-secondary">
            No tab open. Press <kbd class="rounded bg-bg-secondary px-1.5 py-0.5 font-mono text-[11px]">+</kbd>{' '}
            in the tab bar to start a new request.
          </p>
        </div>
      }
    >
      <Splitter
        left={() => <RequestEditor />}
        right={() => <ResponsePane />}
      />
    </Show>
  );

  return <AppShell tabContent={tabContent} />;
}
