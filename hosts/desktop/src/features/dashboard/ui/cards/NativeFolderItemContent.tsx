import { useTranslation } from '@/shared/translation';
import { useAddableDashboardCards } from '../../hooks/useAddableDashboardCards';
import { type FolderItemIconSize } from '../../types';
import { FolderItemCell } from '../folder/FolderItemCell';

type Props = {
  itemId: string;
  iconSize: FolderItemIconSize;
};

// TEMPORARY (native stays the dashboard's own concern until natives are
// confirmed): the native provider for `folderItemContentTransformer`. Resolves
// the favourites-folder icon+label for a native catalog entry (e.g. chat) from
// the addable-cards catalog, reusing the entry's own icon so the favourite
// matches the Add Widget modal. Renders nothing when `itemId` isn't a native
// entry, so the transformer falls through to other providers.
export const NativeFolderItemContent = ({ itemId, iconSize }: Props) => {
  const { t } = useTranslation();
  const { byGridId } = useAddableDashboardCards();
  const entry = byGridId.get(itemId);

  if (!entry) return null;

  return <FolderItemCell iconNode={entry.icon} name={t(entry.displayNameKey)} iconSize={iconSize} />;
};
