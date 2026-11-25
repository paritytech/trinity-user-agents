import { type ReactNode } from 'react';

import { useDisplayedProduct, useDotNsLabels, useProductIcon } from '@/domains/product';

import { FavoriteCard } from './FavoriteCard';

type Props = {
  itemId: string;
  onOpen?: VoidFunction;
  action?: ReactNode;
};

// A favourite rendered from its stored id (baseName). The title comes straight
// from the id (no resolution needed); the icon is resolved via `useDisplayedProduct`
// (committed row OR chain), so a catalog-added favourite that isn't installed still
// shows its icon instead of a blank tile. One hook per component (not a loop).
export const FavoriteItemCard = ({ itemId, onOpen, action }: Props) => {
  const { data: product } = useDisplayedProduct(itemId);
  const { data: iconUrl } = useProductIcon(product?.icon ?? null);

  const labels = useDotNsLabels();
  // Prefer the manifest display name; fall back to the dotNS address while the
  // product is still resolving (or has no name).
  const title = product?.displayName || labels.shortLabel(itemId);

  return <FavoriteCard title={title} iconUrl={iconUrl ?? undefined} action={action} onOpen={onOpen} />;
};
