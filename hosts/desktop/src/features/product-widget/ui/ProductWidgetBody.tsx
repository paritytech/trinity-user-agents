import { memo, useState } from 'react';

import { WidgetLoadingScreen, WidgetPlaceholder } from '@/shared/components';
import { useSideEffect } from '@/shared/di';
import { TEST_IDS } from '@/shared/test-ids';
import { useTranslation } from '@/shared/translation';
import { type Product, productService } from '@/domains/product';
import { onProductRefreshRequestedSideEffect } from '@/aggregates/product-loading';
import { Webview } from '@/widgets/Webview';
import { useAnnounceWidgetOpen } from '../hooks/useAnnounceWidgetOpen';

type Props = {
  productId: string;
  // Resolved product + whether its archive loaded + loading, owned by
  // `ProductWidgetContent` (the single source — it also drives the block pulse
  // via the chrome's `isLoading`). The webview re-resolves the archive itself,
  // so the body only needs to know that content exists, not its shape.
  product: Product | null;
  hasContent: boolean;
  pending: boolean;
  onRemoveCard: VoidFunction;
};

// The card body for a product widget — mounts a webview against the resolved
// widget executable's archive. The surrounding chrome (topbar, menu, actions)
// lives in `DashboardCardChrome`; this component is only the body. Resolution
// and loading come in as props so there is one source of loading truth.
export const ProductWidgetBody = memo(({ productId, product, hasContent, pending, onRemoveCard }: Props) => {
  const { t } = useTranslation();
  const [refreshKey, setRefreshKey] = useState(0);

  useSideEffect(onProductRefreshRequestedSideEffect, ({ identifier }) => {
    if (productService.refreshTargetIdentifiers(productId, product).has(identifier)) {
      setRefreshKey(prev => prev + 1);
    }
  });

  useAnnounceWidgetOpen(productId);

  // When settled (not loading) with no product, the widget was removed on-chain.
  if (!pending && !product) {
    return (
      <WidgetPlaceholder
        message={t('feature.dashboard.placeholder.widgetNotFound')}
        actionLabel={t('feature.dashboard.placeholder.deleteWidget')}
        onAction={onRemoveCard}
      />
    );
  }

  if (!pending && !hasContent) {
    return (
      <WidgetPlaceholder
        testId={TEST_IDS.productWidgetNotFound}
        message={t('feature.dashboard.placeholder.widgetUnavailable')}
        actionLabel={t('common.action.retry')}
        onAction={() => void onProductRefreshRequestedSideEffect.apply({ identifier: productId })}
      />
    );
  }

  // Phase-1 loading renders nothing — the whole block pulses via
  // DashboardCardChrome. The webview `loader` (below) covers phase-2 page load.
  if (pending) {
    return null;
  }

  return (
    <div className="relative h-full w-full overflow-hidden">
      <Webview
        key={`${productId}-${refreshKey}`}
        identifier={productId}
        kind="widget"
        loader={<WidgetLoadingScreen />}
        visible={true}
      />
    </div>
  );
});
