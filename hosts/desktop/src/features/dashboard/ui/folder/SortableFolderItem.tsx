import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { type PropsWithChildren } from 'react';

import { cnTw } from '@/shared/utils';

type Props = PropsWithChildren<{
  itemId: string;
}>;

// Drag-reorder wrapper around one folder cell. The whole cell is the drag handle
// (pointer `listeners` only) — the PointerSensor's activation distance keeps a
// plain click (open) and the cell's own menu button working. Fills its grid cell,
// since the parent grid fits its rows to the card body.
export const SortableFolderItem = ({ itemId, children }: Props) => {
  const { listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: itemId });

  const style = {
    transform: CSS.Translate.toString(transform),
    transition,
  };

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={cnTw('h-full w-full touch-none', isDragging && 'z-10 opacity-80')}
      {...listeners}
    >
      {children}
    </div>
  );
};
