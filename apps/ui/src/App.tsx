import { createSignal, onMount, Show } from 'solid-js';

import { isTauri, invokeCommand } from './lib/tauri';

export default function App() {
  const [coreVersion, setCoreVersion] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  onMount(async () => {
    if (!isTauri()) {
      setCoreVersion('(running in browser, no Tauri shell)');
      return;
    }
    try {
      const v = await invokeCommand<string>('core_version');
      setCoreVersion(v);
    } catch (e) {
      setError(String(e));
    }
  });

  return (
    <main class="flex min-h-screen items-center justify-center bg-bg-primary text-fg-primary">
      <div class="flex flex-col items-center gap-3 text-center">
        <h1 class="font-mono text-5xl font-bold tracking-tight">Argos</h1>
        <p class="text-sm text-fg-secondary">A fast, git-native API client</p>
        <Show
          when={coreVersion()}
          fallback={
            <p class="mt-4 font-mono text-xs text-fg-secondary">
              {error() ? `error: ${error()}` : 'connecting to core…'}
            </p>
          }
        >
          {(v) => <p class="mt-4 font-mono text-xs text-fg-secondary">core v{v()}</p>}
        </Show>
      </div>
    </main>
  );
}
