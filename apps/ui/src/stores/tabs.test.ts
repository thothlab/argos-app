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
import { getRequest } from './request';

describe('tabs store', () => {
  it('seeds with two demo tabs and the first one active', () => {
    expect(tabs()).toHaveLength(2);
    expect(activeTab()?.title).toBe('List users');
  });

  it('seed tabs have request state initialised with their method', () => {
    const list = tabs();
    expect(getRequest(list[0]!.id)?.method).toBe('GET');
    expect(getRequest(list[1]!.id)?.method).toBe('POST');
  });

  it('selectTab() switches the active tab', () => {
    const second = tabs()[1]!;
    selectTab(second.id);
    expect(activeTabId()).toBe(second.id);
    expect(activeTab()?.title).toBe('Create user');
  });

  it('openNewTab() appends, activates, and seeds request state', () => {
    const before = tabs().length;
    openNewTab({ title: 'Smoke test', method: 'PUT' });
    expect(tabs()).toHaveLength(before + 1);
    expect(activeTab()?.title).toBe('Smoke test');
    const id = activeTabId()!;
    expect(getRequest(id)?.method).toBe('PUT');
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

  it('closeTab() removes, drops state, and picks a sensible neighbour', () => {
    const before = tabs().length;
    const id = tabs()[0]!.id;
    selectTab(id);
    closeTab(id);
    expect(tabs()).toHaveLength(before - 1);
    expect(tabs().some((t) => t.id === id)).toBe(false);
    expect(getRequest(id)).toBeNull();
    expect(activeTabId()).toBe(tabs()[0]?.id);
  });
});
