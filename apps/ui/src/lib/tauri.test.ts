import { describe, it, expect } from 'vitest';

import { isTauri } from './tauri';

describe('isTauri', () => {
  it('returns false in plain Node/jsdom (no Tauri runtime)', () => {
    expect(isTauri()).toBe(false);
  });
});
