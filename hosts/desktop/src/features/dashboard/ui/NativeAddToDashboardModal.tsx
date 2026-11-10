import { useNavigate } from '@tanstack/react-router';
import { useCallback } from 'react';

import { useDashboardLayouts, useFavoriteProductIds } from '@/domains/application';
import { type AddableDashboardCard } from '../di';
import { useAddDashboardContent } from '../hooks/useAddDashboardContent';

import { AddToDashboardDialogShell } from './AddToDashboardDialogShell';
import { AddWidgetModalNativePanel } from './add-widget/AddWidgetModalNativePanel';

type NativeAddToDashboardModalProps = {
  entry: AddableDashboardCard;
  isOpen: boolean;
  onClose: () => void;
};

// AtD dialog branch for a native addable entry (e.g. chat) that has no chain
// `Product`. Mirrors `FavoriteSizeSelectorModal`; native add paths go through the
// uniform add dispatch (shared with the catalog modal in `Dashboard`).
export const NativeAddToDashboardModal = ({ entry, isOpen, onClose }: NativeAddToDashboardModalProps) => {
  const navigate = useNavigate();
  const { pages } = useDashboardLayouts();
  const { data: favoriteProductIds } = useFavoriteProductIds();
  const { addContent } = useAddDashboardContent();

  const handleAddNativeCard = useCallback(
    (card: AddableDashboardCard, size: { w: number; h: number }) => addContent({ kind: card.kind, entry: card, size }),
    [addContent],
  );

  const handleAddNativeToFavorites = useCallback(
    (card: AddableDashboardCard) => addContent({ kind: card.kind, entry: card, size: { w: 1, h: 1 } }),
    [addContent],
  );

  const handleNavigateToDashboardPage = useCallback(
    (pageIndex: number) => {
      onClose();
      navigate({ to: '/dashboard', search: { page: pageIndex } });
    },
    [navigate, onClose],
  );

  return (
    <AddToDashboardDialogShell isOpen={isOpen} onClose={onClose}>
      <AddWidgetModalNativePanel
        entry={entry}
        dashboardPages={pages}
        favorites={{ ids: favoriteProductIds, onAdd: handleAddNativeToFavorites }}
        onAddNativeCard={handleAddNativeCard}
        onNavigateToDashboardPage={handleNavigateToDashboardPage}
      />
    </AddToDashboardDialogShell>
  );
};
