import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { type ReactNode } from 'react';

import { cnTw } from '@/shared/utils';

import { FavoriteItemCard } from './FavoriteItemCard';

type Props = {
  itemId: string;
  disabled?: boolean;
  onOpen?: VoidFunction;
  action?: ReactNode;
};

// Drag-reorderable wrapper around a favourite card. The whole card is the drag
// handle (pointer `listeners` only — no `role="button"` attributes, since the card
// already contains its own open/remove buttons). The PointerSensor's activation
// distance keeps a plain click (open) and the remove button working. `disabled`
// turns dragging off (e.g. while searching).
export const SortableFavoriteItem = ({ itemId, disabled, onOpen, action }: Props) => {
  const { listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: itemId, disabled });

  const style = {
    transform: CSS.Translate.toString(transform),
    transition,
  };

  return (
    <div ref={setNodeRef} style={style} className={cnTw('touch-none', isDragging && 'z-10 opacity-80')} {...listeners}>
      <FavoriteItemCard itemId={itemId} action={action} onOpen={onOpen} />
    </div>
  );
};
