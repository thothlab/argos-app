/**
 * Vertical drag-handle sitting between sidebar and main area.
 *
 * On press → tracks pointer movement until release → updates sidebarWidth.
 * Width clamping happens inside the store; this component just feeds raw px.
 */

import { setSidebarWidth, sidebarWidth } from '~/stores/layout';

export default function SidebarResizer() {
  function onPointerDown(downEvt: PointerEvent) {
    downEvt.preventDefault();
    const startX = downEvt.clientX;
    const startWidth = sidebarWidth();

    // Visually freeze the cursor for the whole document during the drag.
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';

    const onMove = (e: PointerEvent) => {
      const dx = e.clientX - startX;
      setSidebarWidth(startWidth + dx);
    };
    const onUp = () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
  }

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize sidebar"
      tabIndex={-1}
      class="relative w-px shrink-0 cursor-col-resize bg-border hover:bg-primary"
      onPointerDown={onPointerDown}
    >
      {/* Wider invisible hit-target so the 1px line is easier to grab. */}
      <span class="absolute inset-y-0 -left-1 -right-1" />
    </div>
  );
}
