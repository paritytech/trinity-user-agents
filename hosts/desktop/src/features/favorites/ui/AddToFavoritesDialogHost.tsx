import { useRxState } from '@/shared/rxstate';
import { addToFavoritesDialogOpen, closeAddToFavoritesDialog } from '../state/addToFavoritesDialog';

import { AddToFavoritesDialog } from './AddToFavoritesDialog';

// Sole owner of the Add-to-Favorites dialog, mounted persistently so the folder
// widget on the dashboard can open it while the Favorites tab is closed. Only the
// host is persistent — the dialog itself mounts per open, so its per-session state
// (search query, the already-favourite snapshot) resets without an effect to do it.
export const AddToFavoritesDialogHost = () => {
  const [isOpen] = useRxState(addToFavoritesDialogOpen);
  if (!isOpen) return null;

  return <AddToFavoritesDialog isOpen onClose={closeAddToFavoritesDialog} />;
};
