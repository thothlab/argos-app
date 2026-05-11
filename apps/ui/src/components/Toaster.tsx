/**
 * Toast region — mounted once at app root. Other code fires
 * notifications via the helpers in `lib/toast.ts`.
 *
 * Kobalte owns stacking, focus management, the `aria-live` region
 * and auto-dismiss timing; we only style the surface.
 */

import { Toast } from '@kobalte/core/toast';
import { X } from 'lucide-solid';
import { Portal } from 'solid-js/web';

export default function Toaster() {
  return (
    <Portal>
      <Toast.Region duration={4500} swipeDirection="right">
        <Toast.List class="fixed bottom-4 right-4 z-[60] flex max-h-screen w-96 flex-col gap-2 outline-none" />
      </Toast.Region>
    </Portal>
  );
}

/**
 * Render a single toast — wired up by `toaster.show()` callbacks.
 * Variant drives the accent stripe + icon colour; everything else is
 * shared layout.
 */
export function ToastView(props: {
  toastId: number;
  variant: 'info' | 'success' | 'error';
  title: string;
  description?: string;
}) {
  const accent =
    props.variant === 'error'
      ? 'border-l-fg-error'
      : props.variant === 'success'
        ? 'border-l-fg-success'
        : 'border-l-primary';

  return (
    <Toast
      toastId={props.toastId}
      class={`pointer-events-auto flex items-start gap-3 overflow-hidden rounded-md border border-border border-l-4 bg-bg-card px-4 py-3 shadow-lg ${accent}`}
    >
      <div class="min-w-0 flex-1">
        <Toast.Title class="text-[13px] font-semibold">{props.title}</Toast.Title>
        {props.description ? (
          <Toast.Description class="mt-0.5 text-[12px] text-fg-secondary">
            {props.description}
          </Toast.Description>
        ) : null}
      </div>
      <Toast.CloseButton class="shrink-0 rounded p-1 text-fg-secondary hover:bg-bg-secondary hover:text-fg-primary">
        <X size={13} />
      </Toast.CloseButton>
    </Toast>
  );
}
