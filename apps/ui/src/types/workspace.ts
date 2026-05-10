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

export type ScriptHooks = {
  pre_request: string | null;
  tests: string | null;
};

/**
 * Wire shape of `argos_core::format::RequestDraft`.
 *
 * Note the flattened `RequestVariant` — the YAML uses a single `type:` tag
 * at the top level (e.g. `type: rest`). Future GraphQL / gRPC / WS variants
 * widen this union.
 */
export type RequestDraft = {
  kind: 'request';
  name: string;
  description: string | null;
  scripts: ScriptHooks;
  schema_ref: string | null;
} & RestRequest;

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
  environments: Environment[];
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
