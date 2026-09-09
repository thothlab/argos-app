import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  applySuggestion,
  DYNAMIC_VARS,
  envSuggestions,
  filterSuggestions,
  findOpenPlaceholder,
} from './dynamic-vars';

describe('findOpenPlaceholder', () => {
  it('detects a placeholder being typed', () => {
    const text = 'Bearer {{tok';
    expect(findOpenPlaceholder(text, text.length)).toEqual({ start: 7, query: 'tok' });
  });

  it('fires on the bare `{{` with no query yet', () => {
    expect(findOpenPlaceholder('{{', 2)).toEqual({ start: 0, query: '' });
  });

  it('is null with no open brace or after a closed one', () => {
    expect(findOpenPlaceholder('plain value', 5)).toBeNull();
    expect(findOpenPlaceholder('{{baseUrl}}/users', 17)).toBeNull();
  });

  it('still fires with the caret inside a closed placeholder', () => {
    // caret right after `{{base`
    expect(findOpenPlaceholder('{{baseUrl}}', 6)).toEqual({ start: 0, query: 'base' });
  });

  it('does not span newlines', () => {
    expect(findOpenPlaceholder('{{a\nb', 5)).toBeNull();
  });
});

describe('filterSuggestions', () => {
  it('returns everything for an empty query', () => {
    expect(filterSuggestions(DYNAMIC_VARS, '')).toHaveLength(DYNAMIC_VARS.length);
  });

  it('matches case-insensitively and ignores a typed `$`', () => {
    const names = filterSuggestions(DYNAMIC_VARS, '$RANDOMUU').map((s) => s.name);
    expect(names).toContain('$randomUuid');
    expect(names).toContain('$randomUUID');
  });

  it('ranks prefix matches above substring matches', () => {
    const names = filterSuggestions(DYNAMIC_VARS, 'timestamp').map((s) => s.name);
    expect(names[0]).toBe('$timestamp');
    expect(names).toContain('$isoTimestamp');
    expect(names.indexOf('$timestamp')).toBeLessThan(names.indexOf('$isoTimestamp'));
  });
});

describe('applySuggestion', () => {
  it('completes a half-typed placeholder', () => {
    const text = 'Bearer {{tok';
    const open = findOpenPlaceholder(text, text.length)!;
    expect(applySuggestion(text, text.length, open, 'token')).toEqual({
      text: 'Bearer {{token}}',
      caret: 16,
    });
  });

  it('does not duplicate a closing brace already present', () => {
    const text = '{{base}}/users';
    const open = findOpenPlaceholder(text, 6)!;
    expect(applySuggestion(text, 6, open, 'baseUrl')).toEqual({
      text: '{{baseUrl}}/users',
      caret: 11,
    });
  });
});

describe('envSuggestions', () => {
  it('sorts names and tags them as env', () => {
    const out = envSuggestions({ token: 'x', baseUrl: 'y' });
    expect(out.map((s) => s.name)).toEqual(['baseUrl', 'token']);
    expect(out.every((s) => s.kind === 'env')).toBe(true);
  });
});

describe('parity with the Rust catalogue', () => {
  // Guards against the one real failure mode of mirroring a list: someone
  // adds a generator in vars.rs and the autocomplete never learns about it.
  it('lists exactly the names in argos_core::vars::BUILTIN_VARS', () => {
    // vitest runs with `apps/ui` as the working directory.
    const varsRs = resolve(process.cwd(), '../../crates/core/src/vars.rs');
    const source = readFileSync(varsRs, 'utf8');
    const from = source.indexOf('pub const BUILTIN_VARS');
    const catalogue = source.slice(from, source.indexOf('];', from));
    const rustNames = [...catalogue.matchAll(/name: "(\$[A-Za-z0-9_]+)"/g)].map((m) => m[1]);

    expect(rustNames.length).toBeGreaterThan(0);
    expect(DYNAMIC_VARS.map((v) => v.name)).toEqual(rustNames);
  });
});
