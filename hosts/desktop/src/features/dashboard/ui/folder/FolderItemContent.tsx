import { useTransformer } from '@/shared/di';
import { folderItemContentTransformer } from '../../di';
import { type FolderItemIconSize } from '../../types';

type Props = {
  itemId: string;
  iconSize: FolderItemIconSize;
};

// One favourites-folder cell's icon+label, resolved through the content seam.
// `useTransformer` runs once per cell instance (one hook per component, not a
// loop), so provider handlers can return hook-bound nodes. Renders nothing when
// no provider claims the id.
export const FolderItemContent = ({ itemId, iconSize }: Props) => {
  const content = useTransformer(folderItemContentTransformer, { itemId, iconSize });

  return content ?? null;
};
