/**
 * Bridge events emitted from the macOS / Linux / Windows native menu
 * (see `crates/desktop/src-tauri/src/main.rs::install_app_menu`) into
 * the matching UI actions.
 *
 * Idempotent — guarded by an `installed` flag so HMR re-runs don't
 * double-bind.
 */

import { listen } from '@tauri-apps/api/event';

import { isTauri } from './tauri';
import { openSettings } from '../stores/settings-panel';

let installed = false;

export async function installNativeMenuBridge(): Promise<void> {
  if (installed || !isTauri()) return;
  installed = true;
  try {
    // Argos → Settings… (CmdOrCtrl+,)
    await listen('settings:open', () => openSettings());
  } catch {
    // Best-effort — UI hotkeys + on-screen buttons still work.
    installed = false;
  }
}
