import { createSignal } from 'solid-js';

const [paletteOpen, setPaletteOpen] = createSignal(false);

export { paletteOpen };

export function openPalette(): void {
  setPaletteOpen(true);
}

export function closePalette(): void {
  setPaletteOpen(false);
}

export function togglePalette(): void {
  setPaletteOpen((v) => !v);
}
