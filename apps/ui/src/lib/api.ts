/**
 * Typed wrapper around `argos-core` Tauri commands.
 *
 * Each function maps 1:1 to a `#[tauri::command]` in
 * `crates/desktop/src-tauri/src/main.rs`. Errors propagated from Rust come
 * back as plain strings; we re-throw as `Error` so the UI can use the usual
 * try/catch flow.
 *
 * In browser-only mode (no Tauri shell) every call rejects — the UI is
 * expected to detect that via `isTauri()` and gate request-sending.
 */

import { invokeCommand } from './tauri';

import type { HttpRequest, HttpResponse } from '../types/http';

export async function coreVersion(): Promise<string> {
  return invokeCommand<string>('core_version');
}

export async function ping(): Promise<string> {
  return invokeCommand<string>('ping');
}

/**
 * Fire one HTTP request through the Rust engine and buffer the full response.
 *
 * Throws on transport errors (bad URL, timeout, network) — non-2xx HTTP
 * responses are returned normally so the UI can render the status badge.
 */
export async function sendRequest(req: HttpRequest): Promise<HttpResponse> {
  return invokeCommand<HttpResponse>('send_request', { req });
}

/** Render the request as a multi-line `curl` invocation. */
export async function requestToCurl(req: HttpRequest): Promise<string> {
  return invokeCommand<string>('request_to_curl', { req });
}
