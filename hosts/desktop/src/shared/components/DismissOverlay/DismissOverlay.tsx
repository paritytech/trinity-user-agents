import { createPortal } from 'react-dom';

type Props = {
  open: boolean;
  onDismiss: VoidFunction;
  testId?: string;
};

/**
 * Transparent, full-window overlay that dismisses an open layer (popover/menu)
 * on the first outside press.
 *
 * It exists because non-modal Radix popovers detect outside clicks via a
 * document `pointerdown`, which never fires for presses on a
 * `-webkit-app-region: drag` region (Electron hands those to the OS for window
 * dragging). This overlay is an explicit `no-drag` element painted on top of
 * the drag region, so the press lands in the renderer and `onPointerDown`
 * closes the layer — independent of which events the drag region emits.
 *
 * Rendered in a portal at `z-40`: below Radix popover content (`z-50`, so the
 * layer stays interactive) and above the toolbar (so toolbar presses hit it).
 */
export const DismissOverlay = ({ open, onDismiss, testId }: Props) => {
  if (!open) return null;

  return createPortal(
    <div
      aria-hidden
      data-testid={testId}
      className="fixed inset-0 z-40"
      style={{ appRegion: 'no-drag' }}
      onPointerDown={onDismiss}
    />,
    document.body,
  );
};
