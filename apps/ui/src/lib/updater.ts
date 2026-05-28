/**
 * Tauri updater integration.
 *
 * Boot path: `checkForUpdatesOnStartup` runs once on app mount, hits
 * `https://argos.thothlab.tech/api/update/{target}-{arch}/{version}`,
 * and — if an update is offered — surfaces a sticky toast with an
 * "Install now" action plus a persistent affordance in
 * Settings → Advanced → Updates.
 *
 * The pending update is exposed as a Solid signal so the Settings
 * panel can show / hide the install button reactively.
 *
 * Network failures, missing manifests, signature mismatches etc. are
 * logged but never toasted — closed-alpha shouldn't nag users about
 * flaky checks.
 */

import { createSignal } from 'solid-js';

import { isTauri } from './tauri';
import { notify, notifyError } from './toast';

// `tauri-plugin-updater` returns an `Update` object whose handle is
// opaque from the renderer's point of view. We only ever pass it back
// to `downloadAndInstall()`, so the unknown shape is fine.
type UpdateHandle = {
  version: string;
  downloadAndInstall: () => Promise<void>;
};

const [pending, setPending] = createSignal<UpdateHandle | null>(null);

let bootChecked = false;

/** Reactive signal: the currently-pending update, or `null` if none. */
export const pendingUpdate = pending;

export function pendingUpdateVersion(): string | null {
  return pending()?.version ?? null;
}

export function hasPendingUpdate(): boolean {
  return pending() !== null;
}

/** Run once on app boot — no-op outside Tauri shell. */
export async function checkForUpdatesOnStartup(): Promise<void> {
  if (bootChecked || !isTauri()) return;
  bootChecked = true;
  await runCheck({ silent: true });
}

/** User-triggered check from Settings → Advanced → Updates. Toasts
 *  result either way so the user gets feedback. */
export async function checkForUpdatesNow(): Promise<void> {
  if (!isTauri()) {
    notify.info('Update check unavailable', 'Run Argos from the desktop binary to check.');
    return;
  }
  await runCheck({ silent: false });
}

async function runCheck(opts: { silent: boolean }): Promise<void> {
  try {
    const { check } = await import('@tauri-apps/plugin-updater');
    const update = await check();
    if (!update) {
      setPending(null);
      if (!opts.silent) notify.success("You're up to date");
      return;
    }
    const handle: UpdateHandle = {
      version: update.version,
      downloadAndInstall: () => update.downloadAndInstall(),
    };
    setPending(handle);
    notify.info(
      `Update available — v${update.version}`,
      'A new build is ready to install.',
      {
        persistent: true,
        action: { label: 'Install now', onClick: () => void installPendingUpdate() },
      },
    );
  } catch (e) {
    if (opts.silent) {
      // eslint-disable-next-line no-console
      console.debug('[updater] check failed', e);
    } else {
      notifyError('Update check failed', e);
    }
  }
}

/** Download + install the pending bundle, then relaunch Argos. */
export async function installPendingUpdate(): Promise<void> {
  const update = pending();
  if (!update) return;
  try {
    notify.info('Installing update', 'Argos will restart when this finishes.');
    await update.downloadAndInstall();
    const { relaunch } = await import('@tauri-apps/plugin-process');
    await relaunch();
  } catch (e) {
    notifyError('Update failed to install', e);
  }
}
