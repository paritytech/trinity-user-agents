import { createState } from '@/shared/rxstate';

// Open/closed only — the picker lists the whole catalog, so unlike
// `createDialogTarget` there is no target to carry. `AddToFavoritesDialogHost`
// owns the single instance; the fullscreen SPA and the dashboard folder widget
// both drive it through this state rather than each mounting their own dialog.
const isOpen = createState(false);

export const addToFavoritesDialogOpen = isOpen;
export const openAddToFavoritesDialog = () => isOpen.set(true);
export const closeAddToFavoritesDialog = () => isOpen.set(false);
