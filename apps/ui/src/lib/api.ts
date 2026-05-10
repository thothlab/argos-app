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

import type { HttpMethod, HttpRequest, HttpResponse } from '../types/http';
import type { RecentEntry, RequestDraft, Workspace } from '../types/workspace';

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
export async function sendRequest(
  req: HttpRequest,
  env: Record<string, string> = {},
): Promise<HttpResponse> {
  return invokeCommand<HttpResponse>('send_request', { req, env });
}

/** Render the request as a multi-line `curl` invocation. */
export async function requestToCurl(
  req: HttpRequest,
  env: Record<string, string> = {},
): Promise<string> {
  return invokeCommand<string>('request_to_curl', { req, env });
}

// ---- workspace ----------------------------------------------------------

export async function workspaceOpen(path: string): Promise<Workspace> {
  return invokeCommand<Workspace>('workspace_open', { path });
}

export async function workspaceCreate(path: string, name: string): Promise<Workspace> {
  return invokeCommand<Workspace>('workspace_create', { path, name });
}

export async function workspaceReload(path: string): Promise<Workspace> {
  return invokeCommand<Workspace>('workspace_reload', { path });
}

export async function workspaceListRecent(): Promise<RecentEntry[]> {
  return invokeCommand<RecentEntry[]>('workspace_list_recent');
}

export async function workspaceClearRecent(): Promise<void> {
  return invokeCommand<void>('workspace_clear_recent');
}

export async function workspaceClose(): Promise<void> {
  return invokeCommand<void>('workspace_close');
}

/** Persist a request draft to its YAML file on disk. */
export async function requestSave(path: string, draft: RequestDraft): Promise<void> {
  return invokeCommand<void>('request_save', { path, draft });
}

/** Slugify a human request name to a filesystem-safe filename. */
export async function slug(name: string): Promise<string> {
  return invokeCommand<string>('slug', { name });
}

// ---- tree CRUD ----------------------------------------------------------

export async function treeCreateFolder(parentDir: string, name: string): Promise<string> {
  return invokeCommand<string>('tree_create_folder', { parentDir, name });
}

export async function treeCreateRequest(
  parentDir: string,
  name: string,
  method: HttpMethod | null = 'GET',
): Promise<string> {
  return invokeCommand<string>('tree_create_request', { parentDir, name, method });
}

export async function treeRename(path: string, newName: string): Promise<string> {
  return invokeCommand<string>('tree_rename', { path, newName });
}

export async function treeDelete(path: string): Promise<void> {
  return invokeCommand<void>('tree_delete', { path });
}

export async function treeMove(src: string, destDir: string): Promise<string> {
  return invokeCommand<string>('tree_move', { src, destDir });
}
