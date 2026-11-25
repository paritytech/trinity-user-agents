import { memo } from 'react';

import { useDisplayedProduct } from '@/domains/product';
import { ProductWorker } from '@/widgets/ProductWorker';

type Props = {
  productId: string;
};

// Resolves the product by id — committed row first, chain fallback — so one component
// serves a product whether or not it has been committed yet.
export const ResolvedProductWorker = memo(({ productId }: Props) => {
  const { data: product } = useDisplayedProduct(productId);

  if (!product?.executables.worker) return null;

  return <ProductWorker productId={productId} />;
});

ResolvedProductWorker.displayName = 'ResolvedProductWorker';
