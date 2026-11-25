import { type ReactNode } from 'react';

type ShortcutIconProps = {
  onClick: () => void;
  customIcon?: ReactNode;
  label?: string;
};

export const ShortcutIcon = ({ onClick, customIcon, label }: ShortcutIconProps) => {
  if (!customIcon) return null;

  return (
    <div
      className="relative flex h-full w-full cursor-pointer flex-col items-center justify-center gap-1 rounded-2xl"
      onClick={onClick}
    >
      {customIcon}
      {label && <span className="truncate text-xs text-fg-secondary">{label}</span>}
    </div>
  );
};
