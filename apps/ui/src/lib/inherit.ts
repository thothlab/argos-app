/**
 * Folder-level header / auth inheritance.
 *
 * A request file lives somewhere under `collections/…`. Every parent
 * folder may carry headers and an auth config in its `_folder.argos.yaml`
 * meta — children inherit both unless they explicitly override.
 *
 * - **Headers**: ancestor headers are merged with the request's own,
 *   with the *closer* level winning on case-insensitive name collision.
 *   The request's headers win over every ancestor.
 * - **Auth**: only when the request says `auth.kind === 'inherit'` do we
 *   walk ancestors leaf → root looking for the first folder that has a
 *   non-`inherit` non-`null` auth. Folders may also chain via `inherit`.
 *
 * The merge happens client-side — the wire request that crosses the IPC
 * boundary already contains the resolved values, and the Rust resolver
 * substitutes `{{vars}}` over that.
 */

import type { RowEntry } from '../components/KeyValueTable';
import type { DraftAuth, DraftRequest } from '../stores/request';
import type { Folder, TreeNode } from '../types/workspace';

/** Walk the tree to find the request leaf at `path`, capturing its
 *  ancestor folder chain (root → leaf order). Returns `null` if the path
 *  is unknown (e.g. scratch tab not yet saved). */
export function findAncestors(path: string, tree: TreeNode | null): Folder[] {
  if (!tree || !path) return [];
  const chain: Folder[] = [];
  if (walk(tree, path, chain)) return chain;
  return [];
}

function walk(node: TreeNode, target: string, chain: Folder[]): boolean {
  if (node.kind === 'request') return node.path === target;

  // Folder node — push its meta tentatively, recurse, pop on miss.
  const before = chain.length;
  if (node.meta) chain.push(node.meta);
  for (const child of node.children) {
    if (walk(child, target, chain)) return true;
  }
  chain.length = before;
  return false;
}

/** Apply folder-level inheritance to a draft. Returns a fresh draft —
 *  the original is left untouched. */
export function applyFolderInheritance(
  draft: DraftRequest,
  ancestors: Folder[],
): DraftRequest {
  return {
    ...draft,
    headers: mergeHeaders(draft.headers, ancestors),
    auth: resolveAuth(draft.auth, ancestors),
  };
}

function mergeHeaders(own: RowEntry[], ancestors: Folder[]): RowEntry[] {
  // Build a case-insensitive name → entry map. Order: root first (lowest
  // precedence), so closer ancestors overwrite farther ones, and the
  // request's own headers overwrite all ancestors.
  const map = new Map<string, RowEntry>();

  for (const folder of ancestors) {
    // `folder.headers` may be omitted on the wire (serde skips empty
    // Vecs) — coalesce to [] defensively.
    for (const h of folder.headers ?? []) {
      if (!h.name) continue;
      map.set(h.name.toLowerCase(), {
        name: h.name,
        value: h.value,
        enabled: h.enabled,
      });
    }
  }

  for (const h of own ?? []) {
    if (!h.name) continue;
    map.set(h.name.toLowerCase(), { ...h });
  }

  return Array.from(map.values());
}

function resolveAuth(own: DraftAuth, ancestors: Folder[]): DraftAuth {
  if (own.kind !== 'inherit') return own;

  // Walk leaf → root to find the first concrete auth.
  for (let i = ancestors.length - 1; i >= 0; i -= 1) {
    const a = ancestors[i]!.auth;
    if (!a) continue;
    if (a.type === 'inherit') continue;
    if (a.type === 'bearer') return { kind: 'bearer', token: a.token };
    if (a.type === 'basic') {
      return { kind: 'basic', username: a.username, password: a.password };
    }
    if (a.type === 'api_key') {
      return {
        kind: 'api_key',
        name: a.name,
        value: a.value,
        location: a.location,
      };
    }
  }
  // No ancestor offers auth — fall through to no auth.
  return { kind: 'none' };
}
