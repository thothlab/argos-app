import { describe, expect, it } from 'vitest';

import {
  activeTab,
  activeTabId,
  closeTab,
  moveTab,
  openNewTab,
  selectTab,
  tabs,
  togglePin,
} from './tabs';

describe('tabs store', () => {
  it('seeds with two demo tabs and the first one active', () => {
    expect(tabs()).toHaveLength(2);
    expect(activeTab()?.title).toBe('List users');
  });

  it('selectTab() switches the active tab', () => {
    const second = tabs()[1]!;
    selectTab(second.id);
    expect(activeTabId()).toBe(second.id);
    expect(activeTab()?.title).toBe('Create user');
  });

  it('openNewTab() appends and activates the new tab', () => {
    const before = tabs().length;
    openNewTab({ title: 'Smoke test', method: 'PUT' });
    expect(tabs()).toHaveLength(before + 1);
    expect(activeTab()?.title).toBe('Smoke test');
    expect(activeTab()?.method).toBe('PUT');
    expect(activeTab()?.dirty).toBe(true);
  });

  it('togglePin() flips the pinned flag', () => {
    const id = activeTabId()!;
    const before = activeTab()?.pinned;
    togglePin(id);
    expect(activeTab()?.pinned).toBe(!before);
    togglePin(id);
    expect(activeTab()?.pinned).toBe(before);
  });

  it('moveTab() reorders within bounds', () => {
    const ordered = tabs();
    const firstId = ordered[0]!.id;
    moveTab(firstId, ordered.length - 1);
    expect(tabs().at(-1)?.id).toBe(firstId);
  });

  it('closeTab() removes and picks a sensible neighbour', () => {
    const before = tabs().length;
    const id = tabs()[0]!.id;
    selectTab(id);
    closeTab(id);
    expect(tabs()).toHaveLength(before - 1);
    expect(tabs().some((t) => t.id === id)).toBe(false);
    // Active tab moves on to the new tab at index 0 (the previous next neighbour).
    expect(activeTabId()).toBe(tabs()[0]?.id);
  });
});
