/**
 * Active workspace state.
 *
 * `null` while the user is on the welcome screen; populated after a
 * successful `workspaceOpen` / `workspaceCreate`. The whole tree is held in
 * memory — for v0.1 sizes (tens of thousands of requests in pathological
 * monorepos) this is fine; we can switch to lazy-loading branches when we
 * hit measurable problems.
 */

import { createSignal } from 'solid-js';

import type { TreeNode, Workspace } from '../types/workspace';

const [workspace, setWorkspaceRaw] = createSignal<Workspace | null>(null);

export { workspace };

export function setWorkspace(ws: Workspace | null): void {
  setWorkspaceRaw(ws);
}

/**
 * Find a request leaf in the tree by its absolute path. Used when the user
 * clicks an item in the sidebar to open the matching tab.
 */
export function findRequestByPath(path: string): TreeNode | null {
  const ws = workspace();
  if (!ws) return null;
  return walk(ws.tree, path);
}

function walk(node: TreeNode, path: string): TreeNode | null {
  if (node.kind === 'request' && node.path === path) return node;
  if (node.kind !== 'folder') return null;
  for (const child of node.children) {
    const found = walk(child, path);
    if (found) return found;
  }
  return null;
}
