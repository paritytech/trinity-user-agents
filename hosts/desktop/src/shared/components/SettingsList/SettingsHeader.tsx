import { Button } from '@novasamatech/tr-ui';
import { ChevronLeft } from 'lucide-react';
import { type PropsWithChildren, type ReactNode } from 'react';

import { cnTw } from '@/shared/utils';

type Props = PropsWithChildren<{
  icon?: ReactNode;
  subtitle?: ReactNode;
  onBack?: VoidFunction;
  /**
   * `page` (default) aligns the header with a content body's `px-4` (`ps-4`, 40px tall).
   * `list` matches a side-menu list header — chat's leading inset and row height (`ps-2`, 44px).
   */
  variant?: 'page' | 'list';
}>;

export const SettingsHeader = ({ icon, subtitle, onBack, variant = 'page', children }: Props) => (
  <header
    className={cnTw(
      'flex w-full shrink-0 items-center gap-2 self-stretch bg-bg-surface-container py-2 pe-2 text-fg-primary',
      variant === 'list' ? 'min-h-11 ps-2' : 'min-h-10 ps-4',
    )}
  >
    {onBack ? (
      <Button size="icon-sm" variant="ghost" onClick={onBack}>
        <ChevronLeft />
      </Button>
    ) : null}
    {icon}
    <div className="flex min-w-0 flex-1 flex-col">
      <span className="text-sm leading-5 font-semibold">{children}</span>
      {subtitle ? <span className="text-xs leading-4 text-fg-secondary">{subtitle}</span> : null}
    </div>
  </header>
);
