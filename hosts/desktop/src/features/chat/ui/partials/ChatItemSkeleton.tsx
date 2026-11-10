import { useMemo } from 'react';

import { cnTw } from '@/shared/utils';
import { type ChatItemDensity } from '../../service';

type ChatItemSkeletonProps = {
  isLast?: boolean;
  density?: ChatItemDensity;
  itemHeight?: number;
};

export const ChatItemSkeleton = ({ isLast, density = 'regular', itemHeight }: ChatItemSkeletonProps) => {
  const isCompact = density === 'compact';
  const rowStyle = useMemo(() => (itemHeight != null ? { height: itemHeight } : undefined), [itemHeight]);

  return (
    <div
      style={rowStyle}
      className={cnTw(
        'relative flex w-full',
        itemHeight == null ? 'h-22 items-start' : 'items-center',
        isCompact ? 'gap-2 p-2' : 'gap-3 p-3',
      )}
    >
      <div className={cnTw('shrink-0 animate-pulse rounded-full bg-bg-action-secondary', isCompact ? 'size-12' : 'size-16')} />
      <div className="flex min-w-0 flex-1 flex-col gap-2 pt-1">
        <div className="flex w-full items-center gap-2">
          <div className="h-4 w-24 animate-pulse rounded bg-bg-action-secondary" />
          <div className="ms-auto h-3 w-12 animate-pulse rounded bg-bg-action-secondary" />
        </div>
        <div className="flex w-full flex-col gap-1.5">
          <div className="h-3 w-32 animate-pulse rounded bg-bg-action-secondary" />
          {!isCompact && <div className="h-3 w-48 animate-pulse rounded bg-bg-action-secondary" />}
        </div>
      </div>
      {!isLast && (
        <div className={cnTw('absolute bottom-0 h-px bg-stroke-secondary', isCompact ? 'start-16 end-2' : 'start-22 end-3')} />
      )}
    </div>
  );
};
