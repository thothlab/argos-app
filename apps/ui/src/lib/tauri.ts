/**
 * Tauri integration helpers.
 *
 * The same UI bundle is used for desktop (inside Tauri) and web (browser /
 * VS Code). `isTauri()` lets call sites branch on transport, and
 * `invokeCommand()` provides a typed wrapper over Tauri's `invoke`.
 */

export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri()) {
    throw new Error(
      `invokeCommand("${command}") called outside Tauri shell. Use the WASM core in browser mode.`,
    );
  }
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(command, args);
}
