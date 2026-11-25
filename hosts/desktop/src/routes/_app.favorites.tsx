import { createFileRoute } from '@tanstack/react-router';

import { FavoritesFullscreen } from '@/features/favorites';

export const Route = createFileRoute('/_app/favorites')({
  component: FavoritesFullscreen,
});
