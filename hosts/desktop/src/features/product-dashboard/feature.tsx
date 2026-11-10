import { createFeature } from '@/shared/feature';
import { productManagementUseCase } from '@/aggregates/product-management';
import { persistentSlot } from '@/features/app-shell';
import {
  addToDashboardModalTransformer,
  addWidgetPanelTransformer,
  dashboardCardAddTransformer,
  folderItemContentTransformer,
  isNativeAddableDashboardId,
} from '@/features/dashboard';
import { productActionsMenuItemsSlot } from '@/features/product-actions-menu';

import { ProductAtDModalResolver } from './ui/ProductAtDModalResolver';
import { ProductDashboardMenuItem } from './ui/ProductDashboardMenuItem';
import { ProductDashboardShortcutBinding } from './ui/ProductDashboardShortcutBinding';
import { ProductFavoriteOpenBinding } from './ui/ProductFavoriteOpenBinding';
import { ProductAddWidgetCatalogBinding } from './ui/add-widget/ProductAddWidgetCatalogBinding';
import { ProductAddWidgetPanelResolver, isProductAddWidgetPanelPayload } from './ui/add-widget/ProductAddWidgetPanelResolver';
import { ProductFolderItemContent } from './ui/cards/ProductFolderItemContent';

// Integrates the product domain with the dashboard host. The dashboard exposes
// content-agnostic DI seams; this feature fills the product slice of each one
// from outside, so the dashboard feature carries no product runtime coupling.
export const productDashboardFeature = createFeature({
  name: 'product/dashboard',
});

// Product dashboard-related menu item ("Add to Dashboard" / "Add to Favorites").
productDashboardFeature.inject(productActionsMenuItemsSlot, {
  order: 10,
  render: ({ productId, closeMenu }) => <ProductDashboardMenuItem productId={productId} closeMenu={closeMenu} />,
});

// Hook-bound bindings registered through the dashboard's persistent slot
// (same host the dashboard mounts its own bindings on).
productDashboardFeature.inject(persistentSlot, () => <ProductDashboardShortcutBinding />);
productDashboardFeature.inject(persistentSlot, () => <ProductFavoriteOpenBinding />);
productDashboardFeature.inject(persistentSlot, () => <ProductAddWidgetCatalogBinding />);

// Plain handler — adding a product is a pure data op over the aggregate use case,
// no React. Claims product requests; the dashboard host / chat claim the rest.
productDashboardFeature.inject(dashboardCardAddTransformer, request =>
  'product' in request ? productManagementUseCase.addProductToDashboard(request.product, request.size) : null,
);

// Product panel for the Add-Widget modal — claims `product:widget` contributed
// entries and returns a hook-bound resolver (it needs `useDisplayedProduct`).
productDashboardFeature.inject(addWidgetPanelTransformer, ({ entry, context }) => {
  if (entry.source !== 'contributed') return null;
  if (entry.entry.kind !== 'product:widget' || !isProductAddWidgetPanelPayload(entry.entry.payload)) return null;
  return <ProductAddWidgetPanelResolver payload={entry.entry.payload} context={context} />;
});

// Product Add-to-Dashboard modal — claims every non-native target (the dashboard
// host claims native targets), returning a hook-bound resolver.
productDashboardFeature.inject(addToDashboardModalTransformer, ({ targetId, onClose }) => {
  if (isNativeAddableDashboardId(targetId)) return null;
  return <ProductAtDModalResolver targetId={targetId} onClose={onClose} />;
});

// Product favourites-folder item content — claims every non-native id (the
// dashboard host claims native ids), returning a hook-bound cell.
productDashboardFeature.inject(folderItemContentTransformer, ({ itemId, iconSize }) => {
  if (isNativeAddableDashboardId(itemId)) return null;
  return <ProductFolderItemContent itemId={itemId} iconSize={iconSize} />;
});
