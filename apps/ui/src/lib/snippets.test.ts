import { describe, expect, it } from 'vitest';

import { SNIPPETS, snippetsFor } from './snippets';

describe('snippets', () => {
  it('exposes both pre-request and tests snippets', () => {
    const pre = snippetsFor('pre');
    const tests = snippetsFor('tests');
    expect(pre.length).toBeGreaterThan(0);
    expect(tests.length).toBeGreaterThan(0);
    expect(pre.every((s) => s.kind === 'pre')).toBe(true);
    expect(tests.every((s) => s.kind === 'tests')).toBe(true);
  });

  it('every snippet has unique id and non-empty body', () => {
    const ids = new Set<string>();
    for (const s of SNIPPETS) {
      expect(s.id).not.toBe('');
      expect(ids.has(s.id), `duplicate id ${s.id}`).toBe(false);
      ids.add(s.id);
      expect(s.body.length).toBeGreaterThan(0);
    }
  });

  it('snippet bodies end with a newline', () => {
    for (const s of SNIPPETS) {
      expect(s.body.endsWith('\n'), `${s.id} should end with newline`).toBe(true);
    }
  });

  it('pre-request snippets reference bru.* APIs we expose', () => {
    const pre = snippetsFor('pre').map((s) => s.body).join('\n');
    expect(pre).toMatch(/bru\.(req|env|info|warn|fail)/);
  });

  it('tests snippets reference bru.test / bru.expect / bru.res', () => {
    const tests = snippetsFor('tests').map((s) => s.body).join('\n');
    expect(tests).toMatch(/bru\.test/);
    expect(tests).toMatch(/bru\.(expect|res)/);
  });
});
