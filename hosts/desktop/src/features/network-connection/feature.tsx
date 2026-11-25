import { Globe } from 'lucide-react';

import { Sidebar } from '@/shared/components';
import { createFeature } from '@/shared/feature';
import { useTranslation } from '@/shared/translation';
import { resolveProductLoaderTransformer } from '@/features/browser';
import { settingsPreferencesNavSlot } from '@/features/settings';

import { ProductLoadScreen } from './ui/ProductLoadScreen';

export const networkConnectionFeature = createFeature({
  name: 'network/connection',
});

networkConnectionFeature.inject(resolveProductLoaderTransformer, ({ identifier }) => (
  <ProductLoadScreen identifier={identifier} />
));

networkConnectionFeature.inject(settingsPreferencesNavSlot, {
  order: 2,
  render: () => {
    const { t } = useTranslation();

    return (
      <Sidebar.Item icon={<Globe />} to="/settings/network">
        {t('feature.networkConnection.title')}
      </Sidebar.Item>
    );
  },
});
