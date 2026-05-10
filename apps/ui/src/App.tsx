import { createSignal, onMount, Show } from 'solid-js';

import AppShell from './components/AppShell';
import { activeTab } from './stores/tabs';
import { isTauri, invokeCommand } from './lib/tauri';

export default function App() {
  const [coreVersion, setCoreVersion] = createSignal<string | null>(null);
  const [bridgeError, setBridgeError] = createSignal<string | null>(null);

  onMount(async () => {
    if (!isTauri()) {
      setCoreVersion('(running in browser)');
      return;
    }
    try {
      const v = await invokeCommand<string>('core_version');
      setCoreVersion(v);
    } catch (e) {
      setBridgeError(String(e));
    }
  });

  const tabContent = () => (
    <ActiveTabPlaceholder
      coreVersion={coreVersion()}
      bridgeError={bridgeError()}
    />
  );

  return <AppShell tabContent={tabContent} />;
}

/**
 * Placeholder rendered until T1.3 brings in the real request editor.
 *
 * Shows the active tab's name + method as a smoke-test for the tab store,
 * and surfaces the core version reported via Tauri IPC so the bridge stays
 * verifiable from the UI.
 */
function ActiveTabPlaceholder(props: {
  coreVersion: string | null;
  bridgeError: string | null;
}) {
  const tab = activeTab;

  return (
    <div class="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
      <Show
        when={tab()}
        fallback={
          <p class="text-fg-secondary">No tab open. Press <kbd class="font-mono">⌘N</kbd> for a new request (TBD).</p>
        }
      >
        {(t) => (
          <>
            <p class="font-mono text-2xl">
              <span style={{ color: `var(--method-${t().method.toLowerCase()})` }}>
                {t().method}
              </span>{' '}
              {t().title}
            </p>
            <p class="text-fg-secondary">
              Request editor & response viewer arrive in T1.3 / T1.4.
            </p>
          </>
        )}
      </Show>

      <div class="mt-8 font-mono text-[11px] text-fg-secondary">
        <Show
          when={!props.bridgeError}
          fallback={<span class="text-[var(--color-error-foreground)]">bridge error: {props.bridgeError}</span>}
        >
          {props.coreVersion ? `core ${props.coreVersion}` : 'connecting to core…'}
        </Show>
      </div>
    </div>
  );
}
