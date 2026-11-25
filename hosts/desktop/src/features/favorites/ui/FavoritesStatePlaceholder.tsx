import { Empty } from '@novasamatech/tr-ui';
import { type ReactNode } from 'react';

import { cnTw } from '@/shared/utils';

type Props = {
  icon: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
  testId?: string;
  className?: string;
};

// Shared centered empty/error/no-results placeholder for the Favorites surfaces
// (the Add-to-Favorites dialog and the fullscreen SPA). Thin presentational wrapper
// over the tr-ui `Empty` compound so all states share one layout and icon frame.
export const FavoritesStatePlaceholder = ({ icon, title, description, action, testId, className }: Props) => (
  <div data-testid={testId} className={cnTw('flex min-h-40 flex-1 items-center justify-center px-3 py-10', className)}>
    <Empty>
      <Empty.Header>
        <Empty.Media variant="icon">{icon}</Empty.Media>
        <Empty.Title>{title}</Empty.Title>
        {description ? <Empty.Description>{description}</Empty.Description> : null}
      </Empty.Header>
      {action ? <Empty.Content>{action}</Empty.Content> : null}
    </Empty>
  </div>
);
