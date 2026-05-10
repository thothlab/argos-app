import { describe, expect, it } from 'vitest';

import { applyFolderInheritance, findAncestors } from './inherit';
import { emptyDraft, type DraftRequest } from '../stores/request';
import type { Folder, RequestDraft, TreeNode } from '../types/workspace';

function folder(name: string, partial: Partial<Folder> = {}): Folder {
  return {
    kind: 'folder',
    name,
    description: null,
    headers: [],
    auth: null,
    ...partial,
  };
}

function reqDraft(name: string): RequestDraft {
  return {
    kind: 'request',
    name,
    description: null,
    type: 'rest',
    method: 'GET',
    url: 'https://example.com',
    query: [],
    headers: [],
    auth: null,
    body: null,
    scripts: { pre_request: null, tests: null },
    schema_ref: null,
  };
}

const tree: TreeNode = {
  kind: 'folder',
  path: '/ws/collections',
  name: 'collections',
  meta: folder('Root', {
    headers: [
      { name: 'X-Trace', value: 'root', enabled: true },
      { name: 'X-Common', value: 'root-only', enabled: true },
    ],
    auth: { type: 'bearer', token: 'root-token' },
  }),
  children: [
    {
      kind: 'folder',
      path: '/ws/collections/users',
      name: 'Users',
      meta: folder('Users', {
        headers: [{ name: 'X-Trace', value: 'users', enabled: true }],
        auth: { type: 'inherit' },
      }),
      children: [
        {
          kind: 'request',
          path: '/ws/collections/users/list.argos.yaml',
          draft: reqDraft('List users'),
        },
      ],
    },
  ],
};

describe('findAncestors', () => {
  it('returns root → leaf folder chain for the request path', () => {
    const chain = findAncestors('/ws/collections/users/list.argos.yaml', tree);
    expect(chain.map((f) => f.name)).toEqual(['Root', 'Users']);
  });

  it('returns [] for unknown paths', () => {
    expect(findAncestors('/nope', tree)).toEqual([]);
  });
});

describe('applyFolderInheritance', () => {
  function makeDraft(overrides: Partial<DraftRequest> = {}): DraftRequest {
    return { ...emptyDraft(), ...overrides };
  }

  it('merges ancestor headers, child overrides parent on collision', () => {
    const chain = findAncestors('/ws/collections/users/list.argos.yaml', tree);
    const merged = applyFolderInheritance(makeDraft(), chain);
    const trace = merged.headers.find((h) => h.name.toLowerCase() === 'x-trace');
    const common = merged.headers.find((h) => h.name.toLowerCase() === 'x-common');
    expect(trace?.value).toBe('users');
    expect(common?.value).toBe('root-only');
  });

  it('request own header beats every ancestor on name collision', () => {
    const chain = findAncestors('/ws/collections/users/list.argos.yaml', tree);
    const merged = applyFolderInheritance(
      makeDraft({
        headers: [{ name: 'X-Trace', value: 'request', enabled: true }],
      }),
      chain,
    );
    const trace = merged.headers.find((h) => h.name.toLowerCase() === 'x-trace');
    expect(trace?.value).toBe('request');
  });

  it('inherit auth picks the nearest concrete ancestor auth', () => {
    const chain = findAncestors('/ws/collections/users/list.argos.yaml', tree);
    const merged = applyFolderInheritance(
      makeDraft({ auth: { kind: 'inherit' } }),
      chain,
    );
    expect(merged.auth).toEqual({ kind: 'bearer', token: 'root-token' });
  });

  it('inherit auth with no ancestor set falls back to none', () => {
    const merged = applyFolderInheritance(
      makeDraft({ auth: { kind: 'inherit' } }),
      [],
    );
    expect(merged.auth).toEqual({ kind: 'none' });
  });

  it('non-inherit auth is left untouched', () => {
    const chain = findAncestors('/ws/collections/users/list.argos.yaml', tree);
    const merged = applyFolderInheritance(
      makeDraft({ auth: { kind: 'bearer', token: 'own' } }),
      chain,
    );
    expect(merged.auth).toEqual({ kind: 'bearer', token: 'own' });
  });
});
