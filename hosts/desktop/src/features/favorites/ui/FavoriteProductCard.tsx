import { type ReactNode } from 'react';

import { type Product, useDotNsLabels, useProductIcon } from '@/domains/product';

import { FavoriteCard } from './FavoriteCard';

type Props = {
  product: Product;
  onOpen?: VoidFunction;
  action?: ReactNode;
};

// Resolves one product's icon, then renders the presentational card. One hook per
// component instance (not a loop), so a grid maps products to these safely.
export const FavoriteProductCard = ({ product, onOpen, action }: Props) => {
  const { data: iconUrl } = useProductIcon(product.icon);

  const labels = useDotNsLabels();
  // Prefer the manifest display name; fall back to the dotNS address.
  const title = product.displayName || labels.shortLabel(product.baseName);

  return <FavoriteCard title={title} iconUrl={iconUrl ?? undefined} action={action} onOpen={onOpen} />;
};
