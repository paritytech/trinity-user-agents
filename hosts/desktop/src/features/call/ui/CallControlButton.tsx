import { type ReactNode } from 'react';

import { cnTw } from '@/shared/utils';

type CallControlButtonProps = {
  label: string;
  tone: 'neutral' | 'accept' | 'decline' | 'primary' | 'secondary';
  icon: ReactNode;
  size?: 'md' | 'lg';
  active?: boolean;
  onPress: VoidFunction;
};

// Each tone carries its circle fill and the icon colour that reads on it.
const toneStyles: Record<CallControlButtonProps['tone'], string> = {
  neutral: cnTw('bg-white/20 text-white hover:bg-white/30'),
  accept: cnTw('bg-green-500 text-white hover:bg-green-600'),
  decline: cnTw('bg-bg-status-error text-white hover:bg-bg-status-error-hover'),
  primary: cnTw('bg-bg-action-primary text-fg-primary-inverted hover:bg-bg-action-primary-hover'),
  secondary: cnTw('bg-bg-action-secondary text-fg-primary hover:bg-bg-action-secondary-hover'),
};

const sizeStyles: Record<NonNullable<CallControlButtonProps['size']>, string> = {
  md: cnTw('size-14'),
  lg: cnTw('size-16'),
};

export const CallControlButton = ({ label, tone, icon, size = 'md', active = true, onPress }: CallControlButtonProps) => (
  <div className="flex flex-col items-center gap-2">
    <button
      type="button"
      aria-label={label}
      className={cnTw(
        'flex items-center justify-center rounded-full transition-colors',
        sizeStyles[size],
        toneStyles[tone],
        !active && 'opacity-50',
      )}
      onClick={onPress}
    >
      {icon}
    </button>
    <span className="text-sm font-medium text-fg-primary">{label}</span>
  </div>
);
