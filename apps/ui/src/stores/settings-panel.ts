/**
 * Open/close state for the Settings panel modal. Lives separately from the
 * settings data ([[settings.ts]]) so the persisted shape stays clean.
 */

import { createSignal } from 'solid-js';

export type SettingsTab = 'appearance' | 'editor' | 'keybindings' | 'advanced';

const [open, setOpen] = createSignal(false);
const [activeTab, setActiveTab] = createSignal<SettingsTab>('appearance');

export { open as settingsOpen, activeTab as settingsActiveTab };

export function openSettings(tab: SettingsTab = 'appearance'): void {
  setActiveTab(tab);
  setOpen(true);
}

export function closeSettings(): void {
  setOpen(false);
}

export function setSettingsTab(tab: SettingsTab): void {
  setActiveTab(tab);
}
