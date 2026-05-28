/**
 * Crash-reporter glue: opt-in state, startup hook, JS error capture.
 *
 * Flow on launch (called from App.tsx):
 *   1. count pending reports
 *   2. if 0 → done
 *   3. if user opted in 'always' → submit silently, toast result
 *   4. if user opted out 'never' → dismiss silently
 *   5. otherwise → open `<CrashReportConsentModal>` for an explicit choice
 *
 * Renderer-side errors (`window.onerror`, `unhandledrejection`) are
 * captured at the same place where the modal listens — see
 * `installCrashCapture`.
 */

import { createSignal } from 'solid-js';

import {
  crashDismissAll,
  crashPendingCount,
  crashRecord,
  crashSubmitAll,
} from './api';
import { loadJSON, saveJSON } from './persist';
import { isTauri } from './tauri';
import { notify, notifyError } from './toast';
import { openCrashLog } from '../stores/crash-log-panel';
import { pushErrorLog } from '../stores/error-log';

const CONSENT_KEY = 'argos:crash:consent';
const SESSION_KEY = 'argos:crash:session_id';

/** Persisted user choice. `ask` = show the modal on every launch
 *  with pending reports; `always` = submit silently; `never` =
 *  drop silently. */
export type CrashConsent = 'ask' | 'always' | 'never';

export function getConsent(): CrashConsent {
  return loadJSON<CrashConsent>(CONSENT_KEY, 'ask');
}

export function setConsent(v: CrashConsent): void {
  saveJSON(CONSENT_KEY, v);
}

/** Anonymous, opt-in session id. Generated when the user first
 *  agrees to submit (or `null` if they haven't). */
function getOrCreateSessionId(): string {
  const existing = loadJSON<string | null>(SESSION_KEY, null);
  if (existing) return existing;
  const id = (typeof crypto !== 'undefined' && 'randomUUID' in crypto)
    ? crypto.randomUUID()
    : Math.random().toString(36).slice(2) + Date.now().toString(36);
  saveJSON(SESSION_KEY, id);
  return id;
}

// ---- signal-backed modal state ------------------------------------------

const [pendingForModal, setPendingForModal] = createSignal<number>(0);

export const crashModalPending = pendingForModal;

export function closeCrashModal(): void {
  setPendingForModal(0);
}

// ---- startup orchestration ----------------------------------------------

let started = false;

/** Run once at app mount. No-op outside Tauri. */
export async function startupCrashFlow(): Promise<void> {
  if (started || !isTauri()) return;
  started = true;
  try {
    const n = await crashPendingCount();
    if (n === 0) return;
    const consent = getConsent();
    if (consent === 'always') {
      const sessionId = getOrCreateSessionId();
      const r = await crashSubmitAll(sessionId);
      if (r.sent > 0) {
        notify.success(
          'Crash reports sent',
          `${r.sent} report${r.sent === 1 ? '' : 's'} submitted${r.failed ? `, ${r.failed} retrying next launch` : ''}.`,
          { label: 'View what was sent', onClick: () => openCrashLog() },
        );
      }
      return;
    }
    if (consent === 'never') {
      await crashDismissAll();
      return;
    }
    // 'ask' → surface the modal with the count.
    setPendingForModal(n);
  } catch (e) {
    // Crash plumbing must never crash the app. Log and move on.
    // eslint-disable-next-line no-console
    console.debug('[crash] startup flow failed', e);
  }
}

/** Called by the modal when the user picks an option. */
export async function applyConsent(
  choice: 'just-once' | 'always' | 'never',
): Promise<void> {
  if (choice === 'never') {
    setConsent('never');
    try {
      const removed = await crashDismissAll();
      if (removed > 0) {
        notify.info(`Discarded ${removed} crash report${removed === 1 ? '' : 's'}.`);
      }
    } catch (e) {
      notifyError('Could not discard reports', e);
    }
    closeCrashModal();
    return;
  }
  if (choice === 'always') setConsent('always');
  try {
    const sessionId = choice === 'always' ? getOrCreateSessionId() : null;
    const r = await crashSubmitAll(sessionId);
    if (r.sent > 0) {
      notify.success(
        'Crash reports sent',
        `Thank you — ${r.sent} report${r.sent === 1 ? '' : 's'} submitted${r.failed ? `, ${r.failed} retrying next launch` : ''}.`,
        { label: 'View what was sent', onClick: () => openCrashLog() },
      );
    }
    if (r.failed > 0 && r.sent === 0) {
      notify.error(
        'Could not submit crash reports',
        `${r.failed} stayed local. We'll retry next launch.`,
      );
    }
  } catch (e) {
    notifyError('Could not submit reports', e);
  }
  closeCrashModal();
}

// ---- renderer-side error capture ----------------------------------------

let captureInstalled = false;

/** Install global handlers that record unhandled JS errors as
 *  pending crash reports. Called once on App mount. */
export function installCrashCapture(): void {
  if (captureInstalled || !isTauri() || typeof window === 'undefined') return;
  captureInstalled = true;

  window.addEventListener('error', (event) => {
    const msg = event.message || String(event.error) || 'unknown JS error';
    const loc = event.filename
      ? `${event.filename}:${event.lineno ?? '?'}:${event.colno ?? '?'}`
      : 'renderer:unknown';
    pushErrorLog({ source: 'renderer:onerror', message: msg, location: loc });
    void crashRecord(msg, loc).catch(() => undefined);
  });

  window.addEventListener('unhandledrejection', (event) => {
    const reason = event.reason;
    const msg =
      reason instanceof Error
        ? `${reason.name}: ${reason.message}`
        : typeof reason === 'string'
          ? reason
          : JSON.stringify(reason);
    // Stack-trace first frame as a "location" if available.
    let loc = 'renderer:unhandledrejection';
    if (reason instanceof Error && reason.stack) {
      const frame = reason.stack.split('\n').find((l) => l.trim().startsWith('at '));
      if (frame) loc = `renderer:${frame.trim().replace(/^at\s+/, '')}`;
    }
    pushErrorLog({ source: 'renderer:unhandledrejection', message: msg, location: loc });
    void crashRecord(msg, loc).catch(() => undefined);
  });
}
