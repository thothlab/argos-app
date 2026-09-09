import { describe, expect, it } from 'vitest';

import { splitShellArgs } from './git-console';

describe('splitShellArgs', () => {
  it('splits on whitespace', () => {
    expect(splitShellArgs('status --short')).toEqual({ ok: true, args: ['status', '--short'] });
  });

  it('drops a leading literal `git`', () => {
    expect(splitShellArgs('git log -5')).toEqual({ ok: true, args: ['log', '-5'] });
  });

  it('keeps a quoted run of spaces together', () => {
    expect(splitShellArgs('commit -m "two words"')).toEqual({
      ok: true,
      args: ['commit', '-m', 'two words'],
    });
    expect(splitShellArgs("commit -m 'two words'")).toEqual({
      ok: true,
      args: ['commit', '-m', 'two words'],
    });
  });

  it('keeps an empty quoted argument', () => {
    expect(splitShellArgs('commit -m ""')).toEqual({ ok: true, args: ['commit', '-m', ''] });
  });

  it('honours backslash escapes outside single quotes', () => {
    expect(splitShellArgs('add a\\ b.txt')).toEqual({ ok: true, args: ['add', 'a b.txt'] });
    expect(splitShellArgs("add 'a\\ b.txt'")).toEqual({ ok: true, args: ['add', 'a\\ b.txt'] });
  });

  it('reports an unmatched quote instead of guessing', () => {
    expect(splitShellArgs('commit -m "unfinished')).toEqual({
      ok: false,
      error: 'unmatched " quote',
    });
  });

  it('returns no args for blank input', () => {
    expect(splitShellArgs('   ')).toEqual({ ok: true, args: [] });
  });
});
