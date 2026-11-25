import { useNavigate } from '@tanstack/react-router';
import { useCallback, useMemo } from 'react';

import { dashboardLayoutService, useDashboardLayouts, useFavoriteProductIds } from '@/domains/application';
import { browseService, useDotNsTld, usePublishedWidgetListings } from '@/domains/product';
import { type Product, usePersistedProducts } from '@/domains/product';
import { AddToDashboardDialogShell, useAddDashboardContent } from '@/features/dashboard';

import { AddWidgetModalProductPanel } from './add-widget/AddWidgetModalProductPanel';

type FavoriteSizeSelectorModalProps = {
  product: Product;
  isOpen: boolean;
  onClose: () => void;
};

export const FavoriteSizeSelectorModal = ({ product, isOpen, onClose }: FavoriteSizeSelectorModalProps) => {
  const navigate = useNavigate();
  const { data: products = [] } = usePersistedProducts();
  const { pages } = useDashboardLayouts();
  const { data: favoriteProductIds } = useFavoriteProductIds();
  const { data: publishedListings } = usePublishedWidgetListings(isOpen);
  const { addContent } = useAddDashboardContent();
  const { data: tld, pending: tldPending, error: tldError } = useDotNsTld();

  const handleSelectProduct = useCallback(
    (selected: Product, size: { w: number; h: number }) =>
      addContent({
        kind: 'product:widget',
        product: selected,
        size,
      }),
    [addContent],
  );

  // Top-level placement by id (content-agnostic) — preserves the prior
  // `findWidgetPlacement` semantics: a product's top-level card is its widget.
  const dashboardWidgetPlacement = useMemo(
    () => dashboardLayoutService.findCardPlacementById(pages, product.baseName),
    [pages, product.baseName],
  );

  const displayProduct = useMemo((): Product => {
    const stored = products.find(p => p.baseName === product.baseName) ?? product;
    // Every listing normalises to `<label><tld>` before it is compared, so under
    // an unsettled or failed suffix nothing matches a name from this network and
    // the panel would quietly render the unenriched product.
    if (tldPending || tldError !== null) return stored;

    const listing = browseService.findListingByBaseName(publishedListings, product.baseName, tld);

    return browseService.enrichProductWithListing(stored, listing);
  }, [product, products, publishedListings, tld, tldPending, tldError]);

  const handleNavigateToDashboardPage = useCallback(
    (pageIndex: number) => {
      onClose();
      navigate({ to: '/dashboard', search: { page: pageIndex } });
    },
    [navigate, onClose],
  );

  return (
    <AddToDashboardDialogShell isOpen={isOpen} onClose={onClose}>
      <AddWidgetModalProductPanel
        selectedProduct={displayProduct}
        favoriteProductIds={favoriteProductIds}
        dashboardWidgetPlacement={dashboardWidgetPlacement}
        onSelectProduct={handleSelectProduct}
        onNavigateToDashboardPage={handleNavigateToDashboardPage}
      />
    </AddToDashboardDialogShell>
  );
};
