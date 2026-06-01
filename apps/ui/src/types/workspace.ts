/**
 * TypeScript mirror of `argos_core::workspace` and `argos_core::format`
 * types — enough surface to render the tree and roundtrip a draft.
 *
 * Hand-written to surface drift loudly during code review. Drift breaks IPC
 * deserialisation; see `apps/ui/src/lib/api.ts` for how those errors map to
 * UI state.
 */

import type { HttpMethod } from './http';

// ---- file kinds ----------------------------------------------------------

export type FileKind = 'workspace' | 'folder' | 'request' | 'environment';

// ---- workspace meta ------------------------------------------------------

export type WorkspaceConfig = {
  collections_dir: string;
  environments_dir: string;
  runs_dir: string;
  default_environment: string | null;
};

export type WorkspaceManifest = {
  kind: 'workspace';
  version: number;
  name: string;
  description: string | null;
  config: WorkspaceConfig;
};

// ---- requests ------------------------------------------------------------

export type KeyValue = {
  name: string;
  value: string;
  enabled: boolean;
};

export type ApiKeyLocation = 'header' | 'query' | 'cookie';

export type AuthConfig =
  | { type: 'inherit' }
  | { type: 'bearer'; token: string }
  | { type: 'basic'; username: string; password: string }
  | { type: 'api_key'; name: string; value: string; location: ApiKeyLocation };

export type FormField = {
  name: string;
  value: string;
  enabled: boolean;
};

export type BodyDraft =
  | { kind: 'text'; content: string; content_type: string }
  | { kind: 'json'; value: unknown }
  | { kind: 'form_url_encoded'; fields: FormField[] };

export type RestRequest = {
  type: 'rest';
  method: HttpMethod;
  url: string;
  query: KeyValue[];
  headers: KeyValue[];
  auth: AuthConfig | null;
  body: BodyDraft | null;
};

/**
 * GraphQL request — POSTed as `{ query, variables, operationName }`.
 * Execution lands in E5 chunk 2; the chunk-1 editor shows a placeholder.
 */
export type GraphqlRequest = {
  type: 'graphql';
  url: string;
  query: string;
  variables: unknown | null;
  operation_name: string | null;
  headers: KeyValue[];
  auth: AuthConfig | null;
};

export type WsMessageTemplate = {
  name: string;
  body: string;
};

/**
 * WebSocket request — connection params + outgoing message templates.
 * Execution lands in E5 chunk 3.
 */
export type WebsocketRequest = {
  type: 'websocket';
  url: string;
  subprotocols: string[];
  headers: KeyValue[];
  auth: AuthConfig | null;
  messages: WsMessageTemplate[];
};

export type RequestVariant = RestRequest | GraphqlRequest | WebsocketRequest;

export type ProtocolTag = RequestVariant['type'];

export type ScriptHooks = {
  pre_request: string | null;
  tests: string | null;
};

/**
 * Wire shape of `argos_core::format::RequestDraft`.
 *
 * The YAML uses a single `type:` tag at the top level (e.g. `type: rest`,
 * `type: graphql`, `type: websocket`) — `RequestVariant` is flattened
 * into the draft, so a GraphQL request gets `query:` / `variables:`
 * keys directly, not nested under an opaque blob.
 */
export type RequestDraft = {
  kind: 'request';
  name: string;
  description: string | null;
  scripts: ScriptHooks;
  schema_ref: string | null;
} & RequestVariant;

// ---- folder --------------------------------------------------------------

export type Folder = {
  kind: 'folder';
  name: string;
  description: string | null;
  headers: KeyValue[];
  auth: AuthConfig | null;
};

// ---- environment ---------------------------------------------------------

export type EnvVar = {
  name: string;
  value: string;
  enabled: boolean;
};

export type Environment = {
  kind: 'environment';
  name: string;
  variables: EnvVar[];
  secrets: EnvVar[];
};

/** Environment file paired with its absolute on-disk path. */
export type EnvironmentEntry = {
  path: string;
} & Environment;

// ---- tree ----------------------------------------------------------------

export type TreeNode =
  | {
      kind: 'folder';
      path: string;
      name: string;
      meta: Folder | null;
      children: TreeNode[];
    }
  | {
      kind: 'request';
      path: string;
      draft: RequestDraft;
    };

// ---- workspace -----------------------------------------------------------

export type Workspace = {
  root: string;
  manifest: WorkspaceManifest;
  tree: TreeNode;
  environments: EnvironmentEntry[];
};

export type RecentEntry = {
  path: string;
  last_opened_ms: number;
};

// ---- helpers -------------------------------------------------------------

/** Walk the tree and return all request leaves in document order. */
export function walkRequests(tree: TreeNode): Array<{ path: string; draft: RequestDraft }> {
  if (tree.kind === 'request') {
    return [{ path: tree.path, draft: tree.draft }];
  }
  return tree.children.flatMap(walkRequests);
}

/** Walk the tree and return every folder with a breadcrumb label like
 *  `docs / Auth / admin`. The workspace root itself is omitted (its
 *  `name` is the workspace's own name, which isn't meaningful as a
 *  target — pick a child folder instead). The result is in document
 *  order, suitable for a flat dropdown. */
export function walkFolders(tree: TreeNode): Array<{ path: string; label: string }> {
  const out: Array<{ path: string; label: string }> = [];
  function visit(node: TreeNode, trail: string[]): void {
    if (node.kind !== 'folder') return;
    if (trail.length > 0) {
      out.push({ path: node.path, label: trail.join(' / ') });
    }
    for (const c of node.children) {
      if (c.kind === 'folder') {
        visit(c, [...trail, c.name]);
      }
    }
  }
  visit(tree, []);
  return out;
}
