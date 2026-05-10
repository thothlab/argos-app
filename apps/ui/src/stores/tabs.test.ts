import { describe, expect, it } from 'vitest';

import {
  activeTab,
  activeTabId,
  closeAllTabs,
  closeTab,
  moveTab,
  openNewTab,
  selectTab,
  tabs,
  togglePin,
} from './tabs';
import { getRequest } from './request';

describe('tabs store', () => {
  it('starts empty (no workspace seed)', () => {
    closeAllTabs();
    expect(tabs()).toHaveLength(0);
    expect(activeTabId()).toBeNull();
  });

  it('openNewTab() appends, activates, and seeds request state', () => {
    closeAllTabs();
    openNewTab({ title: 'Smoke test', method: 'PUT' });
    expect(tabs()).toHaveLength(1);
    expect(activeTab()?.title).toBe('Smoke test');
    const id = activeTabId()!;
    expect(getRequest(id)?.method).toBe('PUT');
  });

  it('selectTab() switches the active tab', () => {
    closeAllTabs();
    openNewTab({ title: 'one' });
    openNewTab({ title: 'two' });
    const first = tabs()[0]!;
    selectTab(first.id);
    expect(activeTab()?.title).toBe('one');
  });

  it('togglePin() flips the pinned flag', () => {
    closeAllTabs();
    openNewTab({ title: 'pinme' });
    const id = activeTabId()!;
    expect(activeTab()?.pinned).toBe(false);
    togglePin(id);
    expect(activeTab()?.pinned).toBe(true);
    togglePin(id);
    expect(activeTab()?.pinned).toBe(false);
  });

  it('moveTab() reorders within bounds', () => {
    closeAllTabs();
    openNewTab({ title: 'a' });
    openNewTab({ title: 'b' });
    openNewTab({ title: 'c' });
    const firstId = tabs()[0]!.id;
    moveTab(firstId, tabs().length - 1);
    expect(tabs().at(-1)?.id).toBe(firstId);
  });

  it('closeTab() removes, drops state, and picks a sensible neighbour', () => {
    closeAllTabs();
    openNewTab({ title: 'a' });
    openNewTab({ title: 'b' });
    const firstId = tabs()[0]!.id;
    selectTab(firstId);
    closeTab(firstId);
    expect(tabs()).toHaveLength(1);
    expect(getRequest(firstId)).toBeNull();
    expect(activeTabId()).toBe(tabs()[0]?.id);
  });
});
