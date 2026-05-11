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
import type { Environment, RecentEntry, RequestDraft, Workspace } from '../types/workspace';

export async function coreVersion(): Promise<string> {
  return invokeCommand<string>('core_version');
}

export async function ping(): Promise<string> {
  return invokeCommand<string>('ping');
}

export type TestResult = {
  name: string;
  passed: boolean;
  message: string;
};

/** Outcome of a `send_request` IPC call — response plus any pre-request
 *  / tests script logs, test results, and env mutations. */
export type SendOutcome = {
  response: HttpResponse;
  pre_request_logs: string[];
  tests_logs: string[];
  tests: TestResult[];
  env_updates: Record<string, string>;
  /** Names the script(s) cleared via `bru.env.unset` / `pm.environment.unset`. */
  env_unsets?: string[];
};

/**
 * Fire one HTTP request through the Rust engine and buffer the full response.
 *
 * Throws on transport errors (bad URL, timeout, network) — non-2xx HTTP
 * responses are returned normally so the UI can render the status badge.
 *
 * `preRequestScript` runs before the wire send; `testsScript` runs after
 * the response arrives. Either may be `null`.
 */
export async function sendRequest(
  req: HttpRequest,
  env: Record<string, string> = {},
  preRequestScript: string | null = null,
  testsScript: string | null = null,
): Promise<SendOutcome> {
  return invokeCommand<SendOutcome>('send_request', {
    req,
    env,
    preRequestScript,
    testsScript,
  });
}

/** Render the request as a multi-line `curl` invocation. */
export async function requestToCurl(
  req: HttpRequest,
  env: Record<string, string> = {},
): Promise<string> {
  return invokeCommand<string>('request_to_curl', { req, env });
}

/** Parse a pasted `curl` command into a wire request. */
export async function curlToRequest(input: string): Promise<HttpRequest> {
  return invokeCommand<HttpRequest>('curl_to_request', { input });
}

/** Result of a Postman v2.1 import — counts plus paths the UI can use
 *  to scroll the workspace tree to the freshly created folder / env. */
export type PostmanImportReport = {
  folder_path: string;
  folders_created: number;
  requests_created: number;
  variables_count: number;
  env_path: string | null;
};

/** Import a Postman v2.1 collection JSON into the active workspace.
 *  `source` is either a file path (default) or inline JSON when
 *  `inline = true`. */
export async function postmanImport(
  workspaceRoot: string,
  source: string,
  inline = false,
): Promise<PostmanImportReport> {
  return invokeCommand<PostmanImportReport>('postman_import', {
    workspaceRoot,
    source,
    inline,
  });
}

/** Import an Insomnia v4 export JSON into the active workspace.
 *  Returns the same shape as `postmanImport`. */
export async function insomniaImport(
  workspaceRoot: string,
  source: string,
  inline = false,
): Promise<PostmanImportReport> {
  return invokeCommand<PostmanImportReport>('insomnia_import', {
    workspaceRoot,
    source,
    inline,
  });
}

/** Import a Bruno collection directory. `source` is the path to the
 *  collection root (the folder containing `bruno.json`). */
export async function brunoImport(
  workspaceRoot: string,
  source: string,
): Promise<PostmanImportReport> {
  return invokeCommand<PostmanImportReport>('bruno_import', {
    workspaceRoot,
    source,
  });
}

/** Import an OpenAPI 3.x document (JSON or YAML) into the workspace.
 *  `source` is a path on disk; pass `inline = true` to send the raw
 *  document text instead. */
export async function openapiImport(
  workspaceRoot: string,
  source: string,
  inline = false,
): Promise<PostmanImportReport> {
  return invokeCommand<PostmanImportReport>('openapi_import', {
    workspaceRoot,
    source,
    inline,
  });
}

/** Importer format identifiers used by the drag-drop wizard. */
export type ImportFormat = 'postman' | 'insomnia' | 'openapi' | 'bruno' | 'unknown';

/** Sniff a dropped file / folder path and return the matching importer.
 *  `unknown` means the wizard should show an error toast and bail. */
export async function importDetect(path: string): Promise<{ format: ImportFormat; name: string }> {
  return invokeCommand<{ format: ImportFormat; name: string }>('import_detect', { path });
}

/** Export the active workspace as a Postman v2.1 collection JSON.
 *  Returns the absolute path of the written file. */
export async function postmanExport(
  workspaceRoot: string,
  targetPath: string | null = null,
): Promise<string> {
  return invokeCommand<string>('postman_export', { workspaceRoot, targetPath });
}

/** Export one run as a HAR 1.2 archive. Returns the path of the file. */
export async function runExportHar(
  request: HttpRequest,
  response: HttpResponse,
  startedAtIso8601: string,
  targetPath: string,
): Promise<string> {
  return invokeCommand<string>('run_export_har', {
    request,
    response,
    startedAtIso8601,
    targetPath,
  });
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
  protocol: 'rest' | 'graphql' | 'websocket' = 'rest',
): Promise<string> {
  return invokeCommand<string>('tree_create_request', { parentDir, name, method, protocol });
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

// ---- run history --------------------------------------------------------

export type PersistedRun = {
  id: string;
  started_at_ms: number;
  request: HttpRequest;
  response: HttpResponse;
};

export async function runRecord(
  workspaceRoot: string,
  requestPath: string,
  run: PersistedRun,
): Promise<void> {
  return invokeCommand<void>('run_record', { workspaceRoot, requestPath, run });
}

export async function runLoad(
  workspaceRoot: string,
  requestPath: string,
): Promise<PersistedRun[]> {
  return invokeCommand<PersistedRun[]>('run_load', { workspaceRoot, requestPath });
}

export async function runClear(
  workspaceRoot: string,
  requestPath: string,
): Promise<void> {
  return invokeCommand<void>('run_clear', { workspaceRoot, requestPath });
}

// ---- environments -------------------------------------------------------

export async function environmentSave(path: string, env: Environment): Promise<void> {
  return invokeCommand<void>('environment_save', { path, env });
}

export async function environmentCreate(envDir: string, name: string): Promise<string> {
  return invokeCommand<string>('environment_create', { envDir, name });
}

export async function environmentDelete(path: string): Promise<void> {
  return invokeCommand<void>('environment_delete', { path });
}
