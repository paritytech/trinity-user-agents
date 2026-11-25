import { useCallback } from 'react';

import { type AddWidgetPanelContext, type AddableDashboardCard } from '../../di';
import { useAddDashboardContent } from '../../hooks/useAddDashboardContent';

import { AddWidgetModalNativePanel } from './AddWidgetModalNativePanel';

type NativeAddWidgetPanelResolverProps = {
  entry: AddableDashboardCard;
  context: AddWidgetPanelContext;
};

// Native panel returned by the dashboard's own `addWidgetPanelTransformer` handler.
// Self-sources the uniform add dispatch (mirrors the prior `Dashboard.tsx`
// `handleAddNativeCard`/`handleAddNativeToFavorites`) so the modal threads only the
// content-agnostic `AddWidgetPanelContext` and never the per-kind add callbacks.
export const NativeAddWidgetPanelResolver = ({ entry, context }: NativeAddWidgetPanelResolverProps) => {
  const { dashboardPages, favoriteProductIds, onNavigateToDashboardPage } = context;
  const { addContent } = useAddDashboardContent();

  const handleAddNativeCard = useCallback(
    (target: AddableDashboardCard, size: { w: number; h: number }) => addContent({ kind: target.kind, entry: target, size }),
    [addContent],
  );

  const handleAddNativeToFavorites = useCallback(
    (target: AddableDashboardCard) => addContent({ kind: target.kind, entry: target, size: { w: 1, h: 1 } }),
    [addContent],
  );

  return (
    <AddWidgetModalNativePanel
      entry={entry}
      dashboardPages={dashboardPages}
      favorites={{ ids: favoriteProductIds, onAdd: handleAddNativeToFavorites }}
      onAddNativeCard={handleAddNativeCard}
      onNavigateToDashboardPage={onNavigateToDashboardPage}
    />
  );
};
