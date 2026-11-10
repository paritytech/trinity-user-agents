import { useDisplayedProduct } from '@/domains/product';

import { FavoriteSizeSelectorModal } from './FavoriteSizeSelectorModal';

type ProductAtDModalResolverProps = {
  targetId: string;
  onClose: () => void;
};

// Hook-bound resolver for the product Add-to-Dashboard modal. The transformer
// handler can't call hooks in its body, so it returns this component; here we
// resolve the chain product for `targetId` and render the size selector once it
// loads (nothing while pending/unresolved — identical to the prior host).
export const ProductAtDModalResolver = ({ targetId, onClose }: ProductAtDModalResolverProps) => {
  const { data: product } = useDisplayedProduct(targetId);
  if (!product) return null;
  return <FavoriteSizeSelectorModal product={product} isOpen onClose={onClose} />;
};
