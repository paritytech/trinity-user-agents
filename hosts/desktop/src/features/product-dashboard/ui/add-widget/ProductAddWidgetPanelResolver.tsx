import { useCallback, useMemo } from 'react';

import { dashboardLayoutService } from '@/domains/application';
import { type AppListing, type Product, browseService, useDisplayedProduct } from '@/domains/product';
import { type AddWidgetPanelContext, useAddDashboardContent } from '@/features/dashboard';

import { AddWidgetModalProductPanel } from './AddWidgetModalProductPanel';

export type ProductAddWidgetPanelPayload = { baseName: string; listing: AppListing };

// Structural narrow of the contributed entry's opaque payload to the product
// shape, so the transformer handler claims only its own entries without an `as`.
export function isProductAddWidgetPanelPayload(payload: unknown): payload is ProductAddWidgetPanelPayload {
  return (
    typeof payload === 'object' &&
    payload !== null &&
    'baseName' in payload &&
    typeof payload.baseName === 'string' &&
    'listing' in payload
  );
}

type ProductAddWidgetPanelResolverProps = {
  payload: ProductAddWidgetPanelPayload;
  context: AddWidgetPanelContext;
};

// Hook-bound product panel returned by the dashboard's `addWidgetPanelTransformer`
// handler — the transformer body can't call hooks, so it returns this component.
// Resolves the
// chain product for the entry's baseName, enriches it with the browse listing
// (identical to the prior inline `PublishedWidgetPanel`), and dispatches the add
// through the uniform `addContent` transformer (size 1×1 → favourite, else widget).
export const ProductAddWidgetPanelResolver = ({ payload, context }: ProductAddWidgetPanelResolverProps) => {
  const { listing, baseName } = payload;
  const { dashboardPages, favoriteProductIds, onNavigateToDashboardPage } = context;
  const { addContent } = useAddDashboardContent();

  // Identity comes from the payload, which the catalog binding built under a
  // settled TLD; re-deriving it here would reintroduce the fallback suffix into
  // the id `addContent` persists.
  const previewProduct = useMemo(() => browseService.productPreviewFromListing(listing, baseName), [listing, baseName]);
  const { data: resolvedProduct } = useDisplayedProduct(baseName);
  const product = useMemo(() => {
    const base = resolvedProduct ?? previewProduct;
    return browseService.enrichProductWithListing(base, listing);
  }, [listing, previewProduct, resolvedProduct]);

  // Folder-aware: a product already living inside a user folder counts as
  // "on the dashboard" so the panel offers "Open" instead of "Add" — adding it
  // again would create a duplicate top-level widget alongside the folder entry.
  const dashboardWidgetPlacement = dashboardLayoutService.findProductDashboardPlacement(dashboardPages, product.baseName);

  const handleSelectProduct = useCallback(
    (selected: Product, size: { w: number; h: number }) =>
      addContent({
        kind: 'product:widget',
        product: selected,
        size,
      }),
    [addContent],
  );

  return (
    <AddWidgetModalProductPanel
      selectedProduct={product}
      favoriteProductIds={favoriteProductIds}
      dashboardWidgetPlacement={dashboardWidgetPlacement}
      onSelectProduct={handleSelectProduct}
      onNavigateToDashboardPage={onNavigateToDashboardPage}
    />
  );
};
