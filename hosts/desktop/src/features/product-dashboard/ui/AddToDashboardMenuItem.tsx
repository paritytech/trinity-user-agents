import { LayoutDashboard } from 'lucide-react';

import { useTranslation } from '@/shared/translation';
import { openAddToDashboardDialog } from '@/features/dashboard';
import { MenuItem } from '@/features/product-actions-menu';

type Props = {
  productId: string;
  closeMenu: VoidFunction;
};

export const AddToDashboardMenuItem = ({ productId, closeMenu }: Props) => {
  const { t } = useTranslation();

  return (
    <MenuItem
      icon={<LayoutDashboard className="size-4" aria-hidden />}
      label={t('feature.dashboard.addToDashboard.label')}
      onSelect={() => {
        closeMenu();
        openAddToDashboardDialog(productId);
      }}
    />
  );
};
