import { Import } from 'lucide-react';

import RemoveOfflineIcon from '@/shared/assets/images/remove-offline.svg?jsx';
import { TEST_IDS } from '@/shared/test-ids';
import { useTranslation } from '@/shared/translation';
import { useIsPinned } from '@/domains/product';
import { MenuItem } from '@/features/product-actions-menu';
import { openOfflineAccessDialog } from '../state/dialogState';

type Props = {
  productId: string;
  closeMenu: VoidFunction;
};

export const OfflineAccessMenuItem = ({ productId, closeMenu }: Props) => {
  const { t } = useTranslation();
  const pinned = useIsPinned(productId);

  return (
    <MenuItem
      testId={TEST_IDS.offlineAccessMenuItem}
      icon={pinned ? <RemoveOfflineIcon className="size-4" aria-hidden /> : <Import className="size-4" aria-hidden />}
      label={pinned ? t('feature.offlineAccess.menu.remove') : t('feature.offlineAccess.menu.enable')}
      onSelect={() => {
        openOfflineAccessDialog({ kind: pinned ? 'remove' : 'enable', productId });
        closeMenu();
      }}
    />
  );
};
