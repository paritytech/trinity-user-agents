import { useDisplayedProduct, useDotNsLabels, useProductIcon } from '@/domains/product';
import { type FolderItemIconSize, FolderItemCell } from '@/features/dashboard';

type Props = {
  itemId: string;
  iconSize: FolderItemIconSize;
};

// The product provider for the dashboard's `folderItemContentTransformer`.
// Resolves the favourites-folder icon+label for a product — committed row OR
// resolved from chain — so a favourite added from the browse catalog (not yet
// installed) still renders its icon+name instead of a blank cell. Renders nothing
// when `itemId` is not a resolvable product, so the transformer falls through to
// other providers.
export const ProductFolderItemContent = ({ itemId, iconSize }: Props) => {
  const { data: product } = useDisplayedProduct(itemId);
  const { data: iconUrl } = useProductIcon(product?.icon ?? null);
  const labels = useDotNsLabels();

  if (!product) return null;

  // Prefer the manifest display name; fall back to the dotNS address.
  const name = product.displayName || labels.shortLabel(product.baseName);

  return <FolderItemCell iconUrl={iconUrl ?? undefined} name={name} iconSize={iconSize} />;
};
