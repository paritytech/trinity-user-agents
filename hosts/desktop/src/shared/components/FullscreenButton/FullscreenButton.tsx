import { Maximize2 } from 'lucide-react';
import { type MouseEvent } from 'react';

type FullscreenButtonProps = {
  onClick: (e: MouseEvent) => void;
  ariaLabel?: string;
  title?: string;
};

export const FullscreenButton = ({
  onClick,
  ariaLabel = 'Open fullscreen',
  title = 'Open fullscreen',
}: FullscreenButtonProps) => {
  return (
    <button
      className="rounded p-1 text-fg-tertiary transition-colors hover:bg-bg-action-tertiary hover:text-fg-primary"
      aria-label={ariaLabel}
      title={title}
      onClick={onClick}
    >
      <Maximize2 className="h-4 w-4" />
    </button>
  );
};
