import { TabHoverTitle } from '@/shared/components';
import { useTranslation } from '@/shared/translation';

export const FavoritesTabHover = () => {
  const { t } = useTranslation();

  return <TabHoverTitle title={t('feature.favorites.title')} />;
};
