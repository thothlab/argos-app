/**
 * Tauri updater integration.
 *
 * Checks `https://argos.thothlab.tech/api/update/{target}-{arch}/{version}`
 * once per app launch. If an update is offered, surfaces a sticky
 * toast with a "Install and restart" action — `tauri-plugin-updater`
 * downloads + applies the bundle, then we call
 * `tauri-plugin-process::relaunch()`.
 *
 * Failures are silent (debug log only) — closed-alpha users
 * shouldn't be nagged about flaky network checks.
 */

import { isTauri } from './tauri';
import { notify, notifyError } from './toast';

let installed = false;

/** Run once on app boot — no-op outside Tauri shell. */
export async function checkForUpdatesOnStartup(): Promise<void> {
  if (installed || !isTauri()) return;
  installed = true;

  try {
    const { check } = await import('@tauri-apps/plugin-updater');
    const update = await check();
    if (!update) {
      return; // up to date
    }
    notify.info(
      `Update available — v${update.version}`,
      'A new build is ready. Run "Install update" from the menu (or restart later) to apply.',
    );
    // Stash for the user-triggered install action.
    pendingUpdate = update;
  } catch (e) {
    // Network blip, manifest 503 (no manifest published yet),
    // signature mismatch, etc. Don't toast — just log.
    // eslint-disable-next-line no-console
    console.debug('[updater] check failed', e);
  }
}

// Module-level pending update, populated by `checkForUpdatesOnStartup`
// and consumed by `installPendingUpdate` when the user opts in.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let pendingUpdate: any = null;

export function hasPendingUpdate(): boolean {
  return pendingUpdate !== null;
}

export function pendingUpdateVersion(): string | null {
  return pendingUpdate?.version ?? null;
}

/** Triggered by the user from a menu / toast button. Downloads +
 *  installs the bundle, then relaunches Argos. */
export async function installPendingUpdate(): Promise<void> {
  if (!pendingUpdate) return;
  try {
    notify.info('Installing update', 'Argos will restart when this finishes.');
    await pendingUpdate.downloadAndInstall();
    const { relaunch } = await import('@tauri-apps/plugin-process');
    await relaunch();
  } catch (e) {
    notifyError('Update failed to install', e);
  }
}
